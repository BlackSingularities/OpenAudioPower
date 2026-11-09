//! `oap-plughost` – runs one VST3 or CLAP plug-in for OpenAudioPower in its own process, so a
//! plug-in that crashes or hangs takes down only this helper, never the app.
//!
//! ```text
//! oap-plughost scan <file>                 → JSON array of plug-ins in the file (stdout)
//! oap-plughost run <format> <path> <id> <rate> <block> <shm> <parent-pid> <owner-hwnd>
//! ```
//! In `run` mode audio goes through shared memory (see `shm.rs`), control through JSON lines:
//! stdin takes `{"cmd": "editor" | "closeEditor" | "getState" | "setState" | "quit", …}`,
//! stdout reports `{"event": "ready" | "state" | "ok" | "error" | "editorClosed", …}`.

#![windows_subsystem = "windows"]

mod clap;
mod shm;
mod vst3;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WAIT_TIMEOUT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, BLACK_BRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentThreadId, OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PluginDesc {
    pub format: String,
    pub path: String,
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub category: String,
    pub instrument: bool,
}

enum Plug {
    Clap(clap::ClapPlugin),
    Vst3(vst3::Vst3Plugin),
}

impl Plug {
    fn name(&self) -> &str {
        match self {
            Plug::Clap(p) => &p.name,
            Plug::Vst3(p) => &p.name,
        }
    }
    fn latency(&self) -> u32 {
        match self {
            Plug::Clap(p) => p.latency(),
            Plug::Vst3(p) => p.latency(),
        }
    }
    fn process(&self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32], n: usize) {
        match self {
            Plug::Clap(p) => p.process(l, r, ol, or, n),
            Plug::Vst3(p) => p.process(l, r, ol, or, n),
        }
    }
    fn reset(&self) {
        match self {
            Plug::Clap(p) => p.reset(),
            Plug::Vst3(p) => p.reset(),
        }
    }
    fn idle(&self) {
        match self {
            Plug::Clap(p) => p.idle(),
            Plug::Vst3(p) => p.idle(),
        }
    }
    fn has_editor(&self) -> bool {
        match self {
            Plug::Clap(p) => p.has_editor(),
            Plug::Vst3(p) => p.has_editor(),
        }
    }
    fn open_editor(&self, hwnd: HWND, scale: f64) -> Result<(u32, u32), String> {
        match self {
            Plug::Clap(p) => p.open_editor(hwnd.0, scale),
            Plug::Vst3(p) => p.open_editor(hwnd.0),
        }
    }
    fn set_editor_size(&self, w: u32, h: u32) -> Option<(u32, u32)> {
        match self {
            Plug::Clap(p) => p.set_editor_size(w, h),
            Plug::Vst3(p) => p.set_editor_size(w, h),
        }
    }
    fn take_resize_request(&self) -> Option<(u32, u32)> {
        match self {
            Plug::Clap(p) => p.take_resize_request(),
            Plug::Vst3(p) => p.take_resize_request(),
        }
    }
    fn editor_closed_by_plugin(&self) -> bool {
        match self {
            Plug::Clap(p) => p.host_data.closed.swap(false, Ordering::AcqRel),
            Plug::Vst3(_) => false,
        }
    }
    fn close_editor(&self) {
        match self {
            Plug::Clap(p) => p.close_editor(),
            Plug::Vst3(p) => p.close_editor(),
        }
    }
    fn save_state(&self) -> Result<Vec<u8>, String> {
        match self {
            Plug::Clap(p) => p.save_state(),
            Plug::Vst3(p) => p.save_state(),
        }
    }
    fn load_state(&self, d: &[u8]) -> Result<(), String> {
        match self {
            Plug::Clap(p) => p.load_state(d),
            Plug::Vst3(p) => p.load_state(d),
        }
    }
}

fn emit(v: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("scan") if args.len() >= 2 => scan(Path::new(&args[1])),
        Some("run") if args.len() >= 9 => run(&args[1..]),
        _ => {
            eprintln!("usage: oap-plughost scan <file> | run <format> <path> <id> <rate> <block> <shm> <parent-pid> <owner-hwnd>");
            2
        }
    };
    std::process::exit(code);
}

