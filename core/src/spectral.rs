//! STFT utilities and spectral processing: spectrogram images, phase-vocoder time stretch
//! and pitch shift, spectral noise reduction.

use crate::Cancel;
use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;
use std::f32::consts::PI;

pub fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos()).collect()
}

/// Short-time Fourier transform of one channel. Frames are centred (signal is padded by n/2).
pub struct Stft {
    pub n: usize,
    pub hop: usize,
    pub win: Vec<f32>,
    planner: RealFftPlanner<f32>,
}

impl Stft {
    pub fn new(n: usize, hop: usize) -> Self {
        Self { n, hop, win: hann(n), planner: RealFftPlanner::new() }
    }

    pub fn bins(&self) -> usize {
        self.n / 2 + 1
    }

    pub fn frames_for(&self, len: usize) -> usize {
        len / self.hop + 1
    }

    pub fn forward(&mut self, x: &[f32]) -> Vec<Vec<Complex32>> {
        let fft = self.planner.plan_fft_forward(self.n);
        let half = self.n / 2;
        let frames = self.frames_for(x.len());
        let mut inp = fft.make_input_vec();
        let mut out = fft.make_output_vec();
        let mut res = Vec::with_capacity(frames);
        for f in 0..frames {
            let start = (f * self.hop) as isize - half as isize;
            for i in 0..self.n {
                let j = start + i as isize;
                let v = if j >= 0 && (j as usize) < x.len() { x[j as usize] } else { 0.0 };
                inp[i] = v * self.win[i];
            }
            let _ = fft.process(&mut inp, &mut out);
            res.push(out.clone());
        }
        res
    }

    /// Weighted overlap-add inverse; returns `len` samples.
    pub fn inverse(&mut self, spec: &[Vec<Complex32>], len: usize) -> Vec<f32> {
        let ifft = self.planner.plan_fft_inverse(self.n);
        let half = self.n / 2;
        let mut out = vec![0.0f32; len + self.n];
        let mut norm = vec![0.0f32; len + self.n];
        let mut buf = ifft.make_input_vec();
        let mut time = ifft.make_output_vec();
        let scale = 1.0 / self.n as f32;
        for (f, frame) in spec.iter().enumerate() {
            buf.copy_from_slice(frame);
            buf[0].im = 0.0;
            let last = buf.len() - 1;
            buf[last].im = 0.0;
            let _ = ifft.process(&mut buf, &mut time);
            let start = (f * self.hop) as isize - half as isize;
            for i in 0..self.n {
                let j = start + i as isize;
                if j >= 0 && (j as usize) < out.len() {
                    let w = self.win[i];
                    out[j as usize] += time[i] * scale * w;
                    norm[j as usize] += w * w;
                }
            }
        }
        out.truncate(len);
        for (o, n) in out.iter_mut().zip(norm.iter()) {
            if *n > 1e-6 {
                *o /= *n;
            }
        }
        out
    }
}

/// Spectrogram image: `width` columns × `height` rows (row 0 = top = highest frequency),
/// log-frequency axis 30 Hz – Nyquist, values 0..255 mapping −100..0 dBFS.
pub fn spectrogram(x: &[f32], rate: u32, width: usize, height: usize) -> Vec<u8> {
    let width = width.clamp(1, 4096);
    let height = height.clamp(1, 1024);
    let mut img = vec![0u8; width * height];
    if x.is_empty() {
        return img;
    }
    let n = if x.len() > rate as usize * 60 { 4096 } else { 2048 };
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let win = hann(n);
    let mut inp = fft.make_input_vec();
    let mut out = fft.make_output_vec();
    let bins = n / 2 + 1;
    let nyq = rate as f32 / 2.0;
    let fmin = 30.0f32;
    // row -> bin range
    let rows: Vec<(usize, usize)> = (0..height)
        .map(|r| {
            let t0 = 1.0 - (r as f32 + 1.0) / height as f32;
            let t1 = 1.0 - r as f32 / height as f32;
            let f0 = fmin * (nyq / fmin).powf(t0);
            let f1 = fmin * (nyq / fmin).powf(t1);
            let b0 = ((f0 / nyq) * (bins - 1) as f32).floor() as usize;
            let b1 = (((f1 / nyq) * (bins - 1) as f32).ceil() as usize).max(b0 + 1).min(bins);
            (b0.min(bins - 1), b1)
        })
        .collect();
    let norm = 2.0 / win.iter().sum::<f32>();
    for col in 0..width {
        let centre = ((col as f64 + 0.5) / width as f64 * x.len() as f64) as isize;
        let start = centre - n as isize / 2;
        for i in 0..n {
            let j = start + i as isize;
            inp[i] = if j >= 0 && (j as usize) < x.len() { x[j as usize] * win[i] } else { 0.0 };
        }
        let _ = fft.process(&mut inp, &mut out);
        for (r, &(b0, b1)) in rows.iter().enumerate() {
            let mut m = 0.0f32;
            for c in &out[b0..b1] {
                m = m.max(c.norm());
            }
            let db = 20.0 * (m * norm).max(1e-6).log10();
            let v = ((db + 100.0) / 100.0).clamp(0.0, 1.0);
            img[r * width + col] = (v * 255.0) as u8;
        }
    }
    img
}

