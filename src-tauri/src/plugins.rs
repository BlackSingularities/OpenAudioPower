//! VST3 / CLAP plug-ins. Each instance runs in its own `oap-plughost.exe`, so a plug-in that
//! crashes or hangs costs that slot (it then passes audio through) and never the app.
//!
//! * audio – one shared-memory block + two events per instance (`plughost/src/shm.rs`),
//! * control – JSON lines over the helper's stdin / stdout,
//! * scanning – every file is opened by a short-lived helper (`scan` mode), results cached.

#[path = "../../plughost/src/shm.rs"]
mod shm;

use openaudiopower_core::dsp::Processor;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::Threading::WaitForSingleObject;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginDesc {
    pub format: String,
    pub path: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub instrument: bool,
}

/// What the UI hears about a running instance.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PluginEvent {
    pub instance: u64,
    pub kind: String,
    pub message: String,
}

pub fn host_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let here = dir.join("oap-plughost.exe");
    if here.exists() {
        return Some(here);
    }
    // development: target/{debug,release}
    for a in dir.ancestors() {
        for p in ["release", "debug"] {
            let c = a.join("target").join(p).join("oap-plughost.exe");
            if c.exists() {
                return Some(c);
            }
        }
    }
    None
}

/// Where a bridge reports crashes and editor changes (the app forwards them to the UI).
pub type EventSink = Arc<dyn Fn(PluginEvent) + Send + Sync>;

pub fn ui_sink(h: &AppHandle) -> EventSink {
    let h = h.clone();
    Arc::new(move |e| {
        let _ = h.emit("plugin", e);
    })
}

// ───────────────────────────── one running instance ─────────────────────────────

pub struct Bridge {
    pub instance: u64,
    pub desc: PluginDesc,
    pub has_editor: bool,
    shared: shm::Shared,
    child: Mutex<Child>,
    process: HANDLE,
    stdin: Mutex<Option<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<String, String>>>>>,
    crashed: Arc<AtomicBool>,
    /// one block at a time (live playback and an export may share the instance)
    audio: Mutex<()>,
    seq: AtomicU32,
    misses: AtomicU32,
    req: AtomicU64,
    /// last state we know of (restored after a crash)
    pub last_state: Mutex<String>,
}

unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}

impl Bridge {
    pub fn spawn(on_event: EventSink, instance: u64, desc: &PluginDesc, rate: u32, owner: isize) -> Result<Arc<Self>, String> {
        let exe = host_exe().ok_or("oap-plughost.exe not found")?;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let name = format!("Local\\oap-plug-{}-{instance}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let shared = shm::Shared::create(&name)?;
        let block = 1024usize;
        let mut child = Command::new(exe)
            .args(["run", &desc.format, &desc.path, &desc.id, &rate.to_string(), &block.to_string(), &name, &std::process::id().to_string(), &owner.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("plughost: {e}"))?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stdin = child.stdin.take();
        let process = HANDLE(child.as_raw_handle());
        let pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<String, String>>>>> = Arc::default();
        let crashed = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<serde_json::Value, String>>();
        {
            let (pending, crashed, app, name) = (pending.clone(), crashed.clone(), on_event.clone(), desc.name.clone());
            std::thread::spawn(move || {
                let mut ready = Some(ready_tx);
                let mut started = false;
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                    let ev = v.get("event").and_then(|x| x.as_str()).unwrap_or("");
                    let msg = v.get("message").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    if let Some(tx) = ready.take() {
                        let _ = tx.send(if ev == "ready" { Ok(v.clone()) } else { Err(msg.clone()) });
                        if ev == "ready" {
                            started = true;
                            continue;
                        }
                        break;
                    }
                    match (ev, v.get("req").and_then(|x| x.as_u64())) {
                        ("state", Some(r)) => {
                            if let Some(tx) = pending.lock().remove(&r) {
                                let _ = tx.send(Ok(v.get("data").and_then(|x| x.as_str()).unwrap_or("").to_string()));
                            }
                        }
                        ("ok", Some(r)) => {
                            if let Some(tx) = pending.lock().remove(&r) {
                                let _ = tx.send(Ok(String::new()));
                            }
                        }
                        ("error", Some(r)) => {
                            if let Some(tx) = pending.lock().remove(&r) {
                                let _ = tx.send(Err(msg));
                            }
                        }
                        ("editorClosed", _) | ("editorOpened", _) | ("error", None) => {
                            app(PluginEvent { instance, kind: ev.to_string(), message: msg });
                        }
                        _ => {}
                    }
                }
                // stdout closed: the helper is gone (a failed start is reported by spawn)
                if !crashed.swap(true, Ordering::AcqRel) && started {
                    app(PluginEvent { instance, kind: "crashed".into(), message: name });
                }
                for (_, tx) in pending.lock().drain() {
                    let _ = tx.send(Err("crashed".into()));
                }
            });
        }
        let info = match ready_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                let _ = child.kill();
                return Err(if e.is_empty() { "plug-in failed to load".into() } else { e });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                return Err("plug-in crashed while loading".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                return Err("plug-in did not start in 30 s".into());
            }
        };
        // a crash during start-up is not an event worth reporting later
        let has_editor = info.get("hasEditor").and_then(|x| x.as_bool()).unwrap_or(false);
        let name = info.get("name").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
        let mut desc = desc.clone();
        if let Some(n) = name {
            desc.name = n;
        }
        Ok(Arc::new(Self {
            instance,
            desc,
            has_editor,
            shared,
            child: Mutex::new(child),
            process,
            stdin: Mutex::new(stdin),
            pending,
            crashed,
            audio: Mutex::new(()),
            seq: AtomicU32::new(0),
            misses: AtomicU32::new(0),
            req: AtomicU64::new(1),
            last_state: Mutex::new(String::new()),
        }))
    }

