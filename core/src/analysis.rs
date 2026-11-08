//! Measurement: loudness (EBU R128), peaks, RMS, DC offset, stereo correlation, tempo and key.

use crate::buffer::{gain_to_db, AudioBuffer};
use crate::spectral::Stft;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub duration: f64,
    pub rate: u32,
    pub channels: usize,
    pub peak_db: f32,
    pub true_peak_db: f32,
    pub rms_db: f32,
    pub lufs: f64,
    pub lufs_short_max: f64,
    pub lra: f64,
    pub crest_db: f32,
    pub dc_offset: f32,
    pub correlation: f32,
    pub clipped: usize,
    pub bpm: f32,
    pub bpm_confidence: f32,
    pub key: String,
    pub key_confidence: f32,
    pub silence_ratio: f32,
}

pub fn rms(b: &AudioBuffer) -> f32 {
    let n = (b.frames() * b.channels()).max(1);
    (b.ch.iter().flatten().map(|x| (*x as f64) * (*x as f64)).sum::<f64>() / n as f64).sqrt() as f32
}

pub fn integrated_lufs(b: &AudioBuffer) -> f64 {
    use ebur128::{EbuR128, Mode};
    let Ok(mut m) = EbuR128::new(b.channels().max(1) as u32, b.rate, Mode::I) else { return f64::NEG_INFINITY };
    let refs: Vec<&[f32]> = b.ch.iter().map(|c| c.as_slice()).collect();
    if m.add_frames_planar_f32(&refs).is_err() {
        return f64::NEG_INFINITY;
    }
    m.loudness_global().unwrap_or(f64::NEG_INFINITY)
}

pub fn analyze(b: &AudioBuffer) -> Analysis {
    use ebur128::{EbuR128, Mode};
    let mut a = Analysis { duration: b.duration(), rate: b.rate, channels: b.channels(), ..Default::default() };
    if b.frames() == 0 {
        return a;
    }
    let peak = b.peak();
    a.peak_db = gain_to_db(peak);
    let r = rms(b);
    a.rms_db = gain_to_db(r);
    a.crest_db = a.peak_db - a.rms_db;
    a.clipped = b.ch.iter().flatten().filter(|x| x.abs() >= 0.9999).count();
    let n = b.frames() as f64;
    a.dc_offset = (b.ch.iter().map(|c| c.iter().map(|x| *x as f64).sum::<f64>() / n).sum::<f64>() / b.channels() as f64) as f32;
    if b.channels() >= 2 {
        let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
        for (l, r) in b.ch[0].iter().zip(&b.ch[1]) {
            lr += (*l as f64) * (*r as f64);
            ll += (*l as f64) * (*l as f64);
            rr += (*r as f64) * (*r as f64);
        }
        a.correlation = if ll * rr > 0.0 { (lr / (ll * rr).sqrt()) as f32 } else { 1.0 };
    } else {
        a.correlation = 1.0;
    }
    if let Ok(mut m) = EbuR128::new(b.channels() as u32, b.rate, Mode::I | Mode::LRA | Mode::TRUE_PEAK | Mode::S) {
        let refs: Vec<&[f32]> = b.ch.iter().map(|c| c.as_slice()).collect();
        // feed in 100 ms blocks to track the short-term maximum
        let blk = (b.rate / 10) as usize;
        let mut pos = 0;
        let mut st_max = f64::NEG_INFINITY;
        while pos < b.frames() {
            let e = (pos + blk).min(b.frames());
            let part: Vec<&[f32]> = refs.iter().map(|c| &c[pos..e]).collect();
            let _ = m.add_frames_planar_f32(&part);
            if let Ok(s) = m.loudness_shortterm() {
                if s.is_finite() {
                    st_max = st_max.max(s);
                }
            }
            pos = e;
        }
        a.lufs = m.loudness_global().unwrap_or(f64::NEG_INFINITY);
        a.lra = m.loudness_range().unwrap_or(0.0);
        a.lufs_short_max = st_max;
        let mut tp = 0.0f64;
        for c in 0..b.channels() as u32 {
            tp = tp.max(m.true_peak(c).unwrap_or(0.0));
        }
        a.true_peak_db = gain_to_db(tp as f32);
    }
    let mono = b.mono();
    let quiet = mono.chunks(1024).filter(|c| c.iter().map(|x| x * x).sum::<f32>() / (c.len() as f32) < 1e-6).count();
    a.silence_ratio = quiet as f32 / mono.len().div_ceil(1024).max(1) as f32;
    let (bpm, conf) = tempo(&mono, b.rate);
    a.bpm = bpm;
    a.bpm_confidence = conf;
    let (key, kc) = key(&mono, b.rate);
    a.key = key;
    a.key_confidence = kc;
    a
}