/// Magnitude spectrum of a short block (for live analysers) in `bands` log bands, dB.
pub fn band_spectrum(x: &[f32], rate: u32, bands: usize) -> Vec<f32> {
    let n = x.len().next_power_of_two().max(256);
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let win = hann(x.len().max(1));
    let mut inp = fft.make_input_vec();
    for (i, v) in x.iter().enumerate() {
        inp[i] = v * win[i];
    }
    let mut out = fft.make_output_vec();
    let _ = fft.process(&mut inp, &mut out);
    let norm = 2.0 / win.iter().sum::<f32>().max(1.0);
    let nyq = rate as f32 / 2.0;
    let nb = out.len();
    (0..bands)
        .map(|b| {
            let f0 = 20.0 * (nyq / 20.0).powf(b as f32 / bands as f32);
            let f1 = 20.0 * (nyq / 20.0).powf((b + 1) as f32 / bands as f32);
            let i0 = ((f0 / nyq) * (nb - 1) as f32) as usize;
            let i1 = (((f1 / nyq) * (nb - 1) as f32) as usize).max(i0 + 1).min(nb);
            let m = out[i0.min(nb - 1)..i1].iter().map(|c| c.norm()).fold(0.0f32, f32::max);
            20.0 * (m * norm).max(1e-7).log10()
        })
        .collect()
}

#[inline]
fn princarg(p: f32) -> f32 {
    p - 2.0 * PI * ((p + PI) / (2.0 * PI)).floor()
}

/// Phase-vocoder time stretch with identity phase locking. `ratio` > 1 makes audio longer.
pub fn time_stretch(x: &[f32], ratio: f32, cancel: &Cancel) -> Vec<f32> {
    let ratio = ratio.clamp(0.25, 4.0);
    if (ratio - 1.0).abs() < 1e-4 || x.is_empty() {
        return x.to_vec();
    }
    let n = 4096;
    let hs = 1024; // synthesis hop
    let ha = ((hs as f32) / ratio).round().max(1.0) as usize; // analysis hop
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let win = hann(n);
    let bins = n / 2 + 1;
    let out_len = (x.len() as f32 * ratio) as usize;
    let frames = x.len() / ha + 2;
    let mut out = vec![0.0f32; out_len + n * 2];
    let mut norm = vec![0.0f32; out_len + n * 2];
    let mut prev_phase = vec![0.0f32; bins];
    let mut sum_phase = vec![0.0f32; bins];
    let omega: Vec<f32> = (0..bins).map(|k| 2.0 * PI * k as f32 * ha as f32 / n as f32).collect();
    let mut inp = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let mut time = ifft.make_output_vec();
    let mut mag = vec![0.0f32; bins];
    let mut phase = vec![0.0f32; bins];
    let mut first = true;
    for f in 0..frames {
        if f % 64 == 0 && cancel.is_set() {
            break;
        }
        let a0 = (f * ha) as isize - (n / 2) as isize;
        for i in 0..n {
            let j = a0 + i as isize;
            inp[i] = if j >= 0 && (j as usize) < x.len() { x[j as usize] * win[i] } else { 0.0 };
        }
        let _ = fft.process(&mut inp, &mut spec);
        for k in 0..bins {
            mag[k] = spec[k].norm();
            phase[k] = spec[k].arg();
        }
        if first {
            sum_phase.copy_from_slice(&phase);
            first = false;
        } else {
            // advance peaks properly, lock surrounding bins to their peak (identity phase locking)
            let mut new_phase = vec![0.0f32; bins];
            let mut peak_of = vec![0usize; bins];
            let mut peaks = Vec::new();
            for k in 0..bins {
                let l = if k > 0 { mag[k - 1] } else { 0.0 };
                let r = if k + 1 < bins { mag[k + 1] } else { 0.0 };
                if mag[k] >= l && mag[k] > r {
                    peaks.push(k);
                }
            }
            if peaks.is_empty() {
                peaks.push(0);
            }
            let mut pi = 0;
            for (k, po) in peak_of.iter_mut().enumerate() {
                while pi + 1 < peaks.len() && (peaks[pi + 1] as isize - k as isize).abs() < (peaks[pi] as isize - k as isize).abs() {
                    pi += 1;
                }
                *po = peaks[pi];
            }
            for &p in &peaks {
                let dphi = princarg(phase[p] - prev_phase[p] - omega[p]);
                let inst = omega[p] + dphi;
                new_phase[p] = sum_phase[p] + inst * hs as f32 / ha as f32;
            }
            for k in 0..bins {
                let p = peak_of[k];
                if p != k {
                    new_phase[k] = new_phase[p] + (phase[k] - phase[p]);
                }
            }
            sum_phase.copy_from_slice(&new_phase);
        }
        prev_phase.copy_from_slice(&phase);
        for k in 0..bins {
            spec[k] = Complex32::from_polar(mag[k], sum_phase[k]);
        }
        spec[0].im = 0.0;
        spec[bins - 1].im = 0.0;
        let _ = ifft.process(&mut spec, &mut time);
        let s0 = f * hs;
        for i in 0..n {
            let j = s0 + i;
            if j < out.len() {
                out[j] += time[i] * win[i] / n as f32;
                norm[j] += win[i] * win[i];
            }
        }
    }
    // remove the half-window offset introduced by centring
    let off = n / 2;
    let mut res: Vec<f32> = (0..out_len).map(|i| {
        let j = i + off;
        if j < out.len() && norm[j] > 1e-3 { out[j] / norm[j] } else { 0.0 }
    }).collect();
    res.truncate(out_len);
    res
}

