//! Real-time audio engine: playback of the multitrack project (cpal / WASAPI), metronome,
//! loop playback, metering, live spectrum tap and recording from an input device.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use openaudiopower_core::buffer::AudioBuffer;
use openaudiopower_core::mix::{Mixer, Project, Sources, TrackMeter};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

const BLOCK: usize = 512;
const SPECTRUM: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Metronome {
    pub on: bool,
    pub bpm: f32,
    pub beats: u32,
    pub volume: f32,
    /// Only while recording
    #[serde(default)]
    pub rec_only: bool,
}

enum Cmd {
    Project(Box<Project>),
    Sources(Arc<Sources>),
    Play,
    Pause,
    Seek(i64),
    Loop(Option<(i64, i64)>),
    Metronome(Metronome),
}

#[derive(Default)]
pub struct Shared {
    pub pos: AtomicI64,
    pub playing: AtomicBool,
    pub recording: AtomicBool,
    pub meters: Mutex<Vec<TrackMeter>>,
    master: [AtomicU32; 4], // peak L, peak R, rms L, rms R
    master_gr: AtomicU32,
    clip: AtomicBool,
    spectrum: Mutex<(Vec<f32>, usize)>,
    pub input_peak: [AtomicU32; 2],
    pub rec_frames: AtomicU64,
    pub device_rate: AtomicU32,
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed)
}
fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

struct OutState {
    rx: Receiver<Cmd>,
    sh: Arc<Shared>,
    mixer: Mixer,
    sources: Arc<Sources>,
    playing: bool,
    pos: i64,
    looping: Option<(i64, i64)>,
    metro: Metronome,
    click_left: usize,
    click_freq: f32,
    click_phase: f32,
    rate: u32,
    // rendered block at project rate
    blk: Vec<Vec<f32>>,
    blk_len: usize,
    blk_pos: f64,
    step: f64,
    prev: [f32; 2],
    rms_acc: [f32; 2],
    rms_n: usize,
    peak_hold: [f32; 2],
}

impl OutState {
    fn drain(&mut self) {
        while let Ok(c) = self.rx.try_recv() {
            match c {
                Cmd::Project(p) => self.mixer.set_project(*p),
                Cmd::Sources(s) => self.sources = s,
                Cmd::Play => {
                    self.playing = true;
                }
                Cmd::Pause => {
                    self.playing = false;
                    self.mixer.reset();
                }
                Cmd::Seek(p) => {
                    self.pos = p.max(0);
                    self.mixer.reset();
                    self.click_left = 0;
                }
                Cmd::Loop(l) => self.looping = l.filter(|(a, b)| b > a),
                Cmd::Metronome(m) => self.metro = m,
            }
        }
    }

    /// Render the next project-rate block (BLOCK frames) into `self.blk`.
    fn render_block(&mut self) {
        let n = BLOCK;
        if self.playing {
            let mut done = 0;
            while done < n {
                let mut k = n - done;
                if let Some((a, b)) = self.looping {
                    if self.pos >= b || self.pos < a - (self.rate as i64) * 3600 {
                        self.pos = a;
                    }
                    if self.pos < b {
                        k = k.min((b - self.pos) as usize);
                    }
                }
                let mut tmp = std::mem::take(&mut self.blk);
                for c in tmp.iter_mut() {
                    if c.len() < n {
                        c.resize(n, 0.0);
                    }
                }
                let mut part: Vec<Vec<f32>> = vec![vec![0.0; k]; 2];
                self.mixer.render(&self.sources, self.pos, &mut part, k);
                for c in 0..2 {
                    tmp[c][done..done + k].copy_from_slice(&part[c][..k]);
                }
                self.blk = tmp;
                self.metronome(done, k);
                self.pos += k as i64;
                if let Some((a, b)) = self.looping {
                    if self.pos >= b {
                        self.pos = a;
                    }
                }
                done += k;
            }
            *self.sh.meters.lock() = self.mixer.meters.clone();
            store(&self.sh.master_gr, self.mixer.master_gr);
        } else {
            for c in self.blk.iter_mut() {
                c.resize(n, 0.0);
                c.iter_mut().for_each(|x| *x = 0.0);
            }
        }
        self.blk_len = n;
        self.sh.pos.store(self.pos, Ordering::Relaxed);
    }

