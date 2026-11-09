//! Minimal CLAP host: loads a `.clap` library, creates one plug-in, runs it as a stereo
//! insert effect and shows its editor in a window of ours.
//!
//! Threads follow the CLAP rules: everything except `start_processing` / `process` /
//! `stop_processing` is called on the host's main thread.

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{clap_event_header, clap_input_events, clap_output_events};
use clap_sys::ext::audio_ports::{clap_audio_port_info, clap_plugin_audio_ports, CLAP_EXT_AUDIO_PORTS};
use clap_sys::ext::gui::{clap_host_gui, clap_plugin_gui, clap_window, clap_window_handle, CLAP_EXT_GUI, CLAP_WINDOW_API_WIN32};
use clap_sys::ext::latency::{clap_host_latency, clap_plugin_latency, CLAP_EXT_LATENCY};
use clap_sys::ext::log::{clap_host_log, clap_log_severity, CLAP_EXT_LOG};
use clap_sys::ext::params::{clap_host_params, clap_param_clear_flags, clap_param_rescan_flags, CLAP_EXT_PARAMS};
use clap_sys::ext::state::{clap_host_state, clap_plugin_state, CLAP_EXT_STATE};
use clap_sys::ext::thread_check::{clap_host_thread_check, CLAP_EXT_THREAD_CHECK};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::id::clap_id;
use clap_sys::plugin::clap_plugin;
use clap_sys::process::clap_process;
use clap_sys::stream::{clap_istream, clap_ostream};
use clap_sys::version::{clap_version_is_compatible, CLAP_VERSION};
use parking_lot::Mutex;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::PluginDesc;

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

// ───────────────────────────── library ─────────────────────────────

pub struct ClapLib {
    entry: *const clap_plugin_entry,
    _lib: libloading::Library,
}

unsafe impl Send for ClapLib {}
unsafe impl Sync for ClapLib {}

impl ClapLib {
    pub fn load(path: &Path) -> Result<Arc<Self>, String> {
        unsafe {
            let lib = libloading::Library::new(path).map_err(|e| format!("load: {e}"))?;
            let entry = *lib.get::<*const clap_plugin_entry>(b"clap_entry\0").map_err(|e| format!("clap_entry: {e}"))?;
            if entry.is_null() || !clap_version_is_compatible((*entry).clap_version) {
                return Err("incompatible CLAP version".into());
            }
            let p = CString::new(path.to_string_lossy().as_bytes()).unwrap_or_default();
            if !(*entry).init.map(|f| f(p.as_ptr())).unwrap_or(false) {
                return Err("clap_entry.init failed".into());
            }
            Ok(Arc::new(Self { entry, _lib: lib }))
        }
    }

    fn factory(&self) -> Option<&clap_plugin_factory> {
        unsafe {
            let f = (*self.entry).get_factory?(CLAP_PLUGIN_FACTORY_ID.as_ptr()) as *const clap_plugin_factory;
            f.as_ref()
        }
    }

    pub fn descriptors(&self, path: &Path) -> Vec<PluginDesc> {
        let Some(f) = self.factory() else { return Vec::new() };
        let mut out = Vec::new();
        unsafe {
            let n = f.get_plugin_count.map(|g| g(f)).unwrap_or(0);
            for i in 0..n {
                let Some(d) = f.get_plugin_descriptor.and_then(|g| g(f, i).as_ref()) else { continue };
                let mut features = Vec::new();
                if !d.features.is_null() {
                    let mut k = 0;
                    while !(*d.features.add(k)).is_null() {
                        features.push(cstr(*d.features.add(k)));
                        k += 1;
                    }
                }
                let instrument = features.iter().any(|x| x == "instrument");
                out.push(PluginDesc {
                    format: "clap".into(),
                    path: path.to_string_lossy().into_owned(),
                    id: cstr(d.id),
                    name: cstr(d.name),
                    vendor: cstr(d.vendor),
                    version: cstr(d.version),
                    category: features.join("|"),
                    instrument,
                });
            }
        }
        out
    }
}

impl Drop for ClapLib {
    fn drop(&mut self) {
        unsafe {
            if let Some(d) = (*self.entry).deinit {
                d();
            }
        }
    }
}

// ───────────────────────────── host callbacks ─────────────────────────────

