//! "OAP Test Gain" – a tiny CLAP effect used to test OpenAudioPower's plug-in host:
//! stereo gain, an editor (drag the bar), state save/load, and a deliberate crash
//! (load the state `CRASH` and the next `process` call dereferences null).

#![allow(clippy::missing_safety_doc)]

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::ext::audio_ports::{clap_audio_port_info, clap_plugin_audio_ports, CLAP_AUDIO_PORT_IS_MAIN, CLAP_EXT_AUDIO_PORTS, CLAP_PORT_STEREO};
use clap_sys::ext::gui::{clap_gui_resize_hints, clap_plugin_gui, clap_window, CLAP_EXT_GUI, CLAP_WINDOW_API_WIN32};
use clap_sys::ext::state::{clap_plugin_state, CLAP_EXT_STATE};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
use clap_sys::process::{clap_process, clap_process_status, CLAP_PROCESS_CONTINUE};
use clap_sys::stream::{clap_istream, clap_ostream};
use clap_sys::version::CLAP_VERSION;
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const ID: &CStr = c"com.redzikowski.oap-test-gain";

struct Features([*const c_char; 3]);
unsafe impl Sync for Features {}
static FEATURES: Features = Features([c"audio-effect".as_ptr(), c"utility".as_ptr(), std::ptr::null()]);

struct Desc(clap_plugin_descriptor);
unsafe impl Sync for Desc {}
static DESC: Desc = Desc(clap_plugin_descriptor {
    clap_version: CLAP_VERSION,
    id: ID.as_ptr(),
    name: c"OAP Test Gain".as_ptr(),
    vendor: c"redzikowski.com".as_ptr(),
    url: c"https://redzikowski.com".as_ptr(),
    manual_url: c"".as_ptr(),
    support_url: c"".as_ptr(),
    version: c"1.0.0".as_ptr(),
    description: c"Gain for testing OpenAudioPower".as_ptr(),
    features: &FEATURES.0 as *const _ as *const *const c_char,
});

struct Gain {
    plugin: clap_plugin,
    gain: AtomicU32, // f32 bits, linear 0…2
    crash: AtomicBool,
    child: std::sync::atomic::AtomicIsize,
}

unsafe fn me<'a>(p: *const clap_plugin) -> &'a Gain {
    &*((*p).plugin_data as *const Gain)
}

impl Gain {
    fn gain(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }
    fn set_gain(&self, g: f32) {
        self.gain.store(g.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }
}

unsafe extern "C" fn init(_p: *const clap_plugin) -> bool {
    true
}
unsafe extern "C" fn destroy(p: *const clap_plugin) {
    drop(Box::from_raw((*p).plugin_data as *mut Gain));
}
unsafe extern "C" fn activate(_p: *const clap_plugin, _sr: f64, _min: u32, _max: u32) -> bool {
    true
}
unsafe extern "C" fn deactivate(_p: *const clap_plugin) {}
unsafe extern "C" fn start(_p: *const clap_plugin) -> bool {
    true
}
unsafe extern "C" fn stop(_p: *const clap_plugin) {}
unsafe extern "C" fn reset(_p: *const clap_plugin) {}
unsafe extern "C" fn process(p: *const clap_plugin, pr: *const clap_process) -> clap_process_status {
    let g = me(p);
    if g.crash.load(Ordering::Relaxed) {
        // deliberate access violation for the host's crash test
        std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1);
    }
    let pr = &*pr;
    let n = pr.frames_count as usize;
    let gain = g.gain();
    let inp: &clap_audio_buffer = &*pr.audio_inputs;
    let out: &clap_audio_buffer = &*pr.audio_outputs;
    for c in 0..2 {
        let i = *inp.data32.add(c);
        let o = *out.data32.add(c);
        for k in 0..n {
            *o.add(k) = *i.add(k) * gain;
        }
    }
    CLAP_PROCESS_CONTINUE
}
unsafe extern "C" fn on_main(_p: *const clap_plugin) {}

// audio ports: one stereo in, one stereo out
unsafe extern "C" fn ports_count(_p: *const clap_plugin, _input: bool) -> u32 {
    1
}
unsafe extern "C" fn ports_get(_p: *const clap_plugin, index: u32, _input: bool, info: *mut clap_audio_port_info) -> bool {
    if index != 0 {
        return false;
    }
    let i = &mut *info;
    i.id = 0;
    i.name = [0; 256];
    for (k, b) in b"Main".iter().enumerate() {
        i.name[k] = *b as c_char;
    }
    i.flags = CLAP_AUDIO_PORT_IS_MAIN;
    i.channel_count = 2;
    i.port_type = CLAP_PORT_STEREO.as_ptr();
    i.in_place_pair = 0;
    true
}
static PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports { count: Some(ports_count), get: Some(ports_get) };

