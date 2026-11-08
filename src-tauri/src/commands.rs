//! Tauri commands – the whole UI ↔ engine surface.

use crate::engine::{self, DeviceList, Metronome};
use crate::state::{App, NewSource, SourceInfo};
use openaudiopower_core::analysis::{self, Analysis};
use openaudiopower_core::buffer::{AudioBuffer, Meta};
use openaudiopower_core::dsp::biquad::EqBand;
use openaudiopower_core::encode::{self, EncodeOptions};
use openaudiopower_core::mix::{self, Project, TrackMeter};
use openaudiopower_core::ops::{self, Op};
use openaudiopower_core::separate::{self, models, SepOptions};
use openaudiopower_core::{decode, peaks, resample, spectral, Cancel, Progress};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, State};

type R<T> = Result<T, String>;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobEvent<'a> {
    kind: &'a str,
    fraction: f32,
    label: String,
}

fn emitter(app: &AppHandle, kind: &'static str) -> impl Fn(f32, &str) + Send + Sync + Clone {
    let app = app.clone();
    let last = Arc::new(parking_lot::Mutex::new((std::time::Instant::now(), -1.0f32)));
    move |f: f32, label: &str| {
        let mut l = last.lock();
        if f >= 1.0 || f - l.1 >= 0.01 || l.0.elapsed().as_millis() > 250 {
            *l = (std::time::Instant::now(), f);
            let _ = app.emit("job", JobEvent { kind, fraction: f, label: label.to_string() });
        }
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn app_state(h: &AppHandle) -> Arc<App> {
    use tauri::Manager;
    h.state::<Arc<App>>().inner().clone()
}

// ───────────────────────────── app ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    rate: u32,
    models_dir: String,
    threads: usize,
    extensions: Vec<&'static str>,
    neural: bool,
}

#[tauri::command]
pub fn app_info(h: AppHandle, app: State<'_, Arc<App>>) -> AppInfo {
    AppInfo {
        version: h.package_info().version.to_string(),
        rate: app.rate(),
        models_dir: app.models_dir.to_string_lossy().into_owned(),
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        extensions: decode::EXTENSIONS.to_vec(),
        neural: true,
    }
}

#[tauri::command]
pub fn system_language() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en".into())
}

#[tauri::command]
pub fn startup_files(app: State<'_, Arc<App>>) -> Vec<String> {
    app.startup.clone()
}

#[tauri::command]
pub fn load_settings(app: State<'_, Arc<App>>) -> Option<String> {
    std::fs::read_to_string(app.config_dir.join("settings.json")).ok()
}

#[tauri::command]
pub fn save_settings(app: State<'_, Arc<App>>, json: String) -> R<()> {
    std::fs::create_dir_all(&app.config_dir).map_err(|e| e.to_string())?;
    std::fs::write(app.config_dir.join("settings.json"), json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_job(app: State<'_, Arc<App>>) {
    app.cancel.lock().set();
}

// ───────────────────────────── sources ─────────────────────────────

fn import_one(app: &App, path: &Path, cancel: &Cancel, progress: &(dyn Fn(f32) + Sync)) -> R<SourceInfo> {
    let (buf, meta) = decode::decode_file(path, cancel, &|f| progress(f * 0.8))?;
    let buf = if buf.rate != app.rate() { resample::resample(&buf, app.rate()) } else { buf };
    let buf = if buf.channels() > 2 { buf.to_stereo() } else { buf };
    let name = if !meta.title.is_empty() && !meta.artist.is_empty() {
        format!("{} – {}", meta.artist, meta.title)
    } else {
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio").to_string()
    };
    Ok(app.add(NewSource { name, path: Some(path.to_string_lossy().into_owned()), kind: "file", parent: None, stem: None, meta, buf }))
}

#[derive(Serialize)]
pub struct ImportResult {
    sources: Vec<SourceInfo>,
    errors: Vec<String>,
}

#[tauri::command]
pub async fn import_files(h: AppHandle, paths: Vec<String>) -> R<ImportResult> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "import");
    blocking(move || {
        // expand folders
        let mut files: Vec<PathBuf> = Vec::new();
        for p in paths {
            let pb = PathBuf::from(&p);
            if pb.is_dir() {
                if let Ok(rd) = std::fs::read_dir(&pb) {
                    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| decode::is_supported(p)).collect();
                    v.sort();
                    files.extend(v);
                }
            } else {
                files.push(pb);
            }
        }
        let total = files.len().max(1) as f32;
        let done = std::sync::atomic::AtomicUsize::new(0);
        let results: Vec<(PathBuf, R<SourceInfo>)> = files
            .par_iter()
            .map(|p| {
                let r = import_one(&app, p, &cancel, &|_f| {});
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                emit(d as f32 / total, &p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                (p.clone(), r)
            })
            .collect();
        emit(1.0, "");
        let mut sources = Vec::new();
        let mut errors = Vec::new();
        for (p, r) in results {
            match r {
                Ok(s) => sources.push(s),
                Err(e) if e == "cancelled" => {}
                Err(e) => errors.push(format!("{}: {e}", p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())),
            }
        }
        Ok(ImportResult { sources, errors })
    })
    .await
}