    pub fn crashed(&self) -> bool {
        self.crashed.load(Ordering::Acquire)
    }

    pub fn latency(&self) -> u32 {
        self.shared.header().latency.load(Ordering::Relaxed)
    }

    fn alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.process, 0) == WAIT_TIMEOUT }
    }

    fn mark_crashed(&self) {
        if !self.crashed.swap(true, Ordering::AcqRel) {
            let _ = self.child.lock().kill();
        }
    }

    /// Audio thread (or export): runs `buf[..2][..n]` through the plug-in in place.
    /// On a crash or a hang the audio is left untouched.
    pub fn process(&self, buf: &mut [Vec<f32>], n: usize, reset: bool) {
        if self.crashed() || buf.len() < 2 {
            return;
        }
        let _g = self.audio.lock();
        let h = self.shared.header();
        if reset {
            h.flags.fetch_or(shm::FLAG_RESET, Ordering::AcqRel);
        }
        let mut i = 0;
        while i < n {
            let k = (n - i).min(shm::MAX_BLOCK);
            let seq = self.seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
            unsafe {
                self.shared.buf(false, 0)[..k].copy_from_slice(&buf[0][i..i + k]);
                self.shared.buf(false, 1)[..k].copy_from_slice(&buf[1][i..i + k]);
            }
            h.frames.store(k as u32, Ordering::Release);
            h.seq_in.store(seq, Ordering::Release);
            self.shared.signal(self.shared.ev_in);
            // generous for slow plug-ins, short enough to notice a hang
            let deadline = Instant::now() + Duration::from_millis(250);
            let mut done = false;
            loop {
                let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
                if left == 0 {
                    break;
                }
                if self.shared.wait(self.shared.ev_out, left) && self.shared.seq_out() == seq {
                    done = true;
                    break;
                }
                if self.shared.seq_out() == seq {
                    done = true;
                    break;
                }
            }
            if !done {
                // dead, or no answer for ~2 s in a row: give up on this instance
                if !self.alive() || self.misses.fetch_add(1, Ordering::AcqRel) >= 8 {
                    self.mark_crashed();
                }
                return;
            }
            self.misses.store(0, Ordering::Relaxed);
            unsafe {
                buf[0][i..i + k].copy_from_slice(&self.shared.buf(true, 0)[..k]);
                buf[1][i..i + k].copy_from_slice(&self.shared.buf(true, 1)[..k]);
            }
            i += k;
        }
    }

    fn send(&self, v: serde_json::Value) -> Result<(), String> {
        let mut s = self.stdin.lock();
        let w = s.as_mut().ok_or("closed")?;
        writeln!(w, "{v}").and_then(|_| w.flush()).map_err(|e| e.to_string())
    }

    fn request(&self, mut v: serde_json::Value, timeout: Duration) -> Result<String, String> {
        if self.crashed() {
            return Err("crashed".into());
        }
        let r = self.req.fetch_add(1, Ordering::Relaxed);
        v["req"] = r.into();
        let (tx, rx) = mpsc::channel();
        self.pending.lock().insert(r, tx);
        self.send(v)?;
        rx.recv_timeout(timeout).unwrap_or_else(|_| {
            self.pending.lock().remove(&r);
            Err("timeout".into())
        })
    }

    pub fn open_editor(&self) -> Result<(), String> {
        self.send(serde_json::json!({ "cmd": "editor" }))
    }

    pub fn close_editor(&self) -> Result<(), String> {
        self.send(serde_json::json!({ "cmd": "closeEditor" }))
    }

    /// The plug-in's state as base64 (also remembered for a restart after a crash).
    pub fn get_state(&self) -> Result<String, String> {
        if self.crashed() {
            return Ok(self.last_state.lock().clone());
        }
        let s = self.request(serde_json::json!({ "cmd": "getState" }), Duration::from_secs(10))?;
        *self.last_state.lock() = s.clone();
        Ok(s)
    }

    pub fn set_state(&self, data: &str) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }
        self.request(serde_json::json!({ "cmd": "setState", "data": data }), Duration::from_secs(10))?;
        *self.last_state.lock() = data.to_string();
        Ok(())
    }

    pub fn shutdown(&self) {
        let _ = self.send(serde_json::json!({ "cmd": "quit" }));
        *self.stdin.lock() = None;
        let mut c = self.child.lock();
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(800) {
            if let Ok(Some(_)) = c.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = c.kill();
    }
}

