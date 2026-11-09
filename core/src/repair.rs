//! Offline repair tools: clipping restoration, hum removal, plosive suppression and a
//! slow loudness leveler (gain riding). All work on whole channel buffers.

use crate::buffer::{db_to_gain, gain_to_db, AudioBuffer};
use crate::dsp::biquad::{Biquad, Coefs, FilterKind};
use crate::dsp::ms_to_coef;

/// Runs a biquad forward and then backward over `x` (zero phase, squared magnitude).
fn filtfilt(x: &mut [f32], c: Coefs) {
    let mut f = Biquad::new(c);
    x.iter_mut().for_each(|v| *v = f.tick(*v));
    let mut b = Biquad::new(c);
    x.iter_mut().rev().for_each(|v| *v = b.tick(*v));
}

// ───────────────────────────── declip ─────────────────────────────

/// Rebuilds runs of clipped samples. A sample counts as clipped when it lies within
/// `tolerance` (fraction, 0–0.2) of the channel's peak; each run is replaced by a cubic
/// that matches the level and slope of the audio on both sides. `gain_db` is applied
/// afterwards, since restored peaks rise above the old ceiling.
pub fn declip(b: &mut AudioBuffer, tolerance: f32, gain_db: f32) -> usize {
    let tol = tolerance.clamp(0.0, 0.2);
    let mut runs = 0;
    for c in b.ch.iter_mut() {
        let n = c.len();
        let peak = c.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        if peak < 1e-6 || n < 8 {
            continue;
        }
        let lvl = peak * (1.0 - tol) - 1e-7;
        let mut i = 2;
        while i < n - 2 {
            if c[i].abs() < lvl {
                i += 1;
                continue;
            }
            let start = i;
            let sign = c[i].signum();
            while i < n - 2 && c[i].abs() >= lvl && c[i].signum() == sign {
                i += 1;
            }
            let end = i; // first good sample after the run
            let len = end - start;
            // a single sample at the peak is just a peak, not clipping
            if len < 2 || start < 2 {
                continue;
            }
            runs += 1;
            let a = start - 1;
            let (y0, y1) = (c[a], c[end]);
            let m0 = c[a] - c[a - 1];
            let m1 = c[(end + 1).min(n - 1)] - c[end];
            let span = (end - a) as f32;
            // Hermite with slopes scaled to the span
            for j in start..end {
                let t = (j - a) as f32 / span;
                let t2 = t * t;
                let t3 = t2 * t;
                let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                let h10 = t3 - 2.0 * t2 + t;
                let h01 = -2.0 * t3 + 3.0 * t2;
                let h11 = t3 - t2;
                let v = h00 * y0 + h10 * m0 * span + h01 * y1 + h11 * m1 * span;
                // never let the rebuilt curve fall back below the clipped plateau
                c[j] = if v.abs() < c[j].abs() || v.signum() != sign { c[j] } else { v };
            }
        }
    }
    let g = db_to_gain(gain_db);
    if (g - 1.0).abs() > 1e-6 {
        b.ch.iter_mut().flatten().for_each(|x| *x *= g);
    }
    runs
}

// ───────────────────────────── dehum ─────────────────────────────

/// Removes mains hum: zero-phase notches at `base` (50 or 60 Hz) and its harmonics.
/// `q` sets the notch width (higher = narrower).
pub fn dehum(b: &mut AudioBuffer, base: f32, harmonics: usize, q: f32) {
    let rate = b.rate as f32;
    let base = base.clamp(40.0, 70.0);
    let q = q.clamp(2.0, 100.0);
    for c in b.ch.iter_mut() {
        for k in 1..=harmonics.clamp(1, 16) {
            let f = base * k as f32;
            if f >= rate * 0.45 {
                break;
            }
            // filtfilt doubles the depth, so the width per pass can stay as given
            filtfilt(c, Coefs::new(FilterKind::Notch, rate, f, q, 0.0));
        }
    }
}

// ───────────────────────────── deplosive ─────────────────────────────

/// Tames plosives (p/b pops, wind bumps): the band below `freq` is ducked whenever its
/// level jumps above the rest of the signal by more than the sensitivity allows.
/// Returns the deepest reduction applied, in dB.
pub fn deplosive(b: &mut AudioBuffer, freq: f32, sensitivity: f32, max_reduction: f32) -> f32 {
    let rate = b.rate as f32;
    let f = freq.clamp(60.0, 400.0);
    // sensitivity 0 → only ducks when lows are 12 dB over the highs, 1 → 0 dB
    let margin = 12.0 * (1.0 - sensitivity.clamp(0.0, 1.0));
    let max_red = max_reduction.clamp(0.0, 40.0);
    let att = ms_to_coef(1.0, rate);
    let rel = ms_to_coef(80.0, rate);
    let smooth = ms_to_coef(5.0, rate);
    let mut deepest = 0.0f32;
    let lp = Coefs::new(FilterKind::LowPass, rate, f, std::f32::consts::FRAC_1_SQRT_2, 0.0);
    for c in b.ch.iter_mut() {
        let mut low = c.clone();
        filtfilt(&mut low, lp);
        let (mut el, mut eh, mut g) = (0.0f32, 0.0f32, 1.0f32);
        for (x, l) in c.iter_mut().zip(low.iter()) {
            let h = *x - *l;
            let al = l.abs();
            let ah = h.abs();
            el = if al > el { al + att * (el - al) } else { al + rel * (el - al) };
            eh = if ah > eh { ah + att * (eh - ah) } else { ah + rel * (eh - ah) };
            let over = gain_to_db(el + 1e-9) - gain_to_db(eh + 1e-9) - margin;
            let red = if el > 1e-3 && over > 0.0 { over.min(max_red) } else { 0.0 };
            deepest = deepest.max(red);
            let target = db_to_gain(-red);
            g = target + smooth * (g - target);
            *x = h + *l * g;
        }
    }
    deepest
}