#[tauri::command]
pub fn list_sources(app: State<'_, Arc<App>>) -> Vec<SourceInfo> {
    let mut v: Vec<SourceInfo> = app.sources.read().values().map(|s| s.info()).collect();
    v.sort_by_key(|s| s.id);
    v
}

#[tauri::command]
pub fn source_peaks(app: State<'_, Arc<App>>, id: u64) -> R<Response> {
    let s = app.get(id)?;
    Ok(Response::new(peaks::encode(&s.peaks, peaks::PEAK_SPP)))
}

/// Frequency-band overview for coloured / RMS waveforms.
#[tauri::command]
pub async fn source_bands(h: AppHandle, id: u64) -> R<Response> {
    let app = app_state(&h);
    blocking(move || {
        let s = app.get(id)?;
        Ok(Response::new(peaks::bands(&s.buf)))
    })
    .await
}

/// Raw samples (for deep zoom): u32 channels, u32 frames, then f32 planar.
#[tauri::command]
pub fn source_samples(app: State<'_, Arc<App>>, id: u64, start: usize, len: usize) -> R<Response> {
    let s = app.get(id)?;
    let n = s.buf.frames();
    let a = start.min(n);
    let b = (start + len.min(1 << 20)).min(n);
    let mut out = Vec::with_capacity(8 + (b - a) * 4 * s.buf.channels());
    out.extend_from_slice(&(s.buf.channels() as u32).to_le_bytes());
    out.extend_from_slice(&((b - a) as u32).to_le_bytes());
    for c in &s.buf.ch {
        for v in &c[a..b] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(Response::new(out))
}

#[tauri::command]
pub fn remove_sources(app: State<'_, Arc<App>>, ids: Vec<u64>) {
    app.remove(&ids);
}

#[tauri::command]
pub fn rename_source(app: State<'_, Arc<App>>, id: u64, name: String) -> R<SourceInfo> {
    let s = app.get(id)?;
    let e = crate::state::SourceEntry {
        id: s.id,
        name,
        path: s.path.clone(),
        kind: s.kind.clone(),
        parent: s.parent,
        stem: s.stem.clone(),
        meta: s.meta.clone(),
        buf: s.buf.clone(),
        peaks: s.peaks.clone(),
        saved_as: parking_lot::Mutex::new(s.saved_as.lock().clone()),
    };
    let info = e.info();
    app.sources.write().insert(id, Arc::new(e));
    Ok(info)
}

// ───────────────────────────── transport ─────────────────────────────

#[tauri::command]
pub fn set_project(app: State<'_, Arc<App>>, project: Project) {
    app.engine.set_project(project);
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Transport {
    Play,
    Pause,
    Seek { pos: i64 },
    Loop { start: i64, end: i64, on: bool },
    Metronome(Metronome),
}

#[tauri::command]
pub fn transport(app: State<'_, Arc<App>>, t: Transport) {
    let e = &app.engine;
    match t {
        Transport::Play => e.play(),
        Transport::Pause => e.pause(),
        Transport::Seek { pos } => e.seek(pos),
        Transport::Loop { start, end, on } => e.set_loop(if on { Some((start, end)) } else { None }),
        Transport::Metronome(m) => e.set_metronome(m),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    pos: i64,
    playing: bool,
    recording: bool,
    rec_frames: u64,
    rec_start: i64,
    rec_rate: u32,
    meters: Vec<TrackMeter>,
    master: [f32; 4],
    master_gr: f32,
    clip: bool,
    input: [f32; 2],
    input_open: bool,
    error: String,
    device_rate: u32,
}

#[tauri::command]
pub fn poll(app: State<'_, Arc<App>>) -> Poll {
    let e = &app.engine;
    let (master, gr, clip) = e.master_meters();
    let mut err = e.last_error.lock().clone();
    if err.is_empty() {
        err = e.input.error.lock().clone();
    }
    Poll {
        pos: e.sh.pos.load(Ordering::Relaxed),
        playing: e.sh.playing.load(Ordering::Relaxed),
        recording: e.sh.recording.load(Ordering::Relaxed),
        rec_frames: e.sh.rec_frames.load(Ordering::Relaxed),
        rec_start: e.input.start_pos.load(Ordering::Relaxed),
        rec_rate: e.input.input_rate(),
        meters: std::mem::take(&mut *e.sh.meters.lock()),
        master,
        master_gr: gr,
        clip,
        input: e.input.levels(),
        input_open: e.input.open.load(Ordering::Relaxed),
        error: err,
        device_rate: e.sh.device_rate.load(Ordering::Relaxed),
    }
}

#[tauri::command]
pub fn spectrum(app: State<'_, Arc<App>>, bands: usize) -> Vec<f32> {
    app.engine.spectrum(bands.clamp(8, 256))
}

#[tauri::command]
pub fn audio_devices(app: State<'_, Arc<App>>) -> DeviceList {
    engine::devices(&app.engine)
}

#[tauri::command]
pub fn set_audio_device(app: State<'_, Arc<App>>, kind: String, name: Option<String>) {
    let name = name.filter(|n| !n.is_empty());
    if kind == "output" {
        app.engine.open_output(name);
    } else {
        app.engine.input.open_device(name);
    }
}

#[tauri::command]
pub fn input_monitor(app: State<'_, Arc<App>>, on: bool) {
    if on {
        if !app.engine.input.open.load(Ordering::Relaxed) {
            let n = app.engine.input.device_name.lock().clone();
            app.engine.input.open_device(if n.is_empty() { None } else { Some(n) });
        }
    } else {
        app.engine.input.close();
    }
}

#[tauri::command]
pub fn record_start(app: State<'_, Arc<App>>, pos: i64) {
    app.engine.seek(pos);
    app.engine.input.start(pos);
    app.engine.play();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Take {
    source: SourceInfo,
    start: i64,
}

#[tauri::command]
pub async fn record_stop(h: AppHandle, name: String) -> R<Option<Take>> {
    let app = app_state(&h);
    app.engine.pause();
    blocking(move || {
        let start = app.engine.input.start_pos.load(Ordering::Relaxed);
        let Some(buf) = app.engine.input.stop() else { return Ok(None) };
        let meta = Meta { title: name.clone(), source_rate: app.engine.input.input_rate(), ..Default::default() };
        let info = app.add(NewSource { name, path: None, kind: "recording", parent: None, stem: None, meta, buf });
        Ok(Some(Take { source: info, start }))
    })
    .await
}

// ───────────────────────────── editing & analysis ─────────────────────────────

#[tauri::command]
pub async fn apply_op(h: AppHandle, id: u64, start: usize, end: usize, op: Op, name: Option<String>) -> R<SourceInfo> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "process");
    blocking(move || {
        let s = app.get(id)?;
        emit(0.05, "");
        let out = ops::apply(&s.buf, start, end, &op, &cancel)?;
        emit(1.0, "");
        Ok(app.add(NewSource {
            name: name.unwrap_or_else(|| s.name.clone()),
            path: s.path.clone(),
            kind: "edit",
            parent: Some(s.parent.unwrap_or(s.id)),
            stem: s.stem.clone(),
            meta: s.meta.clone(),
            buf: out,
        }))
    })
    .await
}

/// Mixes the project range down to a new source (bounce / freeze).
#[tauri::command]
pub async fn bounce(h: AppHandle, project: Project, start: i64, end: i64, name: String) -> R<SourceInfo> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "bounce");
    blocking(move || {
        let src = app.engine.sources();
        let buf = mix::render_range(&project, &src, app.rate(), start, end, 0.0, &|f| emit(f, ""), &cancel);
        emit(1.0, "");
        if cancel.is_set() {
            return Err("cancelled".into());
        }
        Ok(app.add(NewSource { name, path: None, kind: "bounce", parent: None, stem: None, meta: Meta::default(), buf }))
    })
    .await
}

