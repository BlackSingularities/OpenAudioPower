//! Length-changing tools: Paulstretch, sliding tempo / pitch, truncating silence inside a
//! recording, and the voice detector behind Auto Duck.

use crate::buffer::{db_to_gain, AudioBuffer};
use crate::mix::AutoPoint;
use crate::spectral::hann;
use crate::Cancel;
use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;
use std::f32::consts::PI;

// ───────────────────────────── Paulstretch ─────────────────────────────

/// Extreme time stretch (Paul Nasca's algorithm): long windows with randomised phases.
/// `factor` ≥ 1 multiplies the length, `window` is the window length in seconds.
pub fn paulstretch(x: &[f32], rate: u32, factor: f32, window: f32, cancel: &Cancel) -> Vec<f32> {
    let factor = factor.clamp(1.0, 50.0);
    if x.is_empty() {
        return Vec::new();
    }
    // even window, at least 256 samples
    let n = (((window.clamp(0.01, 10.0) * rate as f32) as usize).max(256) / 2) * 2;
    let half = n / 2;
    let win: Vec<f32> = (0..n).map(|i| (1.0 - (2.0 * i as f32 / (n - 1) as f32 - 1.0).powi(2)).max(0.0).powf(1.25)).collect();
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let mut inp = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let mut time = ifft.make_output_vec();
    let out_len = (x.len() as f32 * factor) as usize;
    let mut out = vec![0.0f32; out_len + n];
    let step_in = half as f32 / factor;
    let mut seed = 0x2545_F491u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let mut pos = 0.0f32;
    let mut o = 0usize;
    let mut k = 0usize;
    while o < out_len {
        if k % 32 == 0 && cancel.is_set() {
            break;
        }
        k += 1;
        let p0 = pos as isize - half as isize;
        for i in 0..n {
            let j = p0 + i as isize;
            inp[i] = if j >= 0 && (j as usize) < x.len() { x[j as usize] * win[i] } else { 0.0 };
        }
        let _ = fft.process(&mut inp, &mut spec);
        for c in spec.iter_mut() {
            *c = Complex32::from_polar(c.norm(), rnd() * 2.0 * PI);
        }
        let last = spec.len() - 1;
        spec[0].im = 0.0;
        spec[last].im = 0.0;
        let _ = ifft.process(&mut spec, &mut time);
        for i in 0..n {
            if o + i < out.len() {
                out[o + i] += time[i] * win[i] / n as f32;
            }
        }
        o += half;
        pos += step_in;
    }
    out.truncate(out_len);
    // the random phases change the level; match the input's RMS
    let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len().max(1) as f32).sqrt();
    let (ri, ro) = (rms(x), rms(&out));
    if ro > 1e-9 {
        let g = ri / ro;
        out.iter_mut().for_each(|s| *s *= g);
    }
    out
}

// ───────────────────────────── sliding stretch ─────────────────────────────

#[inline]
fn princarg(p: f32) -> f32 {
    p - 2.0 * PI * ((p + PI) / (2.0 * PI)).floor()
}

