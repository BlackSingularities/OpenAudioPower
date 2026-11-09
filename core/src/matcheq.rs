//! Match EQ: the average spectrum of a reference recording against the target's gives a
//! correction curve, applied offline as a linear-phase filter.

use crate::buffer::AudioBuffer;
use crate::spectral::hann;
use realfft::RealFftPlanner;

const N: usize = 8192;

/// Average power spectrum (linear power per bin) of the channel mix.
fn avg_power(b: &AudioBuffer) -> Vec<f64> {
    let bins = N / 2 + 1;
    let mut acc = vec![0.0f64; bins];
    let mono = b.mono();
    if mono.len() < N {
        return acc;
    }
    let win = hann(N);
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N);
    let mut inp = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let mut frames = 0usize;
    let mut a = 0;
    // at most ~2000 frames spread over the whole recording
    let hop = (N / 2).max((mono.len() - N) / 2000);
    while a + N <= mono.len() {
        for i in 0..N {
            inp[i] = mono[a + i] * win[i];
        }
        let _ = fft.process(&mut inp, &mut spec);
        for (k, c) in spec.iter().enumerate() {
            acc[k] += c.norm_sqr() as f64;
        }
        frames += 1;
        a += hop;
    }
    acc.iter_mut().for_each(|v| *v /= frames.max(1) as f64);
    acc
}

/// Correction curve (frequency Hz, gain dB) that makes `target` sound like `reference`.
/// `smoothing` is the averaging width in octaves (e.g. 1/3); the curve is level-neutral
/// in the 200 Hz – 5 kHz range and limited to ±12 dB.
pub fn match_curve(target: &AudioBuffer, reference: &AudioBuffer, smoothing: f32) -> Vec<[f32; 2]> {
    let rate = target.rate as f32;
    let pt = avg_power(target);
    let pr = avg_power(reference);
    let bin_hz = rate / N as f32;
    let band = |p: &[f64], f: f32| -> f64 {
        let w = 2f32.powf(smoothing.clamp(0.05, 2.0) / 2.0);
        let (a, z) = (((f / w) / bin_hz).floor() as usize, (((f * w) / bin_hz).ceil() as usize).min(p.len() - 1));
        let (a, z) = (a.max(1), z.max(a.max(1)));
        p[a..=z].iter().sum::<f64>() / (z - a + 1) as f64
    };
    let mut pts: Vec<[f32; 2]> = Vec::new();
    let mut f = 20.0f32;
    let top = (rate * 0.45).min(20000.0);
    while f <= top {
        let (t, r) = (band(&pt, f), band(&pr, f));
        let d = if t > 1e-14 && r > 1e-14 { 10.0 * (r / t).log10() as f32 } else { 0.0 };
        pts.push([f, d]);
        f *= 2f32.powf(1.0 / 12.0);
    }
    // level-neutral where the ear is most sensitive
    let mid: Vec<f32> = pts.iter().filter(|p| p[0] >= 200.0 && p[0] <= 5000.0).map(|p| p[1]).collect();
    let off = if mid.is_empty() { 0.0 } else { mid.iter().sum::<f32>() / mid.len() as f32 };
    pts.iter_mut().for_each(|p| p[1] = (p[1] - off).clamp(-12.0, 12.0));
    pts
}

/// Curve value at `f`, linear in log-frequency, held beyond the ends.
pub fn curve_at(curve: &[[f32; 2]], f: f32) -> f32 {
    let Some(first) = curve.first() else { return 0.0 };
    if f <= first[0] {
        return first[1];
    }
    let last = curve[curve.len() - 1];
    if f >= last[0] {
        return last[1];
    }
    let i = curve.partition_point(|p| p[0] <= f);
    let (a, b) = (curve[i - 1], curve[i]);
    let t = (f.ln() - a[0].ln()) / (b[0].ln() - a[0].ln());
    a[1] + (b[1] - a[1]) * t
}

/// Applies `amount` (0–1) of the curve to every channel with a linear-phase filter.
pub fn apply(buf: &AudioBuffer, curve: &[[f32; 2]], amount: f32) -> AudioBuffer {
    let k = amount.clamp(0.0, 1.5);
    let h = crate::dsp::fir::design(N, buf.rate as f32, &|f| k * curve_at(curve, f));
    let ch = buf.ch.iter().map(|c| crate::dsp::fir::convolve_centered(c, &h)).collect();
    AudioBuffer::from_channels(buf.rate, ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                s as f32 / u32::MAX as f32 - 0.5
            })
            .collect()
    }

    #[test]
    fn dull_target_gets_brighter() {
        let rate = 48000;
        let n = rate as usize * 4;
        let reference = AudioBuffer::from_channels(rate, vec![noise(n, 7)]);
        // target: the same kind of noise through a gentle low-pass
        let mut t = noise(n, 99);
        let mut y = 0.0f32;
        let a = (-2.0 * std::f32::consts::PI * 2000.0 / rate as f32).exp();
        for v in t.iter_mut() {
            y = *v + a * (y - *v);
            *v = y;
        }
        let target = AudioBuffer::from_channels(rate, vec![t]);
        let c = match_curve(&target, &reference, 1.0 / 3.0);
        let at = |f| curve_at(&c, f);
        assert!(at(8000.0) > at(500.0) + 6.0, "curve 500 Hz {} / 8 kHz {}", at(500.0), at(8000.0));
        let out = apply(&target, &c, 1.0);
        assert_eq!(out.frames(), target.frames());
        // after matching, the remaining difference is much flatter
        let c2 = match_curve(&out, &reference, 1.0 / 3.0);
        let span = |c: &[[f32; 2]]| {
            let v: Vec<f32> = c.iter().filter(|p| p[0] > 100.0 && p[0] < 12000.0).map(|p| p[1]).collect();
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(span(&c2) < span(&c) * 0.5, "before {} after {}", span(&c), span(&c2));
    }
}