#[tauri::command]
pub async fn analyze(h: AppHandle, id: u64, start: usize, end: usize) -> R<Analysis> {
    let app = app_state(&h);
    blocking(move || {
        let s = app.get(id)?;
        let b = if start == 0 && (end == 0 || end >= s.buf.frames()) { (*s.buf).clone() } else { s.buf.slice(start, end) };
        Ok(analysis::analyze(&b))
    })
    .await
}

#[tauri::command]
pub async fn spectrogram(h: AppHandle, id: u64, start: usize, end: usize, width: usize, height: usize) -> R<Response> {
    let app = app_state(&h);
    blocking(move || {
        let s = app.get(id)?;
        let n = s.buf.frames();
        let e = if end == 0 { n } else { end.min(n) };
        let a = start.min(e);
        let mono: Vec<f32> = if s.buf.channels() == 1 {
            s.buf.ch[0][a..e].to_vec()
        } else {
            s.buf.ch[0][a..e].iter().zip(&s.buf.ch[1][a..e]).map(|(l, r)| (l + r) * 0.5).collect()
        };
        Ok(Response::new(spectral::spectrogram(&mono, s.buf.rate, width, height)))
    })
    .await
}

#[tauri::command]
pub fn eq_response(app: State<'_, Arc<App>>, bands: Vec<EqBand>, points: usize) -> Vec<f32> {
    openaudiopower_core::effects::eq_response(&bands, app.rate() as f32, points.clamp(16, 1024))
}