// state: "GAIN" + f32, or "CRASH"
unsafe extern "C" fn state_save(p: *const clap_plugin, s: *const clap_ostream) -> bool {
    let mut b = b"GAIN".to_vec();
    b.extend_from_slice(&me(p).gain().to_le_bytes());
    (*s).write.map(|w| w(s, b.as_ptr() as *const c_void, b.len() as u64)).unwrap_or(-1) == b.len() as i64
}
unsafe extern "C" fn state_load(p: *const clap_plugin, s: *const clap_istream) -> bool {
    let mut b = [0u8; 16];
    let n = (*s).read.map(|r| r(s, b.as_mut_ptr() as *mut c_void, 16)).unwrap_or(-1);
    if n >= 5 && &b[..5] == b"CRASH" {
        me(p).crash.store(true, Ordering::Relaxed);
        return true;
    }
    if n >= 8 && &b[..4] == b"GAIN" {
        me(p).set_gain(f32::from_le_bytes([b[4], b[5], b[6], b[7]]));
        redraw(me(p));
        return true;
    }
    false
}
static STATE: clap_plugin_state = clap_plugin_state { save: Some(state_save), load: Some(state_load) };

// editor: a child window with a horizontal bar
const W: u32 = 360;
const H: u32 = 120;

fn redraw(g: &Gain) {
    let h = HWND(g.child.load(Ordering::Relaxed) as *mut c_void);
    if !h.0.is_null() {
        unsafe {
            let _ = InvalidateRect(Some(h), None, true);
        }
    }
}

unsafe extern "system" fn child_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let g = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Gain;
    match msg {
        WM_LBUTTONDOWN | WM_MOUSEMOVE if !g.is_null() && (msg == WM_LBUTTONDOWN || wp.0 & 1 != 0) => {
            if msg == WM_LBUTTONDOWN {
                windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            }
            let x = (lp.0 & 0xffff) as i16 as f32;
            (*g).set_gain(((x - 20.0) / (W as f32 - 40.0)) * 2.0);
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            LRESULT(0)
        }
        WM_PAINT if !g.is_null() => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let bg = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0024_1A1E));
            FillRect(dc, &RECT { left: 0, top: 0, right: W as i32, bottom: H as i32 }, bg);
            let _ = DeleteObject(bg.into());
            let gain = (*g).gain();
            let fill = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00F6_5C8B));
            let w = ((W as f32 - 40.0) * gain / 2.0) as i32;
            FillRect(dc, &RECT { left: 20, top: 60, right: 20 + w, bottom: 84 }, fill);
            let _ = DeleteObject(fill.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, windows::Win32::Foundation::COLORREF(0x00FF_FFFF));
            let text: Vec<u16> = format!("OAP Test Gain   {:.0} %", gain * 100.0).encode_utf16().collect();
            let _ = TextOutW(dc, 20, 24, &text);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "C" fn gui_supported(_p: *const clap_plugin, api: *const c_char, floating: bool) -> bool {
    !floating && CStr::from_ptr(api) == CLAP_WINDOW_API_WIN32
}
unsafe extern "C" fn gui_preferred(_p: *const clap_plugin, api: *mut *const c_char, floating: *mut bool) -> bool {
    *api = CLAP_WINDOW_API_WIN32.as_ptr();
    *floating = false;
    true
}
unsafe extern "C" fn gui_create(_p: *const clap_plugin, api: *const c_char, floating: bool) -> bool {
    gui_supported(_p, api, floating)
}
unsafe extern "C" fn gui_destroy(p: *const clap_plugin) {
    let h = HWND(me(p).child.swap(0, Ordering::Relaxed) as *mut c_void);
    if !h.0.is_null() {
        let _ = DestroyWindow(h);
    }
}
unsafe extern "C" fn gui_scale(_p: *const clap_plugin, _s: f64) -> bool {
    false
}
unsafe extern "C" fn gui_size(_p: *const clap_plugin, w: *mut u32, h: *mut u32) -> bool {
    *w = W;
    *h = H;
    true
}
unsafe extern "C" fn gui_can_resize(_p: *const clap_plugin) -> bool {
    false
}
unsafe extern "C" fn gui_hints(_p: *const clap_plugin, _h: *mut clap_gui_resize_hints) -> bool {
    false
}
unsafe extern "C" fn gui_adjust(_p: *const clap_plugin, _w: *mut u32, _h: *mut u32) -> bool {
    false
}
unsafe extern "C" fn gui_set_size(_p: *const clap_plugin, _w: u32, _h: u32) -> bool {
    false
}
unsafe extern "C" fn gui_set_parent(p: *const clap_plugin, win: *const clap_window) -> bool {
    let parent = HWND((*win).specific.win32);
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let wc = WNDCLASSW { lpfnWndProc: Some(child_proc), hInstance: hinst.into(), lpszClassName: w!("OAPTestGainView"), hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(), ..Default::default() };
    RegisterClassW(&wc);
    let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE(0), w!("OAPTestGainView"), w!(""), WS_CHILD | WS_VISIBLE, 0, 0, W as i32, H as i32, Some(parent), None, Some(hinst.into()), None) else {
        return false;
    };
    SetWindowLongPtrW(h, GWLP_USERDATA, me(p) as *const Gain as isize);
    me(p).child.store(h.0 as isize, Ordering::Relaxed);
    true
}
unsafe extern "C" fn gui_set_transient(_p: *const clap_plugin, _w: *const clap_window) -> bool {
    false
}
unsafe extern "C" fn gui_title(_p: *const clap_plugin, _t: *const c_char) {}
unsafe extern "C" fn gui_show(_p: *const clap_plugin) -> bool {
    true
}
unsafe extern "C" fn gui_hide(_p: *const clap_plugin) -> bool {
    true
}
static GUI: clap_plugin_gui = clap_plugin_gui {
    is_api_supported: Some(gui_supported),
    get_preferred_api: Some(gui_preferred),
    create: Some(gui_create),
    destroy: Some(gui_destroy),
    set_scale: Some(gui_scale),
    get_size: Some(gui_size),
    can_resize: Some(gui_can_resize),
    get_resize_hints: Some(gui_hints),
    adjust_size: Some(gui_adjust),
    set_size: Some(gui_set_size),
    set_parent: Some(gui_set_parent),
    set_transient: Some(gui_set_transient),
    suggest_title: Some(gui_title),
    show: Some(gui_show),
    hide: Some(gui_hide),
};

