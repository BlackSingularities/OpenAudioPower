//! Dynamics: compressor (soft knee, stereo-linked), look-ahead brickwall limiter,
//! noise gate with hold and de-esser.

use super::biquad::{Biquad, Coefs, FilterKind};
use super::{ms_to_coef, Processor};
use crate::buffer::{db_to_gain, gain_to_db};

pub struct Compressor {
    rate: f32,
    pub threshold: f32,
    pub ratio: f32,
    pub knee: f32,
    pub makeup: f32,
    att: f32,
    rel: f32,
    env_db: f32,
    gr: f32,
    pub mix: f32,
}

impl Compressor {
    pub fn new(rate: f32) -> Self {
        Self { rate, threshold: -18.0, ratio: 4.0, knee: 6.0, makeup: 0.0, att: 0.0, rel: 0.0, env_db: -120.0, gr: 0.0, mix: 1.0 }
    }
    pub fn set(&mut self, threshold: f32, ratio: f32, attack_ms: f32, release_ms: f32, knee: f32, makeup: f32, mix: f32) {
        self.threshold = threshold;
        self.ratio = ratio.max(1.0);
        self.att = ms_to_coef(attack_ms.max(0.05), self.rate);
        self.rel = ms_to_coef(release_ms.max(1.0), self.rate);
        self.knee = knee.max(0.0);
        self.makeup = makeup;
        self.mix = mix.clamp(0.0, 1.0);
    }
    #[inline]
    fn curve(&self, x: f32) -> f32 {
        // static gain computer: returns output level (dB) for input level x (dB)
        let t = self.threshold;
        let k = self.knee;
        let r = self.ratio;
        if 2.0 * (x - t) < -k {
            x
        } else if k > 0.0 && 2.0 * (x - t).abs() <= k {
            x + (1.0 / r - 1.0) * (x - t + k / 2.0).powi(2) / (2.0 * k)
        } else {
            t + (x - t) / r
        }
    }
}

impl Compressor {
    /// Compresses `buf` while detecting the level on `key` (sidechain) instead of `buf`.
    pub fn process_keyed(&mut self, buf: &mut [Vec<f32>], key: Option<&[Vec<f32>]>, n: usize) {
        let mk = db_to_gain(self.makeup);
        let mut max_gr = 0.0f32;
        for i in 0..n {
            let mut peak = 0.0f32;
            match key {
                Some(k) => k.iter().for_each(|ch| peak = peak.max(ch.get(i).copied().unwrap_or(0.0).abs())),
                None => buf.iter().for_each(|ch| peak = peak.max(ch[i].abs())),
            }
            let x = gain_to_db(peak);
            let want = x - self.curve(x); // gain reduction (positive dB)
            let c = if want > self.env_db { self.att } else { self.rel };
            self.env_db = want + c * (self.env_db - want);
            let g = db_to_gain(-self.env_db.max(0.0)) * mk;
            max_gr = max_gr.max(self.env_db);
            for ch in buf.iter_mut() {
                let d = ch[i];
                ch[i] = d + (d * g - d) * self.mix;
            }
        }
        self.gr = max_gr.max(0.0);
    }
}

impl Processor for Compressor {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        self.process_keyed(buf, None, n);
    }
    fn reset(&mut self) {
        self.env_db = 0.0;
    }
    fn reduction_db(&self) -> f32 {
        self.gr
    }
}

/// Look-ahead brickwall limiter.
pub struct Limiter {
    rate: f32,
    ceiling: f32,
    gain: f32,
    rel: f32,
    att: f32,
    look: usize,
    delay: Vec<Vec<f32>>,
    peaks: Vec<f32>,
    pos: usize,
    gr: f32,
    input_gain: f32,
    /// detect inter-sample peaks on a 4× oversampled copy (ceiling in dBTP)
    true_peak: bool,
    /// recent input per channel for the oversampling interpolator
    hist: Vec<[f32; TP_TAPS]>,
    hpos: usize,
    dpos: usize,
}

/// Taps per phase of the 4× true-peak interpolator.
const TP_TAPS: usize = 12;
/// Delay of the interpolator's centre in input samples.
const TP_DELAY: usize = TP_TAPS / 2;

/// Polyphase windowed-sinc coefficients for phases 1/4, 2/4 and 3/4 between samples.
fn tp_coefs() -> &'static [[f32; TP_TAPS]; 3] {
    static C: std::sync::OnceLock<[[f32; TP_TAPS]; 3]> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let mut c = [[0.0f32; TP_TAPS]; 3];
        for (p, row) in c.iter_mut().enumerate() {
            let frac = (p + 1) as f32 / 4.0;
            for (k, v) in row.iter_mut().enumerate() {
                // tap k sits at offset (k − TP_DELAY + 1) samples from the interpolated point's left neighbour
                let t = k as f32 - (TP_DELAY as f32 - 1.0) - frac;
                let x = std::f32::consts::PI * t;
                let sinc = if t.abs() < 1e-6 { 1.0 } else { x.sin() / x };
                let w = 0.5 + 0.5 * (std::f32::consts::PI * t / (TP_TAPS as f32 / 2.0 + 0.5)).cos();
                *v = sinc * w.max(0.0);
            }
            let sum: f32 = row.iter().sum();
            row.iter_mut().for_each(|v| *v /= sum);
        }
        c
    })
}