// ───────────────────────────── separation ─────────────────────────────

#[tauri::command]
pub fn separation_models(app: State<'_, Arc<App>>) -> Vec<models::ModelStatus> {
    models::status(&app.models_dir)
}

#[tauri::command]
pub async fn download_model(h: AppHandle, id: String) -> R<()> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "download");
    blocking(move || {
        let m = models::find(&id).ok_or("unknown model")?;
        let prog = Progress::default();
        let p2 = prog.clone();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d2 = done.clone();
        let e2 = emit.clone();
        let name = m.name.to_string();
        let ticker = std::thread::spawn(move || {
            while !d2.load(Ordering::Relaxed) {
                e2(p2.get().min(0.999), &name);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });
        let r = models::download(&app.models_dir, m, &prog, &cancel);
        done.store(true, Ordering::Relaxed);
        let _ = ticker.join();
        emit(1.0, "");
        r.map(|_| ())
    })
    .await
}

#[tauri::command]
pub fn delete_model(app: State<'_, Arc<App>>, id: String) -> R<()> {
    let m = models::find(&id).ok_or("unknown model")?;
    let p = models::path(&app.models_dir, m);
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SepResult {
    stems: Vec<SourceInfo>,
    device: String,
    seconds: f32,
}

#[tauri::command]
pub async fn separate_source(h: AppHandle, id: u64, start: usize, end: usize, options: SepOptions, keep: Vec<String>) -> R<SepResult> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "separate");
    blocking(move || {
        let s = app.get(id)?;
        let t0 = std::time::Instant::now();
        let n = s.buf.frames();
        let e = if end == 0 { n } else { end.min(n) };
        let part: AudioBuffer = if start == 0 && e == n { (*s.buf).clone() } else { s.buf.slice(start, e) };
        let (stems, device) = separate::separate(&part, &options, &app.models_dir, &|f, l| emit(f, l), &cancel)?;
        let mut out = Vec::new();
        for st in stems {
            if !keep.is_empty() && !keep.contains(&st.name) {
                continue;
            }
            let info = app.add(NewSource {
                name: format!("{} · {}", s.name, st.name),
                path: s.path.clone(),
                kind: "stem",
                parent: Some(s.id),
                stem: Some(st.name.clone()),
                meta: s.meta.clone(),
                buf: st.buf,
            });
            out.push(info);
        }
        Ok(SepResult { stems: out, device, seconds: t0.elapsed().as_secs_f32() })
    })
    .await
}

// ───────────────────────────── export ─────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    #[serde(flatten)]
    enc: EncodeOptions,
    #[serde(default)]
    start: i64,
    #[serde(default)]
    end: i64,
    #[serde(default)]
    tail: f32,
    /// Peak-normalise the result to this many dBFS.
    #[serde(default)]
    normalize: Option<f32>,
    /// Integrated loudness target (LUFS) – applied before peak limiting.
    #[serde(default)]
    lufs: Option<f32>,
    #[serde(default)]
    rate: Option<u32>,
}