    fn metronome(&mut self, off: usize, k: usize) {
        let m = self.metro;
        if !m.on || m.bpm <= 1.0 || (m.rec_only && !self.sh.recording.load(Ordering::Relaxed)) {
            return;
        }
        let fpb = self.rate as f64 * 60.0 / m.bpm as f64;
        let vol = m.volume.clamp(0.0, 1.0) * 0.5;
        let clen = (self.rate as f32 * 0.035) as usize;
        for i in 0..k {
            let p = (self.pos + i as i64) as f64;
            let beat = (p / fpb).floor();
            let start = (beat * fpb).round() as i64;
            if start == self.pos + i as i64 {
                self.click_left = clen;
                let accent = m.beats > 0 && (beat as i64) % (m.beats as i64) == 0;
                self.click_freq = if accent { 1760.0 } else { 1175.0 };
                self.click_phase = 0.0;
            }
            if self.click_left > 0 {
                let t = 1.0 - self.click_left as f32 / clen as f32;
                let env = (1.0 - t).powi(3);
                let s = (self.click_phase * std::f32::consts::TAU).sin() * env * vol;
                self.click_phase = (self.click_phase + self.click_freq / self.rate as f32).fract();
                self.blk[0][off + i] += s;
                self.blk[1][off + i] += s;
                self.click_left -= 1;
            }
        }
    }

    fn next_frame(&mut self) -> [f32; 2] {
        if (self.step - 1.0).abs() < 1e-9 {
            if self.blk_pos as usize >= self.blk_len {
                self.render_block();
                self.blk_pos = 0.0;
            }
            let i = self.blk_pos as usize;
            self.blk_pos += 1.0;
            return [self.blk[0][i], self.blk[1][i]];
        }
        // linear interpolation when the device runs at a different rate than the project
        while self.blk_pos >= self.blk_len as f64 {
            self.prev = [self.blk[0][self.blk_len.max(1) - 1], self.blk[1][self.blk_len.max(1) - 1]];
            self.render_block();
            self.blk_pos -= BLOCK as f64;
        }
        let i = self.blk_pos.floor();
        let f = (self.blk_pos - i) as f32;
        let i = i as isize;
        let get = |s: &Self, c: usize, j: isize| -> f32 {
            if j < 0 {
                s.prev[c]
            } else {
                s.blk[c][(j as usize).min(s.blk_len - 1)]
            }
        };
        let out = [
            get(self, 0, i - 1) + (get(self, 0, i) - get(self, 0, i - 1)) * f,
            get(self, 1, i - 1) + (get(self, 1, i) - get(self, 1, i - 1)) * f,
        ];
        self.blk_pos += self.step;
        out
    }

    fn fill<T: SizedSample + FromSample<f32>>(&mut self, data: &mut [T], channels: usize) {
        self.drain();
        let frames = data.len() / channels.max(1);
        let mut pk = [0.0f32; 2];
        let mut mono = Vec::with_capacity(frames);
        for f in 0..frames {
            let s = self.next_frame();
            pk[0] = pk[0].max(s[0].abs());
            pk[1] = pk[1].max(s[1].abs());
            self.rms_acc[0] += s[0] * s[0];
            self.rms_acc[1] += s[1] * s[1];
            mono.push((s[0] + s[1]) * 0.5);
            let base = f * channels;
            match channels {
                1 => data[base] = T::from_sample((s[0] + s[1]) * 0.5),
                _ => {
                    data[base] = T::from_sample(s[0].clamp(-1.0, 1.0));
                    data[base + 1] = T::from_sample(s[1].clamp(-1.0, 1.0));
                    for c in 2..channels {
                        data[base + c] = T::from_sample(0.0f32);
                    }
                }
            }
        }
        self.rms_n += frames;
        if pk[0] >= 1.0 || pk[1] >= 1.0 {
            self.sh.clip.store(true, Ordering::Relaxed);
        }
        // peak meters with ballistics: instant attack, fall ~20 dB/s handled by UI; here keep max since last poll
        self.peak_hold[0] = self.peak_hold[0].max(pk[0]);
        self.peak_hold[1] = self.peak_hold[1].max(pk[1]);
        store(&self.sh.master[0], self.peak_hold[0].max(load(&self.sh.master[0])));
        store(&self.sh.master[1], self.peak_hold[1].max(load(&self.sh.master[1])));
        self.peak_hold = [0.0; 2];
        if self.rms_n >= 2048 {
            store(&self.sh.master[2], (self.rms_acc[0] / self.rms_n as f32).sqrt());
            store(&self.sh.master[3], (self.rms_acc[1] / self.rms_n as f32).sqrt());
            self.rms_acc = [0.0; 2];
            self.rms_n = 0;
        }
        if let Some(mut g) = self.sh.spectrum.try_lock() {
            let (ring, w) = &mut *g;
            if ring.len() != SPECTRUM {
                ring.resize(SPECTRUM, 0.0);
            }
            for s in mono {
                ring[*w] = s;
                *w = (*w + 1) % SPECTRUM;
            }
        }
    }
}