fn scan(path: &Path) -> i32 {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let res = match ext.as_str() {
        "clap" => clap::ClapLib::load(path).map(|lib| lib.descriptors(path)),
        "vst3" => vst3::describe(path),
        _ => Err("unknown format".into()),
    };
    match res {
        Ok(list) => {
            println!("{}", serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()));
            0
        }
        Err(e) => {
            println!("{}", serde_json::json!({ "error": e }));
            1
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
enum Cmd {
    Editor,
    CloseEditor,
    GetState { req: u64 },
    SetState { req: u64, data: String },
    Quit,
}

// main-thread globals for the window procedure
thread_local! {
    static PLUG: std::cell::RefCell<Option<Arc<Plug>>> = const { std::cell::RefCell::new(None) };
    static EDITOR: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    static SIZING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

const WM_CMD: u32 = WM_APP + 1;

fn run(a: &[String]) -> i32 {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let (format, path, id) = (a[0].as_str(), Path::new(&a[1]), a[2].as_str());
    let rate: f64 = a[3].parse().unwrap_or(48000.0);
    let block: usize = a[4].parse::<usize>().unwrap_or(1024).clamp(32, shm::MAX_BLOCK);
    let shm_name = &a[5];
    let parent_pid: u32 = a[6].parse().unwrap_or(0);
    let owner = HWND(a[7].parse::<isize>().unwrap_or(0) as *mut _);

    let shared = match shm::Shared::open(shm_name) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            emit(serde_json::json!({ "event": "error", "message": e }));
            return 1;
        }
    };
    let plug = match format {
        "clap" => clap::ClapLib::load(path).and_then(|lib| clap::ClapPlugin::new(lib, id, rate, block as u32)).map(Plug::Clap),
        "vst3" => vst3::Vst3Plugin::new(path, id, rate, block).map(Plug::Vst3),
        _ => Err(format!("unknown format {format}")),
    };
    let plug = match plug {
        Ok(p) => Arc::new(p),
        Err(e) => {
            emit(serde_json::json!({ "event": "error", "message": e }));
            return 1;
        }
    };
    shared.header().latency.store(plug.latency(), Ordering::Release);
    emit(serde_json::json!({ "event": "ready", "name": plug.name(), "latency": plug.latency(), "hasEditor": plug.has_editor() }));

    let quit = Arc::new(AtomicBool::new(false));
    let main_tid = unsafe { GetCurrentThreadId() };

    // ── audio thread ──
    {
        let (plug, shared, quit) = (plug.clone(), shared.clone(), quit.clone());
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let parent = if parent_pid != 0 { unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid).ok() } } else { None };
                let mut ol = vec![0.0f32; shm::MAX_BLOCK];
                let mut or = vec![0.0f32; shm::MAX_BLOCK];
                while !quit.load(Ordering::Acquire) {
                    if !shared.wait(shared.ev_in, 500) {
                        // nobody is asking for audio: is the app still there?
                        if let Some(p) = parent {
                            if unsafe { WaitForSingleObject(p, 0) } != WAIT_TIMEOUT {
                                quit.store(true, Ordering::Release);
                                unsafe {
                                    let _ = PostThreadMessageW(main_tid, WM_QUIT, WPARAM(0), LPARAM(0));
                                }
                            }
                        }
                        continue;
                    }
                    let h = shared.header();
                    let seq = h.seq_in.load(Ordering::Acquire);
                    let n = (h.frames.load(Ordering::Acquire) as usize).min(shm::MAX_BLOCK);
                    let flags = h.flags.swap(0, Ordering::AcqRel);
                    if flags & shm::FLAG_RESET != 0 {
                        plug.reset();
                    }
                    unsafe {
                        let (l, r) = (shared.buf(false, 0), shared.buf(false, 1));
                        // the plug-in may expect at most `block` frames per call
                        let mut i = 0;
                        while i < n {
                            let k = (n - i).min(block);
                            plug.process(&l[i..i + k], &r[i..i + k], &mut ol[i..i + k], &mut or[i..i + k], k);
                            i += k;
                        }
                        shared.buf(true, 0)[..n].copy_from_slice(&ol[..n]);
                        shared.buf(true, 1)[..n].copy_from_slice(&or[..n]);
                    }
                    h.latency.store(plug.latency(), Ordering::Relaxed);
                    h.seq_out.store(seq, Ordering::Release);
                    shared.signal(shared.ev_out);
                }
                if let Some(p) = parent {
                    unsafe {
                        let _ = CloseHandle(p);
                    }
                }
            })
            .ok();
    }

    // ── control: stdin → main thread ──
    let (tx, rx) = mpsc::channel::<Cmd>();
    {
        let quit = quit.clone();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                if let Ok(c) = serde_json::from_str::<Cmd>(&line) {
                    if tx.send(c).is_err() {
                        break;
                    }
                    unsafe {
                        let _ = PostThreadMessageW(main_tid, WM_CMD, WPARAM(0), LPARAM(0));
                    }
                }
            }
            // the app closed our stdin: shut down
            quit.store(true, Ordering::Release);
            unsafe {
                let _ = PostThreadMessageW(main_tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        });
    }

    // ── main thread: editor window and plug-in callbacks ──
    PLUG.with(|p| *p.borrow_mut() = Some(plug.clone()));
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            lpszClassName: w!("OAPPluginWindow"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        SetTimer(None, 0, 30, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.hwnd.0.is_null() && msg.message == WM_CMD {
                while let Ok(c) = rx.try_recv() {
                    match c {
                        Cmd::Editor => show_editor(&plug, owner),
                        Cmd::CloseEditor => close_editor(&plug, false),
                        Cmd::GetState { req } => match plug.save_state() {
                            Ok(d) => emit(serde_json::json!({ "event": "state", "req": req, "data": base64::engine::general_purpose::STANDARD.encode(d) })),
                            Err(e) => emit(serde_json::json!({ "event": "error", "req": req, "message": e })),
                        },
                        Cmd::SetState { req, data } => {
                            let r = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string()).and_then(|d| plug.load_state(&d));
                            match r {
                                Ok(()) => emit(serde_json::json!({ "event": "ok", "req": req })),
                                Err(e) => emit(serde_json::json!({ "event": "error", "req": req, "message": e })),
                            }
                        }
                        Cmd::Quit => {
                            quit.store(true, Ordering::Release);
                            PostQuitMessage(0);
                        }
                    }
                }
                continue;
            }
            if msg.hwnd.0.is_null() && msg.message == WM_TIMER {
                plug.idle();
                let ed = HWND(EDITOR.with(|e| e.get()) as *mut _);
                if !ed.0.is_null() {
                    if plug.editor_closed_by_plugin() {
                        close_editor(&plug, true);
                    } else if let Some((w, h)) = plug.take_resize_request() {
                        resize_client(ed, w, h);
                    }
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        close_editor(&plug, false);
    }
    quit.store(true, Ordering::Release);
    shared.signal(shared.ev_in);
    0
}

unsafe fn resize_client(hwnd: HWND, w: u32, h: u32) {
    SIZING.with(|s| s.set(true));
    let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
    let mut r = RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 };
    let _ = AdjustWindowRectEx(&mut r, style, false, WINDOW_EX_STYLE(0));
    let _ = SetWindowPos(hwnd, None, 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    SIZING.with(|s| s.set(false));
}

fn show_editor(plug: &Arc<Plug>, owner: HWND) {
    unsafe {
        let existing = HWND(EDITOR.with(|e| e.get()) as *mut _);
        if !existing.0.is_null() {
            let _ = ShowWindow(existing, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(existing);
            return;
        }
        if !plug.has_editor() {
            emit(serde_json::json!({ "event": "error", "message": "no-editor" }));
            return;
        }
        let title: Vec<u16> = format!("{} – OpenAudioPower", plug.name()).encode_utf16().chain(Some(0)).collect();
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("OAPPluginWindow"),
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            640,
            400,
            if owner.0.is_null() { None } else { Some(owner) },
            None,
            GetModuleHandleW(None).ok().map(|h| h.into()),
            None,
        ) else {
            emit(serde_json::json!({ "event": "error", "message": "window" }));
            return;
        };
        let scale = GetDpiForWindow(hwnd) as f64 / 96.0;
        match plug.open_editor(hwnd, scale) {
            Ok((w, h)) => {
                EDITOR.with(|e| e.set(hwnd.0 as isize));
                resize_client(hwnd, w, h);
                let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
                let _ = SetForegroundWindow(hwnd);
                emit(serde_json::json!({ "event": "editorOpened" }));
            }
            Err(e) => {
                let _ = DestroyWindow(hwnd);
                emit(serde_json::json!({ "event": "error", "message": e }));
            }
        }
    }
}

fn close_editor(plug: &Arc<Plug>, notify: bool) {
    let hwnd = HWND(EDITOR.with(|e| e.replace(0)) as *mut _);
    if hwnd.0.is_null() {
        return;
    }
    plug.close_editor();
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    if notify {
        emit(serde_json::json!({ "event": "editorClosed" }));
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            if let Some(p) = PLUG.with(|p| p.borrow().clone()) {
                close_editor(&p, true);
            }
            LRESULT(0)
        }
        WM_SIZE if !SIZING.with(|s| s.get()) => {
            let (w, h) = ((lp.0 & 0xffff) as u32, ((lp.0 >> 16) & 0xffff) as u32);
            if w > 0 && h > 0 {
                if let Some(p) = PLUG.with(|p| p.borrow().clone()) {
                    if let Some((aw, ah)) = p.set_editor_size(w, h) {
                        if (aw, ah) != (w, h) {
                            resize_client(hwnd, aw, ah);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