fn finish(mut buf: AudioBuffer, o: &ExportOptions) -> AudioBuffer {
    let c = Cancel::default();
    if let Some(l) = o.lufs {
        buf = ops::apply(&buf, 0, 0, &Op::Normalize { mode: ops::NormMode::Lufs, target: l, per_channel: false }, &c).unwrap_or(buf);
        // keep the true peak below −1 dBFS after the loudness gain
        if buf.peak() > 0.89 {
            let spec = openaudiopower_core::effects::EffectSpec::Limiter { ceiling: -1.0, release: 60.0, input: 0.0, lookahead: 5.0 };
            buf = ops::apply(&buf, 0, 0, &Op::Effect { spec, tail: 0.0 }, &c).unwrap_or(buf);
        }
    }
    if let Some(p) = o.normalize {
        buf = ops::apply(&buf, 0, 0, &Op::Normalize { mode: ops::NormMode::Peak, target: p, per_channel: false }, &c).unwrap_or(buf);
    }
    if let Some(r) = o.rate {
        if r != buf.rate && r >= 8000 {
            buf = resample::resample(&buf, r);
        }
    }
    buf
}

#[tauri::command]
pub async fn export_mix(h: AppHandle, path: String, project: Project, options: ExportOptions) -> R<()> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "export");
    blocking(move || {
        let src = app.engine.sources();
        let end = if options.end > options.start { options.end } else { project.length() };
        let buf = mix::render_range(&project, &src, app.rate(), options.start, end, options.tail, &|f| emit(f * 0.8, ""), &cancel);
        if cancel.is_set() {
            return Err("cancelled".into());
        }
        let buf = finish(buf, &options);
        emit(0.9, "");
        encode::write(Path::new(&path), &buf, &options.enc)?;
        emit(1.0, "");
        Ok(())
    })
    .await
}

/// Each track rendered on its own (with its effects, without master), e.g. for stems export.
#[tauri::command]
pub async fn export_tracks(h: AppHandle, dir: String, project: Project, options: ExportOptions) -> R<Vec<String>> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "export");
    blocking(move || {
        let src = app.engine.sources();
        let end = if options.end > options.start { options.end } else { project.length() };
        let mut written = Vec::new();
        let k = project.tracks.len().max(1);
        for (i, t) in project.tracks.iter().enumerate() {
            if cancel.is_set() {
                return Err("cancelled".into());
            }
            let mut single = project.clone();
            single.master = Default::default();
            single.tracks = vec![{
                let mut t = t.clone();
                t.mute = false;
                t.solo = false;
                t
            }];
            let buf = mix::render_range(&single, &src, app.rate(), options.start, end, options.tail, &|_| {}, &cancel);
            let buf = finish(buf, &options);
            let ext = if options.enc.format == encode::Format::Flac { "flac" } else { "wav" };
            let safe: String = t.name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
            let p = Path::new(&dir).join(format!("{:02} {}.{ext}", i + 1, if safe.is_empty() { "track".into() } else { safe }));
            encode::write(&p, &buf, &options.enc)?;
            written.push(p.to_string_lossy().into_owned());
            emit((i + 1) as f32 / k as f32, &t.name);
        }
        Ok(written)
    })
    .await
}

#[tauri::command]
pub async fn export_source(h: AppHandle, id: u64, path: String, options: ExportOptions) -> R<()> {
    let app = app_state(&h);
    blocking(move || {
        let s = app.get(id)?;
        let n = s.buf.frames();
        let (a, b) = (options.start.max(0) as usize, if options.end > 0 { (options.end as usize).min(n) } else { n });
        let buf = if a == 0 && b == n { (*s.buf).clone() } else { s.buf.slice(a, b) };
        let buf = finish(buf, &options);
        encode::write(Path::new(&path), &buf, &options.enc)
    })
    .await
}

// ───────────────────────────── projects ─────────────────────────────

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedSource {
    id: u64,
    name: String,
    kind: String,
    file: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    parent: Option<u64>,
    #[serde(default)]
    stem: Option<String>,
    #[serde(default)]
    meta_title: String,
    #[serde(default)]
    meta_artist: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    app: String,
    version: u32,
    rate: u32,
    sources: Vec<SavedSource>,
    /// The UI's full state (tracks, clips, names, colours, markers, …)
    state: serde_json::Value,
}