enum Ctl {
    Open(Option<String>),
}

pub struct Engine {
    pub rate: u32,
    tx: Mutex<Sender<Cmd>>,
    ctl: Sender<Ctl>,
    pub sh: Arc<Shared>,
    project: Mutex<Project>,
    sources: Mutex<Arc<Sources>>,
    looping: Mutex<Option<(i64, i64)>>,
    metro: Mutex<Metronome>,
    pub output_name: Mutex<String>,
    pub input: Recorder,
    pub last_error: Mutex<String>,
}

fn find_output(host: &cpal::Host, name: &Option<String>) -> Option<cpal::Device> {
    if let Some(n) = name {
        if let Ok(mut it) = host.output_devices() {
            if let Some(d) = it.find(|d| d.name().map(|x| &x == n).unwrap_or(false)) {
                return Some(d);
            }
        }
    }
    host.default_output_device()
}

impl Engine {
    pub fn start(preferred: Option<String>) -> Arc<Engine> {
        let host = cpal::default_host();
        let rate = find_output(&host, &preferred)
            .and_then(|d| d.default_output_config().ok())
            .map(|c| c.sample_rate().0)
            .unwrap_or(48000)
            .clamp(22050, 192000);
        let (tx, _rx) = channel::<Cmd>();
        let (ctl_tx, ctl_rx) = channel::<Ctl>();
        let sh = Arc::new(Shared::default());
        let eng = Arc::new(Engine {
            rate,
            tx: Mutex::new(tx),
            ctl: ctl_tx,
            sh: sh.clone(),
            project: Mutex::new(Project::default()),
            sources: Mutex::new(Arc::new(Sources::new())),
            looping: Mutex::new(None),
            metro: Mutex::new(Metronome { on: false, bpm: 120.0, beats: 4, volume: 0.7, rec_only: false }),
            output_name: Mutex::new(String::new()),
            input: Recorder::new(sh, rate),
            last_error: Mutex::new(String::new()),
        });
        let e2 = eng.clone();
        std::thread::Builder::new()
            .name("audio-out".into())
            .spawn(move || output_thread(e2, ctl_rx))
            .ok();
        let _ = eng.ctl.send(Ctl::Open(preferred));
        eng
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.lock().send(c);
    }

    pub fn set_project(&self, p: Project) {
        *self.project.lock() = p.clone();
        self.send(Cmd::Project(Box::new(p)));
    }
    pub fn set_sources(&self, s: Arc<Sources>) {
        *self.sources.lock() = s.clone();
        self.send(Cmd::Sources(s));
    }
    pub fn play(&self) {
        self.sh.playing.store(true, Ordering::Relaxed);
        self.send(Cmd::Play);
    }
    pub fn pause(&self) {
        self.sh.playing.store(false, Ordering::Relaxed);
        self.send(Cmd::Pause);
    }
    pub fn seek(&self, pos: i64) {
        self.sh.pos.store(pos.max(0), Ordering::Relaxed);
        self.send(Cmd::Seek(pos));
    }
    pub fn set_loop(&self, l: Option<(i64, i64)>) {
        *self.looping.lock() = l;
        self.send(Cmd::Loop(l));
    }
    pub fn set_metronome(&self, m: Metronome) {
        *self.metro.lock() = m;
        self.send(Cmd::Metronome(m));
    }
    pub fn open_output(&self, name: Option<String>) {
        let _ = self.ctl.send(Ctl::Open(name));
    }
    pub fn sources(&self) -> Arc<Sources> {
        self.sources.lock().clone()
    }

    /// Reads and resets the master peak meters.
    pub fn master_meters(&self) -> ([f32; 4], f32, bool) {
        let m = [
            f32::from_bits(self.sh.master[0].swap(0, Ordering::Relaxed)),
            f32::from_bits(self.sh.master[1].swap(0, Ordering::Relaxed)),
            load(&self.sh.master[2]),
            load(&self.sh.master[3]),
        ];
        let clip = self.sh.clip.swap(false, Ordering::Relaxed);
        (m, load(&self.sh.master_gr), clip)
    }