/// Tempo estimate from spectral-flux onset envelope autocorrelation (60–200 BPM).
pub fn tempo(x: &[f32], rate: u32) -> (f32, f32) {
    if x.len() < rate as usize * 4 {
        return (0.0, 0.0);
    }
    // limit analysis to 90 s from the middle for speed
    let max = rate as usize * 90;
    let x = if x.len() > max { &x[(x.len() - max) / 2..(x.len() + max) / 2] } else { x };
    let mut st = Stft::new(1024, 512);
    let spec = st.forward(x);
    let fps = rate as f32 / 512.0;
    let mut prev: Vec<f32> = vec![0.0; st.bins()];
    let mut env: Vec<f32> = Vec::with_capacity(spec.len());
    for fr in &spec {
        let mut flux = 0.0;
        for (k, c) in fr.iter().enumerate() {
            let m = (1.0 + 100.0 * c.norm()).ln();
            flux += (m - prev[k]).max(0.0);
            prev[k] = m;
        }
        env.push(flux);
    }
    // remove local mean
    let w = 16;
    let envm: Vec<f32> = (0..env.len())
        .map(|i| {
            let lo = i.saturating_sub(w);
            let hi = (i + w).min(env.len());
            let mean = env[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
            (env[i] - mean).max(0.0)
        })
        .collect();
    let lag_min = (fps * 60.0 / 200.0) as usize;
    let lag_max = (fps * 60.0 / 60.0) as usize;
    let mut best = (0usize, 0.0f32);
    let mut scores = Vec::new();
    let e0: f32 = envm.iter().map(|v| v * v).sum::<f32>().max(1e-9);
    for lag in lag_min..=lag_max.min(envm.len() / 2) {
        let mut s = 0.0;
        for i in 0..envm.len() - lag {
            s += envm[i] * envm[i + lag];
        }
        let bpm = 60.0 * fps / lag as f32;
        // perceptual weighting around 120 BPM
        let wgt = (-(((bpm / 120.0).log2()) * 1.2).powi(2)).exp();
        let v = s / e0 * wgt;
        scores.push(v);
        if v > best.1 {
            best = (lag, v);
        }
    }
    if best.0 == 0 {
        return (0.0, 0.0);
    }
    // parabolic refinement
    let i = best.0 - lag_min;
    let lag = if i > 0 && i + 1 < scores.len() {
        let (a, b, c) = (scores[i - 1], scores[i], scores[i + 1]);
        let d = a - 2.0 * b + c;
        best.0 as f32 + if d.abs() > 1e-9 { 0.5 * (a - c) / d } else { 0.0 }
    } else {
        best.0 as f32
    };
    let mean = scores.iter().sum::<f32>() / scores.len() as f32;
    let conf = ((best.1 - mean) / best.1.max(1e-9)).clamp(0.0, 1.0);
    let bpm = 60.0 * fps / lag;
    ((bpm * 10.0).round() / 10.0, conf)
}

const NOTES: [&str; 12] = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

/// Musical key via chroma profile correlation (Krumhansl–Schmuckler).
pub fn key(x: &[f32], rate: u32) -> (String, f32) {
    if x.len() < rate as usize * 4 {
        return (String::new(), 0.0);
    }
    let max = rate as usize * 120;
    let x = if x.len() > max { &x[(x.len() - max) / 2..(x.len() + max) / 2] } else { x };
    let n = 8192;
    let mut st = Stft::new(n, n / 2);
    let spec = st.forward(x);
    let mut chroma = [0.0f32; 12];
    let bin_hz = rate as f32 / n as f32;
    for fr in &spec {
        for (k, c) in fr.iter().enumerate().skip(1) {
            let f = k as f32 * bin_hz;
            if !(55.0..=2000.0).contains(&f) {
                continue;
            }
            let midi = 69.0 + 12.0 * (f / 440.0).log2();
            let pc = (midi.round() as i32).rem_euclid(12) as usize;
            chroma[pc] += c.norm().powi(2).sqrt();
        }
    }
    let corr = |profile: &[f32; 12], shift: usize| -> f32 {
        let mx = chroma.iter().sum::<f32>() / 12.0;
        let my = profile.iter().sum::<f32>() / 12.0;
        let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            let xv = chroma[(i + shift) % 12] - mx;
            let yv = profile[i] - my;
            sxy += xv * yv;
            sxx += xv * xv;
            syy += yv * yv;
        }
        sxy / (sxx * syy).sqrt().max(1e-9)
    };
    let mut all: Vec<(f32, String)> = Vec::new();
    for s in 0..12 {
        all.push((corr(&MAJOR, s), format!("{} dur", NOTES[s])));
        all.push((corr(&MINOR, s), format!("{} moll", NOTES[s])));
    }
    all.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let conf = (all[0].0 - all[1].0).clamp(0.0, 1.0) * 4.0;
    (all[0].1.clone(), conf.min(1.0))
}