/// State the plug-in can poke through the host interface (any thread).
#[derive(Default)]
pub struct HostData {
    pub resize: Mutex<Option<(u32, u32)>>,
    pub callback: AtomicBool,
    pub closed: AtomicBool,
    pub dirty: AtomicBool,
    pub main_thread: AtomicU64,
    pub audio_thread: AtomicU64,
}

fn thread_key() -> u64 {
    // ThreadId has no stable integer; hash it
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut h);
    h.finish() | 1
}

unsafe fn data<'a>(host: *const clap_host) -> &'a HostData {
    &*((*host).host_data as *const HostData)
}

unsafe extern "C" fn get_extension(_host: *const clap_host, id: *const c_char) -> *const c_void {
    let id = CStr::from_ptr(id);
    if id == CLAP_EXT_GUI {
        &HOST_GUI as *const _ as *const c_void
    } else if id == CLAP_EXT_STATE {
        &HOST_STATE as *const _ as *const c_void
    } else if id == CLAP_EXT_THREAD_CHECK {
        &HOST_THREAD as *const _ as *const c_void
    } else if id == CLAP_EXT_LOG {
        &HOST_LOG as *const _ as *const c_void
    } else if id == CLAP_EXT_PARAMS {
        &HOST_PARAMS as *const _ as *const c_void
    } else if id == CLAP_EXT_LATENCY {
        &HOST_LATENCY as *const _ as *const c_void
    } else {
        std::ptr::null()
    }
}
unsafe extern "C" fn request_restart(_h: *const clap_host) {}
unsafe extern "C" fn request_process(_h: *const clap_host) {}
unsafe extern "C" fn request_callback(h: *const clap_host) {
    data(h).callback.store(true, Ordering::Release);
}

unsafe extern "C" fn gui_hints_changed(_h: *const clap_host) {}
unsafe extern "C" fn gui_request_resize(h: *const clap_host, w: u32, hh: u32) -> bool {
    *data(h).resize.lock() = Some((w, hh));
    true
}
unsafe extern "C" fn gui_request_show(_h: *const clap_host) -> bool {
    true
}
unsafe extern "C" fn gui_request_hide(_h: *const clap_host) -> bool {
    false
}
unsafe extern "C" fn gui_closed(h: *const clap_host, _destroyed: bool) {
    data(h).closed.store(true, Ordering::Release);
}
static HOST_GUI: clap_host_gui = clap_host_gui {
    resize_hints_changed: Some(gui_hints_changed),
    request_resize: Some(gui_request_resize),
    request_show: Some(gui_request_show),
    request_hide: Some(gui_request_hide),
    closed: Some(gui_closed),
};

unsafe extern "C" fn state_dirty(h: *const clap_host) {
    data(h).dirty.store(true, Ordering::Release);
}
static HOST_STATE: clap_host_state = clap_host_state { mark_dirty: Some(state_dirty) };

unsafe extern "C" fn is_main(h: *const clap_host) -> bool {
    data(h).main_thread.load(Ordering::Relaxed) == thread_key()
}
unsafe extern "C" fn is_audio(h: *const clap_host) -> bool {
    data(h).audio_thread.load(Ordering::Relaxed) == thread_key()
}
static HOST_THREAD: clap_host_thread_check = clap_host_thread_check { is_main_thread: Some(is_main), is_audio_thread: Some(is_audio) };

unsafe extern "C" fn log(_h: *const clap_host, severity: clap_log_severity, msg: *const c_char) {
    if severity >= 3 {
        eprintln!("[clap] {}", cstr(msg));
    }
}
static HOST_LOG: clap_host_log = clap_host_log { log: Some(log) };

unsafe extern "C" fn params_rescan(_h: *const clap_host, _f: clap_param_rescan_flags) {}
unsafe extern "C" fn params_clear(_h: *const clap_host, _id: clap_id, _f: clap_param_clear_flags) {}
unsafe extern "C" fn params_flush(_h: *const clap_host) {}
static HOST_PARAMS: clap_host_params = clap_host_params { rescan: Some(params_rescan), clear: Some(params_clear), request_flush: Some(params_flush) };

unsafe extern "C" fn latency_changed(_h: *const clap_host) {}
static HOST_LATENCY: clap_host_latency = clap_host_latency { changed: Some(latency_changed) };