unsafe extern "C" fn get_ext(_p: *const clap_plugin, id: *const c_char) -> *const c_void {
    let id = CStr::from_ptr(id);
    if id == CLAP_EXT_AUDIO_PORTS {
        &PORTS as *const _ as *const c_void
    } else if id == CLAP_EXT_STATE {
        &STATE as *const _ as *const c_void
    } else if id == CLAP_EXT_GUI {
        &GUI as *const _ as *const c_void
    } else {
        std::ptr::null()
    }
}

// factory and entry
unsafe extern "C" fn count(_f: *const clap_plugin_factory) -> u32 {
    1
}
unsafe extern "C" fn descriptor(_f: *const clap_plugin_factory, i: u32) -> *const clap_plugin_descriptor {
    if i == 0 {
        &DESC.0
    } else {
        std::ptr::null()
    }
}
unsafe extern "C" fn create(_f: *const clap_plugin_factory, _host: *const clap_host, id: *const c_char) -> *const clap_plugin {
    if CStr::from_ptr(id) != ID {
        return std::ptr::null();
    }
    let g = Box::into_raw(Box::new(Gain {
        plugin: clap_plugin {
            desc: &DESC.0,
            plugin_data: std::ptr::null_mut(),
            init: Some(init),
            destroy: Some(destroy),
            activate: Some(activate),
            deactivate: Some(deactivate),
            start_processing: Some(start),
            stop_processing: Some(stop),
            reset: Some(reset),
            process: Some(process),
            get_extension: Some(get_ext),
            on_main_thread: Some(on_main),
        },
        gain: AtomicU32::new(0.5f32.to_bits()),
        crash: AtomicBool::new(false),
        child: std::sync::atomic::AtomicIsize::new(0),
    }));
    (*g).plugin.plugin_data = g as *mut c_void;
    &(*g).plugin
}
static FACTORY: clap_plugin_factory = clap_plugin_factory { get_plugin_count: Some(count), get_plugin_descriptor: Some(descriptor), create_plugin: Some(create) };

unsafe extern "C" fn entry_init(_path: *const c_char) -> bool {
    true
}
unsafe extern "C" fn entry_deinit() {}
unsafe extern "C" fn entry_factory(id: *const c_char) -> *const c_void {
    if CStr::from_ptr(id) == CLAP_PLUGIN_FACTORY_ID {
        &FACTORY as *const _ as *const c_void
    } else {
        std::ptr::null()
    }
}

#[no_mangle]
#[used]
pub static clap_entry: clap_plugin_entry = clap_plugin_entry { clap_version: CLAP_VERSION, init: Some(entry_init), deinit: Some(entry_deinit), get_factory: Some(entry_factory) };