// ───────────────────────────── leveler ─────────────────────────────

/// Slow automatic gain riding toward `target_db` RMS, like a sound engineer on the fader.
/// Gain stays within ±`max_gain_db`, holds during passages quieter than `gate_db`, and
/// moves with time constant `speed_ms`. Channels are linked.
pub fn level(b: &mut AudioBuffer, target_db: f32, max_gain_db: f32, gate_db: f32, speed_ms: f32) {
    let n = b.frames();
    if n == 0 {
        return;
    }
    let rate = b.rate as f32;
    let win = ((rate * 0.4) as usize).max(1);
    let k = 1.0 / b.channels().max(1) as f32;
    // power of the channel mix, as a running window sum
    let pw: Vec<f32> = (0..n).map(|i| b.ch.iter().map(|c| c[i] * c[i]).sum::<f32>() * k).collect();
    let mut prefix = vec![0.0f64; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + pw[i] as f64;
    }
    let maxg = max_gain_db.clamp(0.0, 30.0);
    let coef = ms_to_coef(speed_ms.clamp(50.0, 10000.0), rate);
    // start from the gain the first loud window asks for, so the head isn't ramped
    let want = |i: usize| -> Option<f32> {
        let a = i.saturating_sub(win / 2);
        let z = (i + win / 2).min(n);
        let rms = ((prefix[z] - prefix[a]) / (z - a).max(1) as f64).sqrt() as f32;
        let db = gain_to_db(rms + 1e-12);
        (db > gate_db).then(|| (target_db - db).clamp(-maxg, maxg))
    };
    let mut g_db = (0..n).step_by(win).find_map(want).unwrap_or(0.0);
    let mut cached = g_db;
    for i in 0..n {
        if i % 32 == 0 {
            if let Some(w) = want(i) {
                cached = w;
            }
        }
        g_db = cached + coef * (g_db - cached);
        let g = db_to_gain(g_db);
        for c in b.ch.iter_mut() {
            c[i] *= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(rate: u32, f: f32, amp: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).map(|i| amp * (TAU * f * i as f32 / rate as f32).sin()).collect()
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn declip_restores_peaks() {
        let rate = 44100;
        let clean = sine(rate, 220.0, 1.0, 0.5);
        let clipped: Vec<f32> = clean.iter().map(|x| x.clamp(-0.6, 0.6)).collect();
        let mut b = AudioBuffer::from_channels(rate, vec![clipped.clone()]);
        let runs = declip(&mut b, 0.01, 0.0);
        assert!(runs > 100, "runs {runs}");
        let err = |y: &[f32]| rms(&y.iter().zip(&clean).map(|(a, b)| a - b).collect::<Vec<_>>());
        let before = err(&clipped);
        let after = err(&b.ch[0]);
        assert!(after < before * 0.5, "before {before} after {after}");
        assert!(b.peak() > 0.8);
    }

    #[test]
    fn dehum_removes_50hz_keeps_voice() {
        let rate = 44100;
        let hum: Vec<f32> = sine(rate, 50.0, 0.3, 2.0).iter().zip(sine(rate, 150.0, 0.1, 2.0)).map(|(a, b)| a + b).collect();
        let tone = sine(rate, 1000.0, 0.3, 2.0);
        let mut b = AudioBuffer::from_channels(rate, vec![hum.iter().zip(&tone).map(|(a, b)| a + b).collect()]);
        dehum(&mut b, 50.0, 6, 30.0);
        let mid = rate as usize / 2..rate as usize * 3 / 2;
        let resid: Vec<f32> = b.ch[0][mid.clone()].iter().zip(&tone[mid]).map(|(a, b)| a - b).collect();
        assert!(rms(&resid) < 0.03, "residual {}", rms(&resid));
    }

    #[test]
    fn deplosive_ducks_low_burst_only() {
        let rate = 44100;
        let mut x = sine(rate, 2000.0, 0.1, 1.0);
        // 60 ms 70 Hz pop in the middle
        let pop = sine(rate, 70.0, 0.8, 0.06);
        let at = rate as usize / 2;
        for (i, p) in pop.iter().enumerate() {
            x[at + i] += p;
        }
        let mut b = AudioBuffer::from_channels(rate, vec![x.clone()]);
        let deepest = deplosive(&mut b, 150.0, 0.7, 18.0);
        assert!(deepest > 6.0, "deepest {deepest}");
        let seg = at..at + pop.len();
        assert!(rms(&b.ch[0][seg.clone()]) < rms(&x[seg]) * 0.6);
        // untouched away from the pop
        let quiet = 1000..10000;
        let diff: Vec<f32> = b.ch[0][quiet.clone()].iter().zip(&x[quiet]).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) < 0.005);
    }

    #[test]
    fn leveler_evens_out_loudness() {
        let rate = 44100;
        let mut x = sine(rate, 440.0, 0.03, 3.0);
        x.extend(sine(rate, 440.0, 0.5, 3.0));
        let mut b = AudioBuffer::from_channels(rate, vec![x]);
        level(&mut b, -20.0, 18.0, -60.0, 300.0);
        let s = rate as usize;
        let quiet = gain_to_db(rms(&b.ch[0][s..2 * s]));
        let loud = gain_to_db(rms(&b.ch[0][4 * s..5 * s]));
        assert!((quiet - loud).abs() < 3.0, "quiet {quiet} loud {loud}");
        assert!((loud + 20.0).abs() < 3.0, "loud {loud}");
    }
}
