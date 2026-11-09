//! Audio bridge between OpenAudioPower and `oap-plughost`: one shared-memory block per
//! plug-in instance plus two auto-reset events. Shared verbatim by both sides
//! (`#[path]`-included by the app), so the layout cannot drift.
//!
//! Layout (all little-endian, 4-byte aligned):
//! ```text
//! Header | in[0][MAX_BLOCK] | in[1][MAX_BLOCK] | out[0][MAX_BLOCK] | out[1][MAX_BLOCK]
//! ```
//! The app writes the input, bumps `seq_in` and signals `<name>-in`; the host processes,
//! copies `seq_in` to `seq_out` and signals `<name>-out`. A late answer from an earlier,
//! timed-out block is recognised by its stale sequence number.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{CreateEventW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_ALL_ACCESS};

pub const MAGIC: u32 = 0x4F41_5031; // "OAP1"
pub const MAX_BLOCK: usize = 8192;
pub const CHANNELS: usize = 2;

/// Header flags.
pub const FLAG_RESET: u32 = 1;
pub const FLAG_BYPASS: u32 = 2;

#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub seq_in: AtomicU32,
    pub seq_out: AtomicU32,
    pub frames: AtomicU32,
    pub flags: AtomicU32,
    /// latency the plug-in reports, in frames (written by the host)
    pub latency: AtomicU32,
    pub _pad: [u32; 2],
}

pub const SIZE: usize = std::mem::size_of::<Header>() + 4 * CHANNELS * MAX_BLOCK * std::mem::size_of::<f32>();

pub struct Shared {
    map: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    pub ev_in: HANDLE,
    pub ev_out: HANDLE,
}

unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Shared {
    /// App side: creates the block and the events.
    pub fn create(name: &str) -> Result<Self, String> {
        unsafe {
            let map = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, SIZE as u32, &HSTRING::from(name)).map_err(|e| format!("shm: {e}"))?;
            let view = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, SIZE);
            if view.Value.is_null() {
                let _ = CloseHandle(map);
                return Err("shm: map failed".into());
            }
            std::ptr::write_bytes(view.Value as *mut u8, 0, SIZE);
            (*(view.Value as *mut Header)).magic = MAGIC;
            let ev_in = CreateEventW(None, false, false, &HSTRING::from(format!("{name}-in"))).map_err(|e| format!("event: {e}"))?;
            let ev_out = CreateEventW(None, false, false, &HSTRING::from(format!("{name}-out"))).map_err(|e| format!("event: {e}"))?;
            Ok(Self { map, view, ev_in, ev_out })
        }
    }

    /// Host side: opens what the app created.
    pub fn open(name: &str) -> Result<Self, String> {
        unsafe {
            let map = OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, &HSTRING::from(name)).map_err(|e| format!("shm: {e}"))?;
            let view = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, SIZE);
            if view.Value.is_null() {
                let _ = CloseHandle(map);
                return Err("shm: map failed".into());
            }
            if (*(view.Value as *const Header)).magic != MAGIC {
                return Err("shm: bad magic".into());
            }
            let ev_in = OpenEventW(EVENT_ALL_ACCESS, false, &HSTRING::from(format!("{name}-in"))).map_err(|e| format!("event: {e}"))?;
            let ev_out = OpenEventW(EVENT_ALL_ACCESS, false, &HSTRING::from(format!("{name}-out"))).map_err(|e| format!("event: {e}"))?;
            Ok(Self { map, view, ev_in, ev_out })
        }
    }

    pub fn header(&self) -> &Header {
        unsafe { &*(self.view.Value as *const Header) }
    }

    fn base(&self) -> *mut f32 {
        unsafe { (self.view.Value as *mut u8).add(std::mem::size_of::<Header>()) as *mut f32 }
    }

    /// Channel `c` of the input (`out = false`) or output buffer.
    ///
    /// # Safety
    /// The two sides take turns through the sequence numbers; only the side whose turn it is
    /// touches the buffers.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn buf(&self, out: bool, c: usize) -> &mut [f32] {
        let k = if out { CHANNELS + c } else { c };
        std::slice::from_raw_parts_mut(self.base().add(k * MAX_BLOCK), MAX_BLOCK)
    }

    pub fn signal(&self, ev: HANDLE) {
        unsafe {
            let _ = SetEvent(ev);
        }
    }

    /// Waits for `ev`; true when it was signalled within `ms`.
    pub fn wait(&self, ev: HANDLE, ms: u32) -> bool {
        unsafe { WaitForSingleObject(ev, ms) == WAIT_OBJECT_0 }
    }

    pub fn seq_in(&self) -> u32 {
        self.header().seq_in.load(Ordering::Acquire)
    }
    pub fn seq_out(&self) -> u32 {
        self.header().seq_out.load(Ordering::Acquire)
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.map);
            let _ = CloseHandle(self.ev_in);
            let _ = CloseHandle(self.ev_out);
        }
    }
}