impl Limiter {
    pub fn new(rate: f32) -> Self {
        let mut l = Self { rate, ceiling: 1.0, gain: 1.0, rel: 0.0, att: 0.0, look: 0, delay: Vec::new(), peaks: Vec::new(), pos: 0, gr: 0.0, input_gain: 1.0, true_peak: false, hist: Vec::new(), hpos: 0, dpos: 0 };
        l.set(-0.3, 80.0, 0.0, 3.0);
        l
    }
    pub fn set_true_peak(&mut self, on: bool) {
        if on != self.true_peak {
            self.true_peak = on;
            // the audio delay grows by the interpolator's centre; start the lines afresh
            self.delay.clear();
            self.hist.clear();
        }
    }
    /// Delay of the audio path: look-ahead plus the true-peak interpolator.
    fn path(&self) -> usize {
        self.look + if self.true_peak { TP_DELAY } else { 0 }
    }
    pub fn set(&mut self, ceiling_db: f32, release_ms: f32, input_db: f32, lookahead_ms: f32) {
        self.ceiling = db_to_gain(ceiling_db.min(0.0));
        self.rel = ms_to_coef(release_ms.max(1.0), self.rate);
        self.input_gain = db_to_gain(input_db);
        let look = ((lookahead_ms.clamp(0.5, 10.0) * 0.001 * self.rate) as usize).max(1);
        self.att = ms_to_coef(lookahead_ms.clamp(0.5, 10.0) / 3.0, self.rate);
        if look != self.look {
            self.look = look;
            self.delay.clear();
            self.peaks = vec![0.0; look];
            self.pos = 0;
        }
    }
}

impl Processor for Limiter {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let path = self.path();
        if self.delay.len() != buf.len() || self.delay.first().map(|d| d.len()) != Some(path) {
            self.delay = vec![vec![0.0; path]; buf.len()];
            self.dpos = 0;
        }
        if self.true_peak && self.hist.len() != buf.len() {
            self.hist = vec![[0.0; TP_TAPS]; buf.len()];
        }
        let coefs = tp_coefs();
        let mut max_gr = 0.0f32;
        let mut dpos = self.dpos % path.max(1);
        for i in 0..n {
            let mut pk = 0.0f32;
            if self.true_peak {
                // sample peak plus three interpolated points between the newest samples
                for (c, ch) in buf.iter().enumerate() {
                    let h = &mut self.hist[c];
                    h[self.hpos] = ch[i] * self.input_gain;
                    let mut m = h[(self.hpos + TP_TAPS - TP_DELAY) % TP_TAPS].abs();
                    for row in coefs.iter() {
                        let mut acc = 0.0f32;
                        for (k, w) in row.iter().enumerate() {
                            acc += w * h[(self.hpos + 1 + k) % TP_TAPS];
                        }
                        m = m.max(acc.abs());
                    }
                    pk = pk.max(m);
                }
                self.hpos = (self.hpos + 1) % TP_TAPS;
            } else {
                for ch in buf.iter() {
                    pk = pk.max((ch[i] * self.input_gain).abs());
                }
            }
            self.peaks[self.pos] = pk;
            // the window maximum over the look-ahead span
            let wmax = self.peaks.iter().cloned().fold(0.0f32, f32::max);
            let target = if wmax > self.ceiling { self.ceiling / wmax } else { 1.0 };
            if target < self.gain {
                // attack reaches the target within roughly a third of the look-ahead span
                self.gain = target + self.att * (self.gain - target);
            } else {
                self.gain = target + self.rel * (self.gain - target);
            }
            max_gr = max_gr.max(-gain_to_db(self.gain));
            for (c, ch) in buf.iter_mut().enumerate() {
                let d = &mut self.delay[c];
                let out = d[dpos];
                d[dpos] = ch[i] * self.input_gain;
                ch[i] = (out * self.gain).clamp(-self.ceiling, self.ceiling);
            }
            self.pos = (self.pos + 1) % self.look;
            dpos = (dpos + 1) % path;
        }
        self.dpos = dpos;
        self.gr = max_gr;
    }
    fn reset(&mut self) {
        self.delay.iter_mut().for_each(|d| d.iter_mut().for_each(|x| *x = 0.0));
        self.peaks.iter_mut().for_each(|x| *x = 0.0);
        self.gain = 1.0;
    }
    fn reduction_db(&self) -> f32 {
        self.gr
    }
    fn latency(&self) -> usize {
        self.path()
    }
}