// ───────────────────────────── registry + processor ─────────────────────────────

pub static PLUGINS: LazyLock<RwLock<HashMap<u64, Arc<Bridge>>>> = LazyLock::new(Default::default);

/// Bumped whenever an instance starts, restarts or ends, so slots look their bridge up again.
static GENERATION: AtomicU64 = AtomicU64::new(1);

/// The insert-chain side of a plug-in slot. It finds its instance by id, so a slot created
/// before the helper is up, or kept across a restart after a crash, picks up the new bridge.
struct PlugProc {
    instance: u64,
    gen: u64,
    bridge: Option<Arc<Bridge>>,
    reset: bool,
}

impl PlugProc {
    fn refresh(&mut self) {
        let g = GENERATION.load(Ordering::Acquire);
        if g != self.gen {
            // never block the audio thread on the registry
            if let Some(m) = PLUGINS.try_read() {
                self.bridge = m.get(&self.instance).cloned();
                self.gen = g;
            }
        }
    }
}

impl Processor for PlugProc {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        self.refresh();
        let r = std::mem::take(&mut self.reset);
        if let Some(b) = &self.bridge {
            b.process(buf, n, r);
        }
    }
    fn reset(&mut self) {
        self.reset = true;
    }
    fn latency(&self) -> usize {
        self.bridge.as_ref().map(|b| b.latency() as usize).unwrap_or(0)
    }
}

fn factory(instance: u64) -> Option<Box<dyn Processor>> {
    let bridge = PLUGINS.read().get(&instance).cloned();
    Some(Box::new(PlugProc { instance, gen: GENERATION.load(Ordering::Acquire), bridge, reset: false }))
}

pub fn register(b: Arc<Bridge>) {
    let old = PLUGINS.write().insert(b.instance, b);
    GENERATION.fetch_add(1, Ordering::AcqRel);
    if let Some(o) = old {
        std::thread::spawn(move || o.shutdown());
    }
}

pub fn install() {
    openaudiopower_core::effects::set_external_factory(factory);
}

pub fn get(instance: u64) -> Result<Arc<Bridge>, String> {
    PLUGINS.read().get(&instance).cloned().ok_or_else(|| "plug-in is not running".into())
}

/// Ends instances the project no longer uses (keeps `keep`).
pub fn retain(keep: &[u64]) {
    let gone: Vec<Arc<Bridge>> = {
        let mut m = PLUGINS.write();
        let ids: Vec<u64> = m.keys().copied().filter(|k| !keep.contains(k)).collect();
        ids.into_iter().filter_map(|k| m.remove(&k)).collect()
    };
    GENERATION.fetch_add(1, Ordering::AcqRel);
    for b in gone {
        std::thread::spawn(move || b.shutdown());
    }
}

// ───────────────────────────── scanning ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScanEntry {
    pub file: String,
    pub stamp: String,
    #[serde(default)]
    pub plugins: Vec<PluginDesc>,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn default_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in ["CommonProgramFiles", "CommonProgramW6432"] {
        if let Ok(p) = std::env::var(var) {
            v.push(PathBuf::from(&p).join("VST3"));
            v.push(PathBuf::from(&p).join("CLAP"));
        }
    }
    if let Ok(p) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(&p).join("Programs").join("Common").join("VST3"));
        v.push(PathBuf::from(&p).join("Programs").join("Common").join("CLAP"));
    }
    v.sort();
    v.dedup();
    v
}

/// `.vst3` bundles (folders or single files) and `.clap` files under `dir`.
fn find(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext == "vst3" || ext == "clap" {
            out.push(p);
        } else if p.is_dir() && depth < 6 {
            find(&p, out, depth + 1);
        }
    }
}