/// Pitch shift by `semitones` keeping duration: stretch then resample.
pub fn pitch_shift(x: &[f32], semitones: f32, cancel: &Cancel) -> Vec<f32> {
    if semitones.abs() < 1e-3 {
        return x.to_vec();
    }
    let factor = 2f32.powf(semitones / 12.0);
    let stretched = time_stretch(x, factor, cancel);
    let mut y = crate::resample::resample_channel(&stretched, 1.0 / factor as f64);
    y.resize(x.len(), 0.0);
    y
}

/// Spectral noise reduction. `profile` (optional) is a noise-only excerpt; without it the
/// noise floor is estimated per bin from the quietest frames. `reduction_db` is the maximum
/// attenuation, `sensitivity` (0..1) how aggressively bins are classified as noise.
pub fn denoise(x: &[f32], profile: Option<&[f32]>, reduction_db: f32, sensitivity: f32, cancel: &Cancel) -> Vec<f32> {
    if x.is_empty() {
        return Vec::new();
    }
    let n = 2048;
    let mut st = Stft::new(n, n / 4);
    let mut spec = st.forward(x);
    let bins = st.bins();
    let noise: Vec<f32> = {
        let src = match profile {
            Some(p) if p.len() > n => st.forward(p),
            _ => spec.clone(),
        };
        let mut per_bin = vec![0.0f32; bins];
        let mut col: Vec<f32> = Vec::with_capacity(src.len());
        for k in 0..bins {
            col.clear();
            col.extend(src.iter().map(|fr| fr[k].norm()));
            col.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let q = if profile.is_some() { 0.5 } else { 0.1 };
            let idx = ((col.len() as f32 - 1.0) * q) as usize;
            // mean of the quantile area is a stable floor estimate
            let lo = idx.saturating_sub(2);
            let hi = (idx + 3).min(col.len());
            per_bin[k] = col[lo..hi].iter().sum::<f32>() / (hi - lo).max(1) as f32;
        }
        per_bin
    };
    let floor = 10f32.powf(-reduction_db.abs() / 20.0);
    let over = 1.0 + sensitivity.clamp(0.0, 1.0) * 3.0;
    let mut prev = vec![1.0f32; bins];
    for (fi, fr) in spec.iter_mut().enumerate() {
        if fi % 256 == 0 && cancel.is_set() {
            break;
        }
        let mut g = vec![0.0f32; bins];
        for k in 0..bins {
            let m = fr[k].norm();
            let nk = noise[k] * over;
            let snr = (m * m) / (nk * nk + 1e-12);
            // Wiener-style gain
            let w = (1.0 - 1.0 / snr.max(1e-6)).max(0.0);
            g[k] = w.max(floor);
        }
        // frequency smoothing (3 bins) and temporal smoothing (attack fast, release slower)
        let gs: Vec<f32> = (0..bins).map(|k| {
            let a = g[k.saturating_sub(1)];
            let b = g[k];
            let c = g[(k + 1).min(bins - 1)];
            (a + 2.0 * b + c) / 4.0
        }).collect();
        for k in 0..bins {
            let v = if gs[k] > prev[k] { gs[k] } else { prev[k] * 0.6 + gs[k] * 0.4 };
            prev[k] = v;
            fr[k] *= v;
        }
    }
    st.inverse(&spec, x.len())
}