unsafe extern "C" fn in_size(_l: *const clap_input_events) -> u32 {
    0
}
unsafe extern "C" fn in_get(_l: *const clap_input_events, _i: u32) -> *const clap_event_header {
    std::ptr::null()
}
unsafe extern "C" fn out_push(_l: *const clap_output_events, _e: *const clap_event_header) -> bool {
    true
}
static IN_EVENTS: clap_input_events = clap_input_events { ctx: std::ptr::null_mut(), size: Some(in_size), get: Some(in_get) };
static OUT_EVENTS: clap_output_events = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(out_push) };

// streams over memory
unsafe extern "C" fn ostream_write(s: *const clap_ostream, buf: *const c_void, size: u64) -> i64 {
    let v = &mut *((*s).ctx as *mut Vec<u8>);
    v.extend_from_slice(std::slice::from_raw_parts(buf as *const u8, size as usize));
    size as i64
}
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
unsafe extern "C" fn istream_read(s: *const clap_istream, buf: *mut c_void, size: u64) -> i64 {
    let r = &mut *((*s).ctx as *mut Reader);
    let n = (size as usize).min(r.data.len() - r.pos);
    std::ptr::copy_nonoverlapping(r.data.as_ptr().add(r.pos), buf as *mut u8, n);
    r.pos += n;
    n as i64
}

// ───────────────────────────── plug-in ─────────────────────────────

struct Audio {
    started: bool,
    steady: i64,
    tmp_in: Vec<Vec<f32>>,
    tmp_out: Vec<Vec<f32>>,
}

pub struct ClapPlugin {
    plugin: *const clap_plugin,
    pub host_data: Box<HostData>,
    _host: Box<clap_host>,
    gui: *const clap_plugin_gui,
    state: *const clap_plugin_state,
    latency: *const clap_plugin_latency,
    in_ch: usize,
    out_ch: usize,
    audio: Mutex<Audio>,
    gui_open: AtomicBool,
    pub name: String,
    _lib: Arc<ClapLib>,
    _names: [CString; 4],
}

unsafe impl Send for ClapPlugin {}
unsafe impl Sync for ClapPlugin {}

impl ClapPlugin {
    /// Creates, initialises and activates plug-in `id` (main thread).
    pub fn new(lib: Arc<ClapLib>, id: &str, rate: f64, block: u32) -> Result<Self, String> {
        let factory = lib.factory().ok_or("no plug-in factory")?;
        let names = [CString::new("OpenAudioPower").unwrap(), CString::new("redzikowski.com").unwrap(), CString::new("https://redzikowski.com").unwrap(), CString::new(env!("CARGO_PKG_VERSION")).unwrap()];
        let host_data = Box::new(HostData::default());
        host_data.main_thread.store(thread_key(), Ordering::Relaxed);
        let host = Box::new(clap_host {
            clap_version: CLAP_VERSION,
            host_data: &*host_data as *const HostData as *mut c_void,
            name: names[0].as_ptr(),
            vendor: names[1].as_ptr(),
            url: names[2].as_ptr(),
            version: names[3].as_ptr(),
            get_extension: Some(get_extension),
            request_restart: Some(request_restart),
            request_process: Some(request_process),
            request_callback: Some(request_callback),
        });
        let cid = CString::new(id).map_err(|e| e.to_string())?;
        unsafe {
            let plugin = factory.create_plugin.ok_or("create_plugin missing")?(factory, &*host, cid.as_ptr());
            if plugin.is_null() {
                return Err(format!("could not create {id}"));
            }
            let p = &*plugin;
            if !p.init.map(|f| f(plugin)).unwrap_or(false) {
                p.destroy.map(|f| f(plugin));
                return Err("plug-in init failed".into());
            }
            let ext = |id: &CStr| p.get_extension.map(|f| f(plugin, id.as_ptr())).unwrap_or(std::ptr::null());
            let gui = ext(CLAP_EXT_GUI) as *const clap_plugin_gui;
            let state = ext(CLAP_EXT_STATE) as *const clap_plugin_state;
            let latency = ext(CLAP_EXT_LATENCY) as *const clap_plugin_latency;
            let ports = ext(CLAP_EXT_AUDIO_PORTS) as *const clap_plugin_audio_ports;
            let channels = |input: bool| -> usize {
                let Some(ap) = ports.as_ref() else { return 2 };
                if ap.count.map(|f| f(plugin, input)).unwrap_or(0) == 0 {
                    return 0;
                }
                let mut info: clap_audio_port_info = std::mem::zeroed();
                if ap.get.map(|f| f(plugin, 0, input, &mut info)).unwrap_or(false) {
                    info.channel_count as usize
                } else {
                    2
                }
            };
            let (in_ch, out_ch) = (channels(true), channels(false));
            if out_ch == 0 {
                p.destroy.map(|f| f(plugin));
                return Err("plug-in has no audio output".into());
            }
            if !p.activate.map(|f| f(plugin, rate, 1, block)).unwrap_or(false) {
                p.destroy.map(|f| f(plugin));
                return Err("activate failed".into());
            }
            let name = cstr((*p.desc).name);
            Ok(Self {
                plugin,
                host_data,
                _host: host,
                gui,
                state,
                latency,
                in_ch,
                out_ch,
                audio: Mutex::new(Audio { started: false, steady: 0, tmp_in: vec![vec![0.0; block as usize]; in_ch.max(1)], tmp_out: vec![vec![0.0; block as usize]; out_ch] }),
                gui_open: AtomicBool::new(false),
                name,
                _lib: lib,
                _names: names,
            })
        }
    }