    pub fn spectrum(&self, bands: usize) -> Vec<f32> {
        let snap = {
            let g = self.sh.spectrum.lock();
            let (ring, w) = &*g;
            if ring.len() != SPECTRUM {
                return vec![-120.0; bands];
            }
            let mut v = Vec::with_capacity(SPECTRUM);
            v.extend_from_slice(&ring[*w..]);
            v.extend_from_slice(&ring[..*w]);
            v
        };
        openaudiopower_core::spectral::band_spectrum(&snap, self.rate, bands)
    }
}

fn output_thread(eng: Arc<Engine>, ctl: Receiver<Ctl>) {
    let mut stream: Option<cpal::Stream> = None;
    while let Ok(msg) = ctl.recv() {
        match msg {
            Ctl::Open(name) => {
                drop(stream.take());
                let host = cpal::default_host();
                let Some(dev) = find_output(&host, &name) else {
                    *eng.last_error.lock() = "no output device".into();
                    continue;
                };
                *eng.output_name.lock() = dev.name().unwrap_or_default();
                // new command channel for the new callback; state is re-sent
                let (tx, rx) = channel::<Cmd>();
                let mut st = OutState {
                    rx,
                    sh: eng.sh.clone(),
                    mixer: Mixer::new(eng.rate),
                    sources: eng.sources.lock().clone(),
                    playing: false,
                    pos: eng.sh.pos.load(Ordering::Relaxed),
                    looping: *eng.looping.lock(),
                    metro: *eng.metro.lock(),
                    click_left: 0,
                    click_freq: 1000.0,
                    click_phase: 0.0,
                    rate: eng.rate,
                    blk: vec![vec![0.0; BLOCK]; 2],
                    blk_len: 0,
                    blk_pos: 0.0,
                    step: 1.0,
                    prev: [0.0; 2],
                    rms_acc: [0.0; 2],
                    rms_n: 0,
                    peak_hold: [0.0; 2],
                };
                st.mixer.set_project(eng.project.lock().clone());
                st.playing = eng.sh.playing.load(Ordering::Relaxed);
                *eng.tx.lock() = tx;
                match build_output(&dev, eng.rate, st) {
                    Ok((s, dev_rate)) => {
                        eng.sh.device_rate.store(dev_rate, Ordering::Relaxed);
                        if let Err(e) = s.play() {
                            *eng.last_error.lock() = e.to_string();
                        } else {
                            eng.last_error.lock().clear();
                        }
                        stream = Some(s);
                    }
                    Err(e) => *eng.last_error.lock() = e,
                }
            }
        }
    }
}

fn build_output(dev: &cpal::Device, rate: u32, mut st: OutState) -> Result<(cpal::Stream, u32), String> {
    // prefer a config running at the project rate, otherwise the device default (resampled)
    let mut chosen: Option<cpal::SupportedStreamConfig> = None;
    if let Ok(cfgs) = dev.supported_output_configs() {
        for c in cfgs {
            if c.channels() >= 2 && c.min_sample_rate().0 <= rate && c.max_sample_rate().0 >= rate && c.sample_format() == SampleFormat::F32 {
                chosen = Some(c.with_sample_rate(cpal::SampleRate(rate)));
                break;
            }
        }
    }
    let sup = match chosen {
        Some(c) => c,
        None => dev.default_output_config().map_err(|e| e.to_string())?,
    };
    let dev_rate = sup.sample_rate().0;
    st.step = rate as f64 / dev_rate as f64;
    let fmt = sup.sample_format();
    let cfg: StreamConfig = sup.into();
    let ch = cfg.channels as usize;
    let err = |e: cpal::StreamError| eprintln!("audio output: {e}");
    let s = match fmt {
        SampleFormat::F32 => dev.build_output_stream(&cfg, move |d: &mut [f32], _| st.fill(d, ch), err, None),
        SampleFormat::I16 => dev.build_output_stream(&cfg, move |d: &mut [i16], _| st.fill(d, ch), err, None),
        SampleFormat::I32 => dev.build_output_stream(&cfg, move |d: &mut [i32], _| st.fill(d, ch), err, None),
        SampleFormat::U16 => dev.build_output_stream(&cfg, move |d: &mut [u16], _| st.fill(d, ch), err, None),
        f => return Err(format!("unsupported sample format {f:?}")),
    }
    .map_err(|e| e.to_string())?;
    Ok((s, dev_rate))
}