fn stamp(p: &Path) -> String {
    std::fs::metadata(p).map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default()
}

fn scan_file(exe: &Path, file: &Path) -> ScanEntry {
    let mut e = ScanEntry { file: file.to_string_lossy().into_owned(), stamp: stamp(file), plugins: Vec::new(), error: None };
    let child = Command::new(exe).arg("scan").arg(file).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null()).creation_flags(CREATE_NO_WINDOW).spawn();
    let Ok(mut child) = child else {
        e.error = Some("spawn".into());
        return e;
    };
    let t = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if t.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                e.error = Some("timeout".into());
                return e;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break,
        }
    }
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut so, &mut out);
    }
    match serde_json::from_str::<Vec<PluginDesc>>(out.trim()) {
        Ok(list) => e.plugins = list,
        Err(_) => {
            let msg = serde_json::from_str::<serde_json::Value>(out.trim()).ok().and_then(|v| v.get("error").and_then(|x| x.as_str()).map(str::to_string));
            e.error = Some(msg.unwrap_or_else(|| "crashed while loading".into()));
        }
    }
    e
}

/// Scans the folders; unchanged files come from `cache_file` unless `full`.
pub fn scan(dirs: &[PathBuf], cache_file: &Path, full: bool, progress: &(dyn Fn(f32, &str) + Sync)) -> Result<Vec<ScanEntry>, String> {
    use rayon::prelude::*;
    let exe = host_exe().ok_or("oap-plughost.exe not found")?;
    let cache: Vec<ScanEntry> = if full { Vec::new() } else { std::fs::read_to_string(cache_file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() };
    let mut files = Vec::new();
    for d in dirs {
        find(d, &mut files, 0);
    }
    files.sort();
    files.dedup();
    let done = AtomicU32::new(0);
    let total = files.len().max(1) as f32;
    let res: Vec<ScanEntry> = files
        .par_iter()
        .with_max_len(1)
        .map(|f| {
            let key = f.to_string_lossy();
            let r = match cache.iter().find(|c| c.file == key && c.stamp == stamp(f)) {
                Some(c) => c.clone(),
                None => scan_file(&exe, f),
            };
            let k = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(k as f32 / total, f.file_name().and_then(|x| x.to_str()).unwrap_or(""));
            r
        })
        .collect();
    if let Some(dir) = cache_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache_file, serde_json::to_string_pretty(&res).unwrap_or_default());
    Ok(res)
}

// ───────────────────────────── commands ─────────────────────────────

type R<T> = Result<T, String>;

fn app_state(h: &AppHandle) -> Arc<crate::state::App> {
    use tauri::Manager;
    h.state::<Arc<crate::state::App>>().inner().clone()
}

fn owner_hwnd(h: &AppHandle) -> isize {
    use tauri::Manager;
    h.get_webview_window("main").and_then(|w| w.hwnd().ok()).map(|x| x.0 as isize).unwrap_or(0)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    name: String,
    has_editor: bool,
    latency: u32,
}

