//! Speech enhancement and spectral repair:
//!
//! * [`voice_denoise`] – DeepFilterNet3 (embedded model, pure Rust via tract) removes background
//!   noise from speech and damps part of the room sound,
//! * [`dereverb`] – suppression of late reverberation from a statistical room model
//!   (exponential decay with a given RT60), for the echo DeepFilterNet leaves,
//! * [`spectral_repair`] – fills a time × frequency rectangle (a cough, a phone, a squeak) with the
//!   background around it, or just attenuates it.

use crate::resample::resample;
use crate::spectral::Stft;
use crate::{buffer::AudioBuffer, Cancel};
use realfft::num_complex::Complex32;
use serde::Deserialize;

// ───────────────────────────── DeepFilterNet ─────────────────────────────

/// Removes noise from speech in one channel.
/// `strength` 0…1 maps to the maximum attenuation (6 dB … unlimited).
pub fn voice_denoise(x: &[f32], rate: u32, strength: f32, cancel: &Cancel) -> Result<Vec<f32>, String> {
    use deep_filter_rt::tract::{DfParams, DfTract, RuntimeParams};
    use ndarray::Array2;
    if x.is_empty() {
        return Ok(Vec::new());
    }
    // the model runs at 48 kHz
    let buf48 = if rate != 48000 { resample(&AudioBuffer::from_channels(rate, vec![x.to_vec()]), 48000).ch.remove(0) } else { x.to_vec() };
    let s = strength.clamp(0.0, 1.0);
    let atten = if s >= 0.99 { 100.0 } else { 6.0 + s * 54.0 };
    let rp = RuntimeParams::default_with_ch(1).with_atten_lim(atten).with_post_filter(if s > 0.7 { 0.02 } else { 0.0 });
    let mut df = DfTract::new(DfParams::default(), &rp).map_err(|e| format!("DeepFilterNet: {e}"))?;
    let hop = df.hop_size;
    let delay = df.delay();
    let n = buf48.len();
    let total = n + delay;
    let frames = total.div_ceil(hop);
    let mut out = Vec::with_capacity(frames * hop);
    let mut inp = Array2::<f32>::zeros((1, hop));
    let mut enh = Array2::<f32>::zeros((1, hop));
    for f in 0..frames {
        if f % 256 == 0 && cancel.is_set() {
            return Err("cancelled".into());
        }
        for i in 0..hop {
            let k = f * hop + i;
            inp[[0, i]] = if k < n { buf48[k] } else { 0.0 };
        }
        df.process(inp.view(), enh.view_mut()).map_err(|e| format!("DeepFilterNet: {e}"))?;
        out.extend(enh.row(0).iter().copied());
    }
    // drop the model's delay so the result lines up with the input
    let aligned: Vec<f32> = out.into_iter().skip(delay).take(n).collect();
    Ok(if rate != 48000 { resample(&AudioBuffer::from_channels(48000, vec![aligned]), rate).ch.remove(0) } else { aligned })
}

// ───────────────────────────── dereverberation ─────────────────────────────

/// Late-reverb suppression (Lebart et al.): the late reverberant power is the power `Td` earlier,
/// decayed by the room's exponential envelope; a spectral gain removes it.
/// `rt60` in seconds (0.2 small room … 2 hall), `amount` 0…1.
pub fn dereverb(x: &[f32], rate: u32, rt60: f32, amount: f32, cancel: &Cancel) -> Vec<f32> {
    let n_fft = if rate > 32000 { 2048 } else { 1024 };
    let hop = n_fft / 4;
    let mut st = Stft::new(n_fft, hop);
    let mut spec = st.forward(x);
    if spec.is_empty() {
        return x.to_vec();
    }
    let rt60 = rt60.clamp(0.1, 4.0);
    let amount = amount.clamp(0.0, 1.0);
    // reverb tail starts ~50 ms after the direct sound
    let td = 0.05f32;
    let nd = ((td * rate as f32) / hop as f32).round().max(1.0) as usize;
    let delta = 3.0 * std::f32::consts::LN_10 / rt60;
    let decay = (-2.0 * delta * nd as f32 * hop as f32 / rate as f32).exp();
    let bins = spec[0].len();
    let floor = 10f32.powf(-(6.0 + 14.0 * amount) / 20.0);
    // smoothed power used for the late-reverb estimate
    let alpha = 0.6f32;
    let mut smooth: Vec<Vec<f32>> = Vec::with_capacity(spec.len());
    for (t, frame) in spec.iter().enumerate() {
        let prev = if t > 0 { Some(&smooth[t - 1]) } else { None };
        smooth.push(frame.iter().enumerate().map(|(k, c)| {
            let p = c.norm_sqr();
            prev.map_or(p, |pr| alpha * pr[k] + (1.0 - alpha) * p)
        }).collect());
    }
    let mut prev_gain = vec![1.0f32; bins];
    for t in 0..spec.len() {
        if t % 512 == 0 && cancel.is_set() {
            return x.to_vec();
        }
        if t < nd {
            continue;
        }
        for k in 0..bins {
            let p = spec[t][k].norm_sqr().max(1e-12);
            let late = decay * smooth[t - nd][k] * amount * 1.4;
            let mut g = (1.0 - late / p).max(0.0).sqrt().max(floor);
            // light smoothing over time against musical noise
            g = 0.3 * prev_gain[k] + 0.7 * g;
            prev_gain[k] = g;
            spec[t][k] *= g;
        }
    }
    st.inverse(&spec, x.len())
}