/// Phase vocoder whose length ratio changes along the signal: `ratio_at(progress 0..1)`.
fn stretch_varying(x: &[f32], ratio_at: &dyn Fn(f32) -> f32, cancel: &Cancel) -> Vec<f32> {
    let n = 4096;
    let hs = 1024usize;
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let win = hann(n);
    let bins = n / 2 + 1;
    let mut inp = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let mut time = ifft.make_output_vec();
    let mut prev = vec![0.0f32; bins];
    let mut sum = vec![0.0f32; bins];
    let mut out: Vec<f32> = Vec::new();
    let mut norm: Vec<f32> = Vec::new();
    let mut pos = 0.0f64;
    let mut prev_ha = 0.0f32;
    let mut f = 0usize;
    while (pos as usize) < x.len() {
        if f % 64 == 0 && cancel.is_set() {
            break;
        }
        let ratio = ratio_at((pos / x.len() as f64) as f32).clamp(0.1, 10.0);
        let ha = hs as f32 / ratio;
        let a0 = pos as isize - (n / 2) as isize;
        for i in 0..n {
            let j = a0 + i as isize;
            inp[i] = if j >= 0 && (j as usize) < x.len() { x[j as usize] * win[i] } else { 0.0 };
        }
        let _ = fft.process(&mut inp, &mut spec);
        for k in 0..bins {
            let (m, ph) = (spec[k].norm(), spec[k].arg());
            if f == 0 {
                sum[k] = ph;
            } else {
                // expected advance over the hop actually taken since the previous frame
                let omega = 2.0 * PI * k as f32 * prev_ha / n as f32;
                let inst = omega + princarg(ph - prev[k] - omega);
                sum[k] += inst * hs as f32 / prev_ha.max(1e-3);
            }
            prev[k] = ph;
            spec[k] = Complex32::from_polar(m, sum[k]);
        }
        spec[0].im = 0.0;
        spec[bins - 1].im = 0.0;
        let _ = ifft.process(&mut spec, &mut time);
        let s0 = f * hs;
        if out.len() < s0 + n {
            out.resize(s0 + n, 0.0);
            norm.resize(s0 + n, 0.0);
        }
        for i in 0..n {
            out[s0 + i] += time[i] * win[i] / n as f32;
            norm[s0 + i] += win[i] * win[i];
        }
        pos += ha as f64;
        prev_ha = ha;
        f += 1;
    }
    let off = n / 2;
    let len = (f * hs).saturating_sub(0);
    (0..len)
        .map(|i| {
            let j = i + off;
            if j < out.len() && norm[j] > 1e-3 { out[j] / norm[j] } else { 0.0 }
        })
        .collect()
}

/// Cubic resampler with a step that changes along the signal (`step_at(progress)` input
/// samples per output sample).
fn resample_varying(x: &[f32], step_at: &dyn Fn(f32) -> f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len());
    let mut p = 0.0f64;
    let get = |i: isize| -> f32 { if i < 0 || i as usize >= x.len() { 0.0 } else { x[i as usize] } };
    while (p as usize) < x.len() {
        let i = p.floor() as isize;
        let t = (p - i as f64) as f32;
        let (y0, y1, y2, y3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        out.push(y1 + 0.5 * t * (y2 - y0 + t * (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3 + t * (3.0 * (y1 - y2) + y3 - y0))));
        p += step_at((p / x.len() as f64) as f32).clamp(0.05, 20.0) as f64;
    }
    out
}

/// Tempo (percent change, + = faster) and pitch (semitones) sliding linearly from the start
/// values to the end values over the range.
pub fn sliding_stretch(x: &[f32], tempo0: f32, tempo1: f32, pitch0: f32, pitch1: f32, cancel: &Cancel) -> Vec<f32> {
    if x.is_empty() {
        return Vec::new();
    }
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    let len_ratio = |t: f32| 100.0 / (100.0 + lerp(tempo0, tempo1, t)).max(5.0);
    let pf = |t: f32| 2f32.powf(lerp(pitch0, pitch1, t) / 12.0);
    // stretch by tempo × pitch, then read faster by the pitch factor: pitch rises, tempo stays
    let pitch_only = pitch0.abs() < 1e-3 && pitch1.abs() < 1e-3;
    let tempo_only = tempo0.abs() < 1e-3 && tempo1.abs() < 1e-3;
    if pitch_only && tempo_only {
        return x.to_vec();
    }
    let stretched = stretch_varying(x, &|t| len_ratio(t) * pf(t), cancel);
    if pitch_only {
        return stretched;
    }
    resample_varying(&stretched, &pf)
}

// ───────────────────────────── truncate silence ─────────────────────────────