/// Saves the project JSON plus every source it references as FLAC in `<name> audio/`.
#[tauri::command]
pub async fn save_project(h: AppHandle, path: String, state: serde_json::Value, used: Vec<u64>) -> R<()> {
    let app = app_state(&h);
    let emit = emitter(&h, "save");
    blocking(move || {
        let p = PathBuf::from(&path);
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("project").to_string();
        let dir = p.with_file_name(format!("{stem} audio"));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut saved = Vec::new();
        let total = used.len().max(1);
        for (i, id) in used.iter().enumerate() {
            let s = app.get(*id)?;
            let file = format!("{:05}.flac", s.id);
            let target = dir.join(&file);
            let already = s.saved_as.lock().as_ref().map(|x| x == &target && x.exists()).unwrap_or(false);
            if !already {
                encode::write(&target, &s.buf, &EncodeOptions { format: encode::Format::Flac, bits: 24, dither: false })?;
                *s.saved_as.lock() = Some(target.clone());
            }
            saved.push(SavedSource {
                id: s.id,
                name: s.name.clone(),
                kind: s.kind.clone(),
                file,
                path: s.path.clone(),
                parent: s.parent,
                stem: s.stem.clone(),
                meta_title: s.meta.title.clone(),
                meta_artist: s.meta.artist.clone(),
            });
            emit((i + 1) as f32 / total as f32, &s.name);
        }
        // drop audio files no longer referenced
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".flac") && !saved.iter().any(|s| s.file == name) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let pf = ProjectFile { app: "OpenAudioPower".into(), version: 1, rate: app.rate(), sources: saved, state };
        let json = serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("oap.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
        emit(1.0, "");
        Ok(())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedProject {
    state: serde_json::Value,
    sources: Vec<SourceInfo>,
    /// engine rate / file rate – the UI scales frame positions by this
    ratio: f64,
    missing: Vec<String>,
}

#[tauri::command]
pub async fn load_project(h: AppHandle, path: String) -> R<LoadedProject> {
    let app = app_state(&h);
    let cancel = app.new_job();
    let emit = emitter(&h, "open");
    blocking(move || {
        let p = PathBuf::from(&path);
        let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
        let pf: ProjectFile = serde_json::from_str(&text).map_err(|e| format!("project: {e}"))?;
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("project").to_string();
        let dir = p.with_file_name(format!("{stem} audio"));
        // a fresh project replaces everything that was loaded before
        let all: Vec<u64> = app.sources.read().keys().copied().collect();
        app.remove(&all);
        let total = pf.sources.len().max(1);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let results: Vec<(SavedSource, R<SourceInfo>)> = pf
            .sources
            .into_par_iter()
            .map(|s| {
                let f = dir.join(&s.file);
                let r = decode::decode_file(&f, &cancel, &|_| {}).map(|(buf, mut meta)| {
                    let buf = if buf.rate != app.rate() { resample::resample(&buf, app.rate()) } else { buf };
                    meta.title = s.meta_title.clone();
                    meta.artist = s.meta_artist.clone();
                    let kind: &'static str = match s.kind.as_str() {
                        "file" => "file",
                        "recording" => "recording",
                        "stem" => "stem",
                        "bounce" => "bounce",
                        _ => "edit",
                    };
                    let info = app.add_with_id(s.id, NewSource { name: s.name.clone(), path: s.path.clone(), kind, parent: s.parent, stem: s.stem.clone(), meta, buf });
                    if let Ok(e) = app.get(s.id) {
                        *e.saved_as.lock() = Some(f.clone());
                    }
                    info
                });
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                emit(d as f32 / total as f32, &s.name);
                (s, r)
            })
            .collect();
        let mut sources = Vec::new();
        let mut missing = Vec::new();
        for (s, r) in results {
            match r {
                Ok(i) => sources.push(i),
                Err(e) => missing.push(format!("{}: {e}", s.name)),
            }
        }
        sources.sort_by_key(|s| s.id);
        emit(1.0, "");
        Ok(LoadedProject { state: pf.state, sources, ratio: app.rate() as f64 / pf.rate.max(1) as f64, missing })
    })
    .await
}

#[tauri::command]
pub fn clear_sources(app: State<'_, Arc<App>>) {
    let all: Vec<u64> = app.sources.read().keys().copied().collect();
    app.remove(&all);
}