// ───────────────────────────── recording ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct DeviceList {
    pub outputs: Vec<String>,
    pub inputs: Vec<String>,
    pub output: String,
    pub input: String,
    pub default_output: String,
    pub default_input: String,
}

pub fn devices(eng: &Engine) -> DeviceList {
    let host = cpal::default_host();
    let names = |it: Result<cpal::OutputDevices<cpal::Devices>, cpal::DevicesError>| -> Vec<String> {
        it.map(|i| i.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
    };
    let inputs: Vec<String> = host.input_devices().map(|i| i.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    DeviceList {
        outputs: names(host.output_devices()),
        inputs,
        output: eng.output_name.lock().clone(),
        input: eng.input.device_name.lock().clone(),
        default_output: host.default_output_device().and_then(|d| d.name().ok()).unwrap_or_default(),
        default_input: host.default_input_device().and_then(|d| d.name().ok()).unwrap_or_default(),
    }
}

enum InCtl {
    Open(Option<String>),
    Close,
}

pub struct Recorder {
    sh: Arc<Shared>,
    project_rate: u32,
    ctl: Mutex<Option<Sender<InCtl>>>,
    capturing: Arc<AtomicBool>,
    data: Arc<Mutex<Vec<f32>>>,
    in_rate: Arc<AtomicU32>,
    in_channels: Arc<AtomicU32>,
    pub device_name: Mutex<String>,
    pub start_pos: AtomicI64,
    pub open: AtomicBool,
    pub error: Arc<Mutex<String>>,
}

impl Recorder {
    fn new(sh: Arc<Shared>, project_rate: u32) -> Self {
        Self {
            sh,
            project_rate,
            ctl: Mutex::new(None),
            capturing: Arc::new(AtomicBool::new(false)),
            data: Arc::new(Mutex::new(Vec::new())),
            in_rate: Arc::new(AtomicU32::new(48000)),
            in_channels: Arc::new(AtomicU32::new(2)),
            device_name: Mutex::new(String::new()),
            start_pos: AtomicI64::new(0),
            open: AtomicBool::new(false),
            error: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Opens (or re-opens) the input device; metering starts immediately.
    pub fn open_device(&self, name: Option<String>) {
        self.close();
        let (tx, rx) = channel::<InCtl>();
        let sh = self.sh.clone();
        let capturing = self.capturing.clone();
        let data = self.data.clone();
        let in_rate = self.in_rate.clone();
        let in_ch = self.in_channels.clone();
        let err = self.error.clone();
        let (name_tx, name_rx) = channel::<String>();
        std::thread::Builder::new()
            .name("audio-in".into())
            .spawn(move || {
                let host = cpal::default_host();
                let mut stream: Option<cpal::Stream> = None;
                let open = |n: Option<String>, stream: &mut Option<cpal::Stream>| {
                    drop(stream.take());
                    let dev = n
                        .as_ref()
                        .and_then(|n| host.input_devices().ok().and_then(|mut it| it.find(|d| d.name().map(|x| &x == n).unwrap_or(false))))
                        .or_else(|| host.default_input_device());
                    let Some(dev) = dev else {
                        *err.lock() = "no input device".into();
                        let _ = name_tx.send(String::new());
                        return;
                    };
                    let _ = name_tx.send(dev.name().unwrap_or_default());
                    let Ok(sup) = dev.default_input_config() else {
                        *err.lock() = "input config".into();
                        return;
                    };
                    in_rate.store(sup.sample_rate().0, Ordering::Relaxed);
                    let ch = sup.channels() as usize;
                    in_ch.store(ch as u32, Ordering::Relaxed);
                    let fmt = sup.sample_format();
                    let cfg: StreamConfig = sup.into();
                    let sh2 = sh.clone();
                    let cap = capturing.clone();
                    let dat = data.clone();
                    let handle = move |samples: &[f32]| {
                        let mut pk = [0.0f32; 2];
                        for (i, s) in samples.iter().enumerate() {
                            let c = (i % ch).min(1);
                            pk[c] = pk[c].max(s.abs());
                        }
                        if ch == 1 {
                            pk[1] = pk[0];
                        }
                        for c in 0..2 {
                            let old = f32::from_bits(sh2.input_peak[c].load(Ordering::Relaxed));
                            sh2.input_peak[c].store(old.max(pk[c]).to_bits(), Ordering::Relaxed);
                        }
                        if cap.load(Ordering::Relaxed) {
                            let mut d = dat.lock();
                            d.extend_from_slice(samples);
                            sh2.rec_frames.store((d.len() / ch.max(1)) as u64, Ordering::Relaxed);
                        }
                    };
                    let e2 = err.clone();
                    let errf = move |e: cpal::StreamError| *e2.lock() = e.to_string();
                    let r = match fmt {
                        SampleFormat::F32 => {
                            let h = handle;
                            dev.build_input_stream(&cfg, move |d: &[f32], _| h(d), errf, None)
                        }
                        SampleFormat::I16 => {
                            let h = handle;
                            let mut tmp = Vec::new();
                            dev.build_input_stream(
                                &cfg,
                                move |d: &[i16], _| {
                                    tmp.clear();
                                    tmp.extend(d.iter().map(|s| *s as f32 / 32768.0));
                                    h(&tmp)
                                },
                                errf,
                                None,
                            )
                        }
                        SampleFormat::I32 => {
                            let h = handle;
                            let mut tmp = Vec::new();
                            dev.build_input_stream(
                                &cfg,
                                move |d: &[i32], _| {
                                    tmp.clear();
                                    tmp.extend(d.iter().map(|s| *s as f32 / 2147483648.0));
                                    h(&tmp)
                                },
                                errf,
                                None,
                            )
                        }
                        f => {
                            *err.lock() = format!("unsupported input format {f:?}");
                            return;
                        }
                    };
                    match r {
                        Ok(s) => {
                            if let Err(e) = s.play() {
                                *err.lock() = e.to_string();
                            } else {
                                err.lock().clear();
                            }
                            *stream = Some(s);
                        }
                        Err(e) => *err.lock() = e.to_string(),
                    }
                };
                while let Ok(m) = rx.recv() {
                    match m {
                        InCtl::Open(n) => open(n, &mut stream),
                        InCtl::Close => {
                            drop(stream.take());
                            break;
                        }
                    }
                }
            })
            .ok();
        let _ = tx.send(InCtl::Open(name));
        if let Ok(n) = name_rx.recv_timeout(std::time::Duration::from_secs(3)) {
            *self.device_name.lock() = n;
        }
        *self.ctl.lock() = Some(tx);
        self.open.store(true, Ordering::Relaxed);
    }

    pub fn close(&self) {
        if self.capturing.load(Ordering::Relaxed) {
            return;
        }
        if let Some(tx) = self.ctl.lock().take() {
            let _ = tx.send(InCtl::Close);
        }
        self.open.store(false, Ordering::Relaxed);
    }

    pub fn start(&self, pos: i64) {
        if !self.open.load(Ordering::Relaxed) {
            let name = self.device_name.lock().clone();
            self.open_device(if name.is_empty() { None } else { Some(name) });
        }
        self.data.lock().clear();
        self.sh.rec_frames.store(0, Ordering::Relaxed);
        self.start_pos.store(pos, Ordering::Relaxed);
        self.capturing.store(true, Ordering::Relaxed);
        self.sh.recording.store(true, Ordering::Relaxed);
    }

    /// Stops capturing and returns the take at the project rate.
    pub fn stop(&self) -> Option<AudioBuffer> {
        if !self.capturing.swap(false, Ordering::Relaxed) {
            return None;
        }
        self.sh.recording.store(false, Ordering::Relaxed);
        let data = std::mem::take(&mut *self.data.lock());
        let ch = self.in_channels.load(Ordering::Relaxed).max(1) as usize;
        let rate = self.in_rate.load(Ordering::Relaxed);
        if data.len() < ch * 64 {
            return None;
        }
        let mut b = AudioBuffer::from_interleaved(rate, ch, &data);
        if ch > 2 {
            b = b.to_stereo();
        }
        Some(openaudiopower_core::resample::resample(&b, self.project_rate))
    }

    pub fn levels(&self) -> [f32; 2] {
        [
            f32::from_bits(self.sh.input_peak[0].swap(0, Ordering::Relaxed)),
            f32::from_bits(self.sh.input_peak[1].swap(0, Ordering::Relaxed)),
        ]
    }

    pub fn input_rate(&self) -> u32 {
        self.in_rate.load(Ordering::Relaxed)
    }
}