/// Shortens every pause quieter than `threshold_db` and longer than `min_ms` down to `keep_ms`
/// (channels linked), joining the ends with a short equal-power crossfade. Returns the new
/// buffer and the number of pauses shortened.
pub fn truncate_silence(b: &AudioBuffer, threshold_db: f32, min_ms: f32, keep_ms: f32) -> (AudioBuffer, usize) {
    let n = b.frames();
    let rate = b.rate as f32;
    let blk = ((rate * 0.005) as usize).max(1);
    let thr = db_to_gain(threshold_db);
    let silent: Vec<bool> = (0..n.div_ceil(blk))
        .map(|k| {
            let (a, z) = (k * blk, ((k + 1) * blk).min(n));
            b.ch.iter().all(|c| c[a..z].iter().all(|v| v.abs() < thr))
        })
        .collect();
    let min = (min_ms.max(10.0) * 0.001 * rate) as usize;
    let keep = ((keep_ms.max(0.0) * 0.001 * rate) as usize).min(min);
    let xf = ((rate * 0.005) as usize).max(1);
    // cuts: [from, to) removed
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut k = 0;
    while k < silent.len() {
        if !silent[k] {
            k += 1;
            continue;
        }
        let s = k;
        while k < silent.len() && silent[k] {
            k += 1;
        }
        let (a, z) = (s * blk, (k * blk).min(n));
        // pauses at the very edges are left alone (that is what trim does)
        if z - a >= min && a > 0 && z < n {
            let from = a + keep / 2;
            let to = z - (keep - keep / 2);
            if to > from + 2 * xf {
                cuts.push((from, to));
            }
        }
    }
    if cuts.is_empty() {
        return (b.clone(), 0);
    }
    let ch = b
        .ch
        .iter()
        .map(|c| {
            let mut out: Vec<f32> = Vec::with_capacity(n);
            let mut at = 0usize;
            for &(from, to) in &cuts {
                out.extend_from_slice(&c[at..from]);
                // overlap the xf samples after `to` onto the last xf samples written
                let base = out.len() - xf.min(out.len());
                for i in 0..xf.min(out.len()) {
                    let t = (i as f32 + 0.5) / xf as f32;
                    let (g_out, g_in) = ((t * PI / 2.0).cos(), (t * PI / 2.0).sin());
                    out[base + i] = out[base + i] * g_out + c.get(to + i).copied().unwrap_or(0.0) * g_in;
                }
                at = to + xf.min(out.len());
            }
            out.extend_from_slice(&c[at.min(n)..]);
            out
        })
        .collect();
    (AudioBuffer::from_channels(b.rate, ch), cuts.len())
}

// ───────────────────────────── auto duck ─────────────────────────────