/// Noise gate with hold and adjustable floor (range).
pub struct Gate {
    rate: f32,
    open_thr: f32,
    close_thr: f32,
    att: f32,
    rel: f32,
    hold: usize,
    hold_left: usize,
    floor: f32,
    g: f32,
    env: f32,
    env_rel: f32,
}

impl Gate {
    pub fn new(rate: f32) -> Self {
        let mut g = Self { rate, open_thr: 0.0, close_thr: 0.0, att: 0.0, rel: 0.0, hold: 0, hold_left: 0, floor: 0.0, g: 0.0, env: 0.0, env_rel: 0.0 };
        g.set(-45.0, 1.0, 120.0, 40.0, -80.0, 4.0);
        g
    }
    pub fn set(&mut self, threshold: f32, attack_ms: f32, release_ms: f32, hold_ms: f32, range_db: f32, hysteresis: f32) {
        self.open_thr = db_to_gain(threshold);
        self.close_thr = db_to_gain(threshold - hysteresis.max(0.0));
        self.att = ms_to_coef(attack_ms.max(0.05), self.rate);
        self.rel = ms_to_coef(release_ms.max(1.0), self.rate);
        self.hold = (hold_ms.max(0.0) * 0.001 * self.rate) as usize;
        self.floor = db_to_gain(range_db.min(0.0));
        self.env_rel = ms_to_coef(10.0, self.rate);
    }
}

impl Processor for Gate {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        for i in 0..n {
            let mut pk = 0.0f32;
            for ch in buf.iter() {
                pk = pk.max(ch[i].abs());
            }
            self.env = if pk > self.env { pk } else { pk + self.env_rel * (self.env - pk) };
            let open = if self.env >= self.open_thr {
                self.hold_left = self.hold;
                true
            } else if self.env >= self.close_thr || self.hold_left > 0 {
                self.hold_left = self.hold_left.saturating_sub(1);
                self.g > 0.5
            } else {
                false
            };
            let target = if open { 1.0 } else { self.floor };
            let c = if target > self.g { self.att } else { self.rel };
            self.g = target + c * (self.g - target);
            for ch in buf.iter_mut() {
                ch[i] *= self.g;
            }
        }
    }
    fn reset(&mut self) {
        self.g = 0.0;
        self.env = 0.0;
    }
}

/// De-esser: compresses only when the sibilance band exceeds the threshold, attenuating the band.
pub struct DeEsser {
    rate: f32,
    detect: Vec<Biquad>,
    split_lo: Vec<Biquad>,
    thr: f32,
    amount: f32,
    env: f32,
    att: f32,
    rel: f32,
    freq: f32,
    gr: f32,
}

impl DeEsser {
    pub fn new(rate: f32) -> Self {
        let mut d = Self { rate, detect: Vec::new(), split_lo: Vec::new(), thr: 0.0, amount: 0.0, env: 0.0, att: 0.0, rel: 0.0, freq: 6500.0, gr: 0.0 };
        d.set(6500.0, -30.0, 8.0);
        d
    }
    pub fn set(&mut self, freq: f32, threshold: f32, max_reduction: f32) {
        if (freq - self.freq).abs() > 0.5 {
            self.detect.clear();
            self.split_lo.clear();
        }
        self.freq = freq;
        self.thr = db_to_gain(threshold);
        self.amount = max_reduction.max(0.0);
        self.att = ms_to_coef(0.5, self.rate);
        self.rel = ms_to_coef(60.0, self.rate);
    }
}

impl Processor for DeEsser {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.detect.len() != buf.len() {
            self.detect = vec![Biquad::new(Coefs::new(FilterKind::HighPass, self.rate, self.freq, 0.707, 0.0)); buf.len()];
            self.split_lo = vec![Biquad::new(Coefs::new(FilterKind::LowPass, self.rate, self.freq, 0.707, 0.0)); buf.len()];
        }
        let mut mx = 0.0f32;
        for i in 0..n {
            let mut pk = 0.0f32;
            let mut hi = [0.0f32; 8];
            let mut lo = [0.0f32; 8];
            for (c, ch) in buf.iter().enumerate().take(8) {
                let h = self.detect[c].tick(ch[i]);
                lo[c] = self.split_lo[c].tick(ch[i]);
                hi[c] = ch[i] - lo[c];
                pk = pk.max(h.abs());
            }
            let c = if pk > self.env { self.att } else { self.rel };
            self.env = pk + c * (self.env - pk);
            let over = if self.env > self.thr { gain_to_db(self.env / self.thr) } else { 0.0 };
            let red = over.min(self.amount);
            mx = mx.max(red);
            let g = db_to_gain(-red);
            for (c, ch) in buf.iter_mut().enumerate().take(8) {
                ch[i] = lo[c] + hi[c] * g;
            }
        }
        self.gr = mx;
    }
    fn reduction_db(&self) -> f32 {
        self.gr
    }
}
