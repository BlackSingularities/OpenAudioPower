//! Linear-phase FIR filters designed from a magnitude curve, a block FFT convolver for
//! real-time use and a whole-buffer convolution for offline processing.

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f32::consts::PI;
use std::sync::Arc;

/// Symmetric FIR of `len` taps whose magnitude follows `db_at(frequency)`; group delay `len / 2`.
pub fn design(len: usize, rate: f32, db_at: &dyn Fn(f32) -> f32) -> Vec<f32> {
    let len = len.max(16) & !1;
    let mut planner = RealFftPlanner::<f32>::new();
    let ifft = planner.plan_fft_inverse(len);
    let mut spec = ifft.make_input_vec();
    for (k, s) in spec.iter_mut().enumerate() {
        let f = k as f32 * rate / len as f32;
        let m = 10f32.powf(db_at(f.max(1.0)).clamp(-60.0, 30.0) / 20.0);
        *s = Complex32::new(m, 0.0);
    }
    let mut imp = ifft.make_output_vec();
    let _ = ifft.process(&mut spec, &mut imp);
    // zero-phase impulse is centred at 0: rotate to the middle and window it
    let mut h = vec![0.0f32; len];
    for (i, v) in h.iter_mut().enumerate() {
        let src = (i + len / 2) % len;
        let w = 0.42 - 0.5 * (2.0 * PI * i as f32 / len as f32).cos() + 0.08 * (4.0 * PI * i as f32 / len as f32).cos();
        *v = imp[src] / len as f32 * w;
    }
    h
}

/// Uniform-block FFT convolution (overlap-add). Latency = `block` samples plus the filter's
/// own group delay. The filter may be up to `block` taps long.
pub struct Convolver {
    block: usize,
    h: Vec<Complex32>,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    inp: Vec<f32>,
    out: Vec<f32>,
    tail: Vec<f32>,
    pos: usize,
    tbuf: Vec<f32>,
    sbuf: Vec<Complex32>,
}

impl Convolver {
    pub fn new(h: &[f32]) -> Self {
        let block = h.len().next_power_of_two().max(64);
        let n = block * 2;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let mut t = fft.make_input_vec();
        t[..h.len()].copy_from_slice(h);
        let mut hs = fft.make_output_vec();
        let _ = fft.process(&mut t, &mut hs);
        let tbuf = fft.make_input_vec();
        let sbuf = fft.make_output_vec();
        Self { block, h: hs, fft, ifft, inp: vec![0.0; block], out: vec![0.0; block], tail: vec![0.0; block], pos: 0, tbuf, sbuf }
    }

    pub fn block(&self) -> usize {
        self.block
    }

    /// Replaces the filter, keeping the audio already in flight.
    pub fn set(&mut self, h: &[f32]) {
        let mut t = vec![0.0f32; self.block * 2];
        t[..h.len().min(self.block)].copy_from_slice(&h[..h.len().min(self.block)]);
        let _ = self.fft.process(&mut t, &mut self.h);
    }

    pub fn process(&mut self, x: &mut [f32]) {
        for s in x.iter_mut() {
            let y = self.out[self.pos];
            self.inp[self.pos] = *s;
            *s = y;
            self.pos += 1;
            if self.pos == self.block {
                self.pos = 0;
                self.tbuf[..self.block].copy_from_slice(&self.inp);
                self.tbuf[self.block..].fill(0.0);
                let _ = self.fft.process(&mut self.tbuf, &mut self.sbuf);
                for (a, b) in self.sbuf.iter_mut().zip(&self.h) {
                    *a *= *b;
                }
                let _ = self.ifft.process(&mut self.sbuf, &mut self.tbuf);
                let k = 1.0 / (self.block * 2) as f32;
                for i in 0..self.block {
                    self.out[i] = self.tbuf[i] * k + self.tail[i];
                    self.tail[i] = self.tbuf[self.block + i] * k;
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.inp.fill(0.0);
        self.out.fill(0.0);
        self.tail.fill(0.0);
        self.pos = 0;
    }
}

/// Offline convolution of a whole signal with a linear-phase filter, compensating its delay:
/// the result lines up with the input and has the same length.
pub fn convolve_centered(x: &[f32], h: &[f32]) -> Vec<f32> {
    if x.is_empty() || h.is_empty() {
        return x.to_vec();
    }
    let delay = h.len() / 2;
    let mut c = Convolver::new(h);
    let mut buf = x.to_vec();
    buf.resize(x.len() + delay + c.block(), 0.0);
    c.process(&mut buf);
    // output trails the input by the block and the filter's centre; the zero padding pushed
    // everything out, so the aligned result is simply the window after that offset
    let skip = delay + c.block();
    buf[skip..skip + x.len()].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(rate: f32, f: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (TAU * f * i as f32 / rate).sin()).collect()
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn flat_filter_is_transparent_and_aligned() {
        let rate = 48000.0;
        let h = design(2048, rate, &|_| 0.0);
        let x = sine(rate, 997.0, 20000);
        let y = convolve_centered(&x, &h);
        assert_eq!(y.len(), x.len());
        let err: Vec<f32> = x.iter().zip(&y).skip(3000).take(14000).map(|(a, b)| a - b).collect();
        assert!(rms(&err) < 0.01, "{}", rms(&err));
    }

    #[test]
    fn shaped_filter_follows_curve() {
        let rate = 48000.0;
        // −12 dB above 4 kHz
        let h = design(4096, rate, &|f| if f > 4000.0 { -12.0 } else { 0.0 });
        let lo = convolve_centered(&sine(rate, 500.0, 30000), &h);
        let hi = convolve_centered(&sine(rate, 10000.0, 30000), &h);
        let g = |y: &[f32]| 20.0 * (rms(&y[5000..25000]) / std::f32::consts::FRAC_1_SQRT_2).log10();
        assert!(g(&lo).abs() < 0.3, "lo {}", g(&lo));
        assert!((g(&hi) + 12.0).abs() < 0.5, "hi {}", g(&hi));
    }
}