/// Volume automation (dB offsets) that lowers a track by `duck_db` wherever `control` is
/// louder than `threshold_db`. Pauses shorter than `hold_ms` stay ducked; the level glides
/// down over `fall_ms` before speech and back up over `rise_ms` after it. `offset` is the
/// timeline frame of the control buffer's first frame.
pub fn duck_points(control: &AudioBuffer, offset: i64, threshold_db: f32, duck_db: f32, fall_ms: f32, rise_ms: f32, hold_ms: f32) -> Vec<AutoPoint> {
    let rate = control.rate as f32;
    let n = control.frames();
    let win = ((rate * 0.02) as usize).max(1);
    let thr = db_to_gain(threshold_db);
    let k = 1.0 / control.channels().max(1) as f32;
    // 20 ms RMS windows
    let loud: Vec<bool> = (0..n.div_ceil(win))
        .map(|w| {
            let (a, z) = (w * win, ((w + 1) * win).min(n));
            let e: f32 = (a..z).map(|i| control.ch.iter().map(|c| c[i] * c[i]).sum::<f32>() * k).sum::<f32>() / (z - a).max(1) as f32;
            e.sqrt() > thr
        })
        .collect();
    let hold = ((hold_ms.max(0.0) * 0.001 * rate) as usize).div_ceil(win);
    let mut regions: Vec<(usize, usize)> = Vec::new();
    let mut w = 0;
    while w < loud.len() {
        if !loud[w] {
            w += 1;
            continue;
        }
        let s = w;
        while w < loud.len() && loud[w] {
            w += 1;
        }
        let (a, z) = (s * win, (w * win).min(n));
        match regions.last_mut() {
            Some(last) if a.saturating_sub(last.1) <= hold * win => last.1 = z,
            _ => regions.push((a, z)),
        }
    }
    let fall = (fall_ms.max(0.0) * 0.001 * rate) as i64;
    let rise = (rise_ms.max(0.0) * 0.001 * rate) as i64;
    let mut pts: Vec<AutoPoint> = Vec::new();
    for (a, z) in regions {
        let (a, z) = (offset + a as i64, offset + z as i64);
        let down0 = (a - fall).max(pts.last().map(|p| p.t + 1).unwrap_or(i64::MIN));
        pts.push(AutoPoint { t: down0, v: 0.0 });
        pts.push(AutoPoint { t: a.max(down0 + 1), v: duck_db });
        pts.push(AutoPoint { t: z.max(a.max(down0 + 1) + 1), v: duck_db });
        pts.push(AutoPoint { t: z.max(a.max(down0 + 1) + 1) + rise.max(1), v: 0.0 });
    }
    // overlapping glides: keep times strictly increasing
    let mut out: Vec<AutoPoint> = Vec::with_capacity(pts.len());
    for p in pts {
        match out.last() {
            Some(l) if p.t <= l.t => {
                if p.v < l.v {
                    out.last_mut().unwrap().v = p.v;
                }
            }
            _ => out.push(p),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(rate: u32, f: f32, a: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).map(|i| a * (TAU * f * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn paulstretch_length_and_level() {
        let rate = 22050;
        let x = sine(rate, 440.0, 0.5, 1.0);
        let y = paulstretch(&x, rate, 8.0, 0.25, &Cancel::default());
        assert_eq!(y.len(), x.len() * 8);
        assert!(y.iter().all(|v| v.is_finite()));
        let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt();
        assert!((rms(&y) / rms(&x) - 1.0).abs() < 0.05);
    }

    #[test]
    fn sliding_tempo_changes_length() {
        let rate = 22050;
        let x = sine(rate, 300.0, 0.5, 2.0);
        // +100 % tempo all the way: half as long
        let y = sliding_stretch(&x, 100.0, 100.0, 0.0, 0.0, &Cancel::default());
        let r = y.len() as f32 / x.len() as f32;
        assert!((r - 0.5).abs() < 0.06, "ratio {r}");
        // slowing down from 0 to −50 %: between 1× and 2×
        let y = sliding_stretch(&x, 0.0, -50.0, 0.0, 0.0, &Cancel::default());
        let r = y.len() as f32 / x.len() as f32;
        assert!(r > 1.25 && r < 1.6, "ratio {r}");
        // pitch only keeps the length
        let y = sliding_stretch(&x, 0.0, 0.0, 0.0, 12.0, &Cancel::default());
        let r = y.len() as f32 / x.len() as f32;
        assert!((r - 1.0).abs() < 0.08, "ratio {r}");
        assert!(y.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn truncate_silence_shortens_pauses_only() {
        let rate = 8000;
        let mut x = sine(rate, 200.0, 0.5, 1.0);
        x.extend(vec![0.0; rate as usize * 2]); // 2 s pause
        x.extend(sine(rate, 200.0, 0.5, 1.0));
        let b = AudioBuffer::from_channels(rate, vec![x.clone()]);
        let (y, n) = truncate_silence(&b, -50.0, 500.0, 300.0);
        assert_eq!(n, 1);
        let secs = y.frames() as f32 / rate as f32;
        assert!((secs - 2.3).abs() < 0.05, "{secs}");
        // sound before and after kept
        assert!(y.ch[0][..rate as usize].iter().any(|v| v.abs() > 0.4));
        assert!(y.ch[0][y.frames() - 4000..].iter().any(|v| v.abs() > 0.4));
        // short pauses survive
        let (_, n) = truncate_silence(&b, -50.0, 3000.0, 300.0);
        assert_eq!(n, 0);
    }

    #[test]
    fn duck_follows_speech() {
        let rate = 1000;
        let mut v = vec![0.0f32; 5000];
        v[1000..2000].iter_mut().enumerate().for_each(|(i, s)| *s = if i % 2 == 0 { 0.5 } else { -0.5 });
        v[2100..3000].iter_mut().enumerate().for_each(|(i, s)| *s = if i % 2 == 0 { 0.5 } else { -0.5 });
        let b = AudioBuffer::from_channels(rate, vec![v]);
        let p = duck_points(&b, 100, -30.0, -12.0, 200.0, 300.0, 300.0);
        // short gap at 2000–2100 is held: one region → 4 points
        assert_eq!(p.len(), 4, "{p:?}");
        assert_eq!(p[0].v, 0.0);
        assert_eq!(p[1].v, -12.0);
        assert_eq!(p[1].t, 1100);
        assert_eq!(p[2].t, 3100);
        assert_eq!(p[3].v, 0.0);
        assert!(p.windows(2).all(|w| w[0].t < w[1].t));
    }
}