#[tauri::command]
pub fn plugin_dirs() -> Vec<String> {
    default_dirs().into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

#[tauri::command]
pub async fn plugin_scan(h: AppHandle, dirs: Vec<String>, full: bool) -> R<Vec<ScanEntry>> {
    let app = app_state(&h);
    let cache = app.config_dir.join("plugins.json");
    let hh = h.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let emit = |f: f32, label: &str| {
            let _ = hh.emit("job", serde_json::json!({ "kind": "scan", "fraction": f, "label": label }));
        };
        let dirs: Vec<PathBuf> = dirs.into_iter().map(PathBuf::from).collect();
        scan(&dirs, &cache, full, &emit)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts (or restarts) instance `instance` and restores `state` (base64, may be empty).
#[tauri::command]
pub async fn plugin_create(h: AppHandle, instance: u64, desc: PluginDesc, state: String) -> R<Started> {
    let app = app_state(&h);
    let owner = owner_hwnd(&h);
    tauri::async_runtime::spawn_blocking(move || {
        let b = Bridge::spawn(ui_sink(&h), instance, &desc, app.rate(), owner)?;
        if !state.is_empty() {
            // a state the plug-in rejects should not stop it from running
            let _ = b.set_state(&state);
        }
        let r = Started { name: b.desc.name.clone(), has_editor: b.has_editor, latency: b.latency() };
        register(b);
        Ok(r)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restarts a crashed instance with the last state it reported.
#[tauri::command]
pub async fn plugin_restart(h: AppHandle, instance: u64) -> R<Started> {
    let old = get(instance)?;
    let state = old.last_state.lock().clone();
    plugin_create(h, instance, old.desc.clone(), state).await
}

#[tauri::command]
pub fn plugin_editor(instance: u64, open: bool) -> R<()> {
    let b = get(instance)?;
    if b.crashed() {
        return Err("crashed".into());
    }
    if open { b.open_editor() } else { b.close_editor() }
}

/// Current state of each running instance (base64), for saving the project.
#[tauri::command]
pub async fn plugin_states(instances: Vec<u64>) -> R<HashMap<u64, String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = HashMap::new();
        for i in instances {
            if let Ok(b) = get(i) {
                if let Ok(s) = b.get_state() {
                    out.insert(i, s);
                }
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    instance: u64,
    name: String,
    crashed: bool,
    has_editor: bool,
}

#[tauri::command]
pub fn plugin_status() -> Vec<Status> {
    PLUGINS.read().values().map(|b| Status { instance: b.instance, name: b.desc.name.clone(), crashed: b.crashed(), has_editor: b.has_editor }).collect()
}

/// Ends every instance not in `keep` (after loading another project, on a new project).
#[tauri::command]
pub fn plugin_retain(keep: Vec<u64>) {
    retain(&keep);
}

#[cfg(test)]
mod tests {
    use super::*;
    use openaudiopower_core::effects::{Chain, EffectSlot, EffectSpec};

    /// The real path: an insert chain with a plug-in slot, the helper process and the test
    /// plug-in (built by `cargo build --release -p oap-testplug`). Skipped when it is missing.
    #[test]
    fn plugin_slot_processes_and_survives_a_crash() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let dll = root.join("target/release/oap_testplug.dll");
        if host_exe().is_none() || !dll.exists() {
            eprintln!("skipped: build oap-plughost and oap-testplug first");
            return;
        }
        let clap = std::env::temp_dir().join("oap-test-gain.clap");
        std::fs::copy(&dll, &clap).unwrap();
        install();
        let events: Arc<Mutex<Vec<PluginEvent>>> = Arc::default();
        let ev = events.clone();
        let sink: EventSink = Arc::new(move |e| ev.lock().push(e));
        let desc = PluginDesc { format: "clap".into(), path: clap.to_string_lossy().into_owned(), id: "com.redzikowski.oap-test-gain".into(), name: "OAP Test Gain".into(), vendor: String::new(), version: String::new(), category: String::new(), instrument: false };
        let b = Bridge::spawn(sink, 4242, &desc, 48000, 0).expect("start");
        register(b.clone());

        let mut chain = Chain::new(48000.0);
        let spec = EffectSpec::Plugin { instance: 4242, name: desc.name.clone(), format: "clap".into(), path: desc.path.clone(), id: desc.id.clone(), state: String::new() };
        chain.sync(&[EffectSlot { id: 1, on: true, spec }]);
        let n = 512;
        let mut buf = vec![vec![0.6f32; n], vec![-0.6f32; n]];
        chain.process(&mut buf, n);
        assert!((buf[0][10] - 0.3).abs() < 1e-5 && (buf[1][10] + 0.3).abs() < 1e-5, "gain 50 %: {} {}", buf[0][10], buf[1][10]);

        // state round trip
        let st = b.get_state().unwrap();
        assert!(!st.is_empty());
        b.set_state(&st).unwrap();

        // make it crash: the slot must keep passing audio through and report the crash
        b.set_state("Q1JBU0g=").unwrap();
        let mut buf = vec![vec![0.5f32; n], vec![0.5f32; n]];
        chain.process(&mut buf, n);
        assert_eq!(buf[0][10], 0.5, "audio must pass through after a crash");
        let t = Instant::now();
        while !b.crashed() && t.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(b.crashed());
        std::thread::sleep(Duration::from_millis(200));
        assert!(events.lock().iter().any(|e| e.kind == "crashed" && e.instance == 4242), "no crash event");
        let mut buf = vec![vec![0.25f32; n], vec![0.25f32; n]];
        chain.process(&mut buf, n);
        assert_eq!(buf[0][10], 0.25);

        // restart under the same instance id: the existing slot picks the new helper up
        let b2 = Bridge::spawn(Arc::new(|_| {}), 4242, &desc, 48000, 0).expect("restart");
        register(b2);
        let mut buf = vec![vec![0.8f32; n], vec![0.8f32; n]];
        chain.process(&mut buf, n);
        assert!((buf[0][10] - 0.4).abs() < 1e-5, "after restart: {}", buf[0][10]);
        retain(&[]);
    }
}