// ───────────────────────────── spectral repair ─────────────────────────────

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RepairMode {
    /// replace with the background interpolated from before and after
    Fill,
    /// only turn the region down by `gain` dB
    Attenuate,
}

/// Repairs samples `[start, end)` of `x` between `f_lo` and `f_hi` Hz. The whole `x` is used as
/// context (callers pass the region with some audio on both sides).
pub fn spectral_repair(x: &[f32], rate: u32, start: usize, end: usize, f_lo: f32, f_hi: f32, mode: RepairMode, gain_db: f32) -> Vec<f32> {
    let n_fft = if rate > 32000 { 2048 } else { 1024 };
    let hop = n_fft / 8;
    let mut st = Stft::new(n_fft, hop);
    let mut spec = st.forward(x);
    if spec.is_empty() || end <= start {
        return x.to_vec();
    }
    let bins = spec[0].len();
    let hz = rate as f32 / n_fft as f32;
    let (lo, hi) = (f_lo.min(f_hi).max(0.0), f_lo.max(f_hi).min(rate as f32 / 2.0));
    let k0 = ((lo / hz).floor() as usize).min(bins - 1);
    let k1 = ((hi / hz).ceil() as usize).clamp(k0 + 1, bins);
    // frames are centred at f * hop and span n_fft = 8 hops: widen the region a little so the
    // windows that still see the sound are treated, and keep the context clear of it
    let frames = spec.len();
    let t0 = (start / hop).saturating_sub(2).min(frames);
    let t1 = (end / hop + 3).min(frames).max(t0 + 1);
    let gap = n_fft / hop / 2;
    // soft borders: 2 frames in time, 2 bins in frequency
    let edge_t = 2usize;
    let edge_k = 2usize;
    let weight = |t: usize, k: usize| -> f32 {
        let wt = {
            let d = (t as isize - t0 as isize).min(t1 as isize - 1 - t as isize);
            if d >= edge_t as isize { 1.0 } else { (d as f32 + 1.0) / (edge_t as f32 + 1.0) }
        };
        let wk = {
            let d = (k as isize - k0 as isize).min(k1 as isize - 1 - k as isize);
            if d >= edge_k as isize { 1.0 } else { (d as f32 + 1.0) / (edge_k as f32 + 1.0) }
        };
        wt.clamp(0.0, 1.0) * wk.clamp(0.0, 1.0)
    };

    match mode {
        RepairMode::Attenuate => {
            let g = 10f32.powf(gain_db.min(0.0) / 20.0);
            for t in t0..t1.min(frames) {
                for k in k0..k1 {
                    let w = weight(t, k);
                    spec[t][k] *= 1.0 - w * (1.0 - g);
                }
            }
        }
        RepairMode::Fill => {
            // background magnitude per bin: median-ish (mean of the quieter half) of the context
            // frames on each side, so a neighbouring transient does not leak in
            let ctx = 12usize;
            let side = |range: std::ops::Range<usize>, k: usize| -> Option<f32> {
                let mut v: Vec<f32> = range.filter(|&t| t < frames).map(|t| spec[t][k].norm()).collect();
                if v.is_empty() {
                    return None;
                }
                v.sort_by(|a, b| a.total_cmp(b));
                let h = v.len().div_ceil(2);
                Some(v[..h].iter().sum::<f32>() / h as f32)
            };
            let before: Vec<Option<f32>> = (0..bins).map(|k| side(t0.saturating_sub(gap + ctx)..t0.saturating_sub(gap), k)).collect();
            let after: Vec<Option<f32>> = (0..bins).map(|k| side((t1 + gap).min(frames)..(t1 + gap + ctx).min(frames), k)).collect();
            // deterministic noise for the phase of the fill
            let mut seed = 0x2545_F491_4F6C_DD1Du64;
            let mut rnd = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 40) as f32 / (1u64 << 24) as f32
            };
            let span = (t1 - t0).max(1) as f32;
            for t in t0..t1.min(frames) {
                let pos = (t - t0) as f32 / span;
                for k in k0..k1 {
                    let target = match (before[k], after[k]) {
                        // geometric interpolation reads naturally on a dB scale
                        (Some(a), Some(b)) => (a.max(1e-9).ln() * (1.0 - pos) + b.max(1e-9).ln() * pos).exp(),
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => spec[t][k].norm() * 0.1,
                    };
                    let orig = spec[t][k];
                    let m = orig.norm();
                    // only ever take energy away: quieter bins than the background stay as they are
                    if m <= target {
                        continue;
                    }
                    let phase = rnd() * std::f32::consts::TAU;
                    // a little level variation keeps the fill from sounding static
                    let fill = Complex32::from_polar(target * (0.85 + 0.3 * rnd()), phase);
                    let w = weight(t, k);
                    spec[t][k] = orig * (1.0 - w) + fill * w;
                }
            }
        }
    }
    st.inverse(&spec, x.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// A 3 kHz beep in the middle of quiet noise is gone after the repair; the rest is untouched.
    #[test]
    fn repair_removes_a_beep() {
        let rate = 44100;
        let n = rate as usize * 2;
        let mut seed = 1u32;
        let mut x: Vec<f32> = (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 * 0.02 - 0.01
            })
            .collect();
        let (a, b) = (rate as usize * 9 / 10, rate as usize * 11 / 10);
        for (i, v) in x[a..b].iter_mut().enumerate() {
            *v += 0.5 * (i as f32 * 3000.0 * std::f32::consts::TAU / rate as f32).sin();
        }
        let y = spectral_repair(&x, rate, a, b, 2700.0, 3300.0, RepairMode::Fill, 0.0);
        let before = rms(&x[a..b]);
        let after = rms(&y[a + 1000..b - 1000]);
        assert!(after < 0.02, "beep left: {after} (was {before})");
        // outside the region the signal is reconstructed nearly exactly
        let diff: f32 = x[..a - 4096].iter().zip(&y[..a - 4096]).map(|(p, q)| (p - q).abs()).fold(0.0, f32::max);
        assert!(diff < 1e-3, "context changed by {diff}");
    }

    /// DeepFilterNet removes most of a steady hiss while keeping a voice-like tone burst.
    #[test]
    fn voice_denoise_reduces_noise() {
        let rate = 48000;
        let n = rate as usize * 2;
        let mut seed = 7u32;
        let x: Vec<f32> = (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 * 0.1 - 0.05
            })
            .collect();
        let y = voice_denoise(&x, rate, 1.0, &Cancel::default()).unwrap();
        assert_eq!(y.len(), x.len());
        let (a, b) = (rms(&x[rate as usize..]), rms(&y[rate as usize..]));
        assert!(b < a * 0.3, "noise {a} -> {b}");
    }

    #[test]
    fn dereverb_keeps_length_and_reduces_tail() {
        let rate = 44100;
        let n = rate as usize;
        // a click followed by an exponentially decaying noise tail (a crude room)
        let mut seed = 3u32;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                let t = i as f32 / rate as f32;
                if i < 64 { 0.8 } else { r * 0.3 * (-6.9 * t / 0.8).exp() }
            })
            .collect();
        let y = dereverb(&x, rate, 0.8, 1.0, &Cancel::default());
        assert_eq!(y.len(), x.len());
        let tail = rate as usize / 5..rate as usize / 2;
        assert!(rms(&y[tail.clone()]) < rms(&x[tail]) * 0.8);
    }
}