    pub fn latency(&self) -> u32 {
        unsafe { self.latency.as_ref().and_then(|l| l.get).map(|f| f(self.plugin)).unwrap_or(0) }
    }

    /// Audio thread: stereo in, stereo out, `n` ≤ the activated block size.
    pub fn process(&self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32], n: usize) {
        let mut a = self.audio.lock();
        let p = unsafe { &*self.plugin };
        if !a.started {
            self.host_data.audio_thread.store(thread_key(), Ordering::Relaxed);
            a.started = unsafe { p.start_processing.map(|f| f(self.plugin)).unwrap_or(true) };
            if !a.started {
                ol[..n].copy_from_slice(&l[..n]);
                or[..n].copy_from_slice(&r[..n]);
                return;
            }
        }
        let Audio { steady, tmp_in, tmp_out, .. } = &mut *a;
        // fit the plug-in's channel layout (mono plug-ins get the sum)
        match self.in_ch {
            0 => {}
            1 => {
                for i in 0..n {
                    tmp_in[0][i] = 0.5 * (l[i] + r[i]);
                }
            }
            _ => {
                tmp_in[0][..n].copy_from_slice(&l[..n]);
                tmp_in[1][..n].copy_from_slice(&r[..n]);
                for c in tmp_in.iter_mut().skip(2) {
                    c[..n].fill(0.0);
                }
            }
        }
        let mut in_ptrs: Vec<*mut f32> = tmp_in.iter_mut().take(self.in_ch.max(1)).map(|c| c.as_mut_ptr()).collect();
        let mut out_ptrs: Vec<*mut f32> = tmp_out.iter_mut().map(|c| c.as_mut_ptr()).collect();
        let inputs = clap_audio_buffer { data32: in_ptrs.as_mut_ptr(), data64: std::ptr::null_mut(), channel_count: self.in_ch as u32, latency: 0, constant_mask: 0 };
        let mut outputs = clap_audio_buffer { data32: out_ptrs.as_mut_ptr(), data64: std::ptr::null_mut(), channel_count: self.out_ch as u32, latency: 0, constant_mask: 0 };
        let proc_ = clap_process {
            steady_time: *steady,
            frames_count: n as u32,
            transport: std::ptr::null(),
            audio_inputs: if self.in_ch > 0 { &inputs } else { std::ptr::null() },
            audio_outputs: &mut outputs,
            audio_inputs_count: (self.in_ch > 0) as u32,
            audio_outputs_count: 1,
            in_events: &IN_EVENTS,
            out_events: &OUT_EVENTS,
        };
        let status = unsafe { p.process.map(|f| f(self.plugin, &proc_)).unwrap_or(0) };
        *steady += n as i64;
        if status == clap_sys::process::CLAP_PROCESS_ERROR {
            ol[..n].copy_from_slice(&l[..n]);
            or[..n].copy_from_slice(&r[..n]);
            return;
        }
        ol[..n].copy_from_slice(&tmp_out[0][..n]);
        or[..n].copy_from_slice(&tmp_out[self.out_ch.min(2) - 1][..n]);
    }

    /// Audio thread: forget tails (after a seek).
    pub fn reset(&self) {
        unsafe {
            if let Some(f) = (*self.plugin).reset {
                f(self.plugin)
            }
        }
    }

    /// Main thread: run callbacks the plug-in asked for.
    pub fn idle(&self) {
        if self.host_data.callback.swap(false, Ordering::AcqRel) {
            unsafe {
                if let Some(f) = (*self.plugin).on_main_thread {
                    f(self.plugin)
                }
            }
        }
    }

    pub fn has_editor(&self) -> bool {
        unsafe { self.gui.as_ref().and_then(|g| g.is_api_supported).map(|f| f(self.plugin, CLAP_WINDOW_API_WIN32.as_ptr(), false)).unwrap_or(false) }
    }

    /// Embeds the editor into `hwnd`; returns its size in pixels.
    pub fn open_editor(&self, hwnd: *mut c_void, scale: f64) -> Result<(u32, u32), String> {
        let g = unsafe { self.gui.as_ref() }.ok_or("no editor")?;
        unsafe {
            if !g.create.map(|f| f(self.plugin, CLAP_WINDOW_API_WIN32.as_ptr(), false)).unwrap_or(false) {
                return Err("gui.create failed".into());
            }
            if let Some(f) = g.set_scale {
                f(self.plugin, scale);
            }
            let (mut w, mut h) = (0u32, 0u32);
            if !g.get_size.map(|f| f(self.plugin, &mut w, &mut h)).unwrap_or(false) {
                w = 640;
                h = 400;
            }
            let win = clap_window { api: CLAP_WINDOW_API_WIN32.as_ptr(), specific: clap_window_handle { win32: hwnd } };
            if !g.set_parent.map(|f| f(self.plugin, &win)).unwrap_or(false) {
                g.destroy.map(|f| f(self.plugin));
                return Err("gui.set_parent failed".into());
            }
            if let Some(f) = g.show {
                f(self.plugin);
            }
            self.gui_open.store(true, Ordering::Release);
            self.host_data.closed.store(false, Ordering::Release);
            Ok((w.max(50), h.max(50)))
        }
    }

    /// The window was resized by the user: let the plug-in adapt (it may adjust the size).
    pub fn set_editor_size(&self, w: u32, h: u32) -> Option<(u32, u32)> {
        let g = unsafe { self.gui.as_ref() }?;
        unsafe {
            if !g.can_resize.map(|f| f(self.plugin)).unwrap_or(false) {
                return None;
            }
            let (mut ww, mut hh) = (w, h);
            if let Some(f) = g.adjust_size {
                f(self.plugin, &mut ww, &mut hh);
            }
            g.set_size.map(|f| f(self.plugin, ww, hh));
            Some((ww, hh))
        }
    }

    pub fn take_resize_request(&self) -> Option<(u32, u32)> {
        let r = self.host_data.resize.lock().take()?;
        unsafe {
            if let Some(g) = self.gui.as_ref() {
                g.set_size.map(|f| f(self.plugin, r.0, r.1));
            }
        }
        Some(r)
    }

    pub fn close_editor(&self) {
        if self.gui_open.swap(false, Ordering::AcqRel) {
            unsafe {
                if let Some(g) = self.gui.as_ref() {
                    g.hide.map(|f| f(self.plugin));
                    g.destroy.map(|f| f(self.plugin));
                }
            }
        }
    }

    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        let s = unsafe { self.state.as_ref() }.ok_or("no state support")?;
        let mut buf: Vec<u8> = Vec::new();
        let stream = clap_ostream { ctx: &mut buf as *mut Vec<u8> as *mut c_void, write: Some(ostream_write) };
        if unsafe { s.save.map(|f| f(self.plugin, &stream)).unwrap_or(false) } {
            Ok(buf)
        } else {
            Err("state.save failed".into())
        }
    }

    pub fn load_state(&self, data: &[u8]) -> Result<(), String> {
        let s = unsafe { self.state.as_ref() }.ok_or("no state support")?;
        let mut r = Reader { data, pos: 0 };
        let stream = clap_istream { ctx: &mut r as *mut Reader as *mut c_void, read: Some(istream_read) };
        if unsafe { s.load.map(|f| f(self.plugin, &stream)).unwrap_or(false) } {
            Ok(())
        } else {
            Err("state.load failed".into())
        }
    }
}

impl Drop for ClapPlugin {
    fn drop(&mut self) {
        self.close_editor();
        unsafe {
            let p = &*self.plugin;
            if self.audio.lock().started {
                p.stop_processing.map(|f| f(self.plugin));
            }
            p.deactivate.map(|f| f(self.plugin));
            p.destroy.map(|f| f(self.plugin));
        }
    }
}
