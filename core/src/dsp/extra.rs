//! More processors: ladder and auto filters, vowel filter, tilt EQ, comb filter, three-band
//! compressor, transient shaper, exciter, clipper, tape, ring modulator, vibrato, Haas widener.

use super::biquad::{Biquad, Coefs, FilterKind};
use super::{ms_to_coef, Processor};
use crate::buffer::{db_to_gain, gain_to_db};
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

fn ensure<T: Clone>(v: &mut Vec<T>, n: usize, init: T) {
    if v.len() != n {
        *v = vec![init; n];
    }
}

/// Fractional read from a circular delay line (`d` samples behind `pos`).
#[inline]
fn tap(line: &[f32], pos: usize, d: f32) -> f32 {
    let cap = line.len();
    let rp = pos as f32 - d;
    let rp = if rp < 0.0 { rp + cap as f32 } else { rp };
    let j = rp.floor() as usize % cap;
    let f = rp - rp.floor();
    let a = line[j];
    let b = line[(j + 1) % cap];
    a + (b - a) * f
}

/// Topology-preserving state-variable filter (Cytomic), one channel.
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Returns (low, band, high) for cutoff `g = tan(pi f / fs)` and damping `k = 1/Q`.
    #[inline]
    fn tick(&mut self, x: f32, g: f32, k: f32) -> (f32, f32, f32) {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterMode {
    LowPass,
    BandPass,
    HighPass,
}

// ───────────────────────────── ladder filter ─────────────────────────────

/// Four-pole ladder low-pass with resonance and drive (Huovilainen-style, simplified);
/// high- and band-pass are taken from the stages.
pub struct Ladder {
    rate: f32,
    mode: FilterMode,
    cutoff: f32,
    res: f32,
    drive: f32,
    s: Vec<[f32; 4]>,
}

impl Ladder {
    pub fn new(rate: f32) -> Self {
        Self { rate, mode: FilterMode::LowPass, cutoff: 1000.0, res: 0.3, drive: 1.0, s: Vec::new() }
    }
    pub fn set(&mut self, mode: FilterMode, cutoff: f32, resonance: f32, drive_db: f32) {
        self.mode = mode;
        self.cutoff = cutoff.clamp(20.0, self.rate * 0.45);
        self.res = resonance.clamp(0.0, 1.0);
        self.drive = db_to_gain(drive_db.clamp(0.0, 24.0));
    }
}

impl Processor for Ladder {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        ensure(&mut self.s, buf.len(), [0.0; 4]);
        let g = 1.0 - (-TAU * self.cutoff / self.rate).exp();
        let k = 4.0 * self.res * 0.98;
        let comp = 1.0 + self.res * 0.8; // keep the pass band roughly level as resonance rises
        for (c, ch) in buf.iter_mut().enumerate() {
            let s = &mut self.s[c];
            for x in ch[..n].iter_mut() {
                let inp = (*x * self.drive - k * s[3]).tanh();
                s[0] += g * (inp - s[0]);
                s[1] += g * (s[0] - s[1]);
                s[2] += g * (s[1] - s[2]);
                s[3] += g * (s[2] - s[3]);
                let y = match self.mode {
                    FilterMode::LowPass => s[3],
                    FilterMode::BandPass => 2.0 * (s[1] - s[3]),
                    FilterMode::HighPass => inp - 4.0 * s[0] + 6.0 * s[1] - 4.0 * s[2] + s[3],
                };
                *x = y * comp / self.drive.sqrt();
            }
        }
    }
    fn reset(&mut self) {
        self.s.iter_mut().for_each(|s| *s = [0.0; 4]);
    }
}

// ───────────────────────────── auto filter / auto-wah ─────────────────────────────

pub struct AutoFilter {
    rate: f32,
    mode: FilterMode,
    base: f32,
    depth: f32,
    lfo_hz: f32,
    envelope: bool,
    q: f32,
    sens: f32,
    mix: f32,
    phase: f32,
    env: f32,
    svf: Vec<Svf>,
}

impl AutoFilter {
    pub fn new(rate: f32) -> Self {
        Self { rate, mode: FilterMode::LowPass, base: 400.0, depth: 3.0, lfo_hz: 1.0, envelope: false, q: 4.0, sens: 1.0, mix: 1.0, phase: 0.0, env: 0.0, svf: Vec::new() }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn set(&mut self, mode: FilterMode, base: f32, depth_oct: f32, rate_hz: f32, envelope: bool, q: f32, sensitivity: f32, mix: f32) {
        self.mode = mode;
        self.base = base.clamp(20.0, 18000.0);
        self.depth = depth_oct.clamp(0.0, 7.0);
        self.lfo_hz = rate_hz.clamp(0.01, 20.0);
        self.envelope = envelope;
        self.q = q.clamp(0.5, 20.0);
        self.sens = sensitivity.clamp(0.0, 4.0);
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Processor for AutoFilter {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        ensure(&mut self.svf, buf.len(), Svf::default());
        let k = 1.0 / self.q;
        let inc = self.lfo_hz / self.rate;
        let (att, rel) = (ms_to_coef(4.0, self.rate), ms_to_coef(120.0, self.rate));
        let mut g = 0.0f32;
        for i in 0..n {
            if self.envelope {
                let lvl = buf.iter().map(|c| c[i].abs()).fold(0.0f32, f32::max);
                let c = if lvl > self.env { att } else { rel };
                self.env = lvl + c * (self.env - lvl);
            }
            if i % 16 == 0 {
                let m = if self.envelope { (self.env * self.sens * 4.0).min(1.0) } else { 0.5 - 0.5 * (self.phase * TAU).cos() };
                let f = (self.base * 2f32.powf(self.depth * m)).min(self.rate * 0.45);
                g = (PI * f / self.rate).tan();
            }
            self.phase = (self.phase + inc).fract();
            for (c, ch) in buf.iter_mut().enumerate() {
                let x = ch[i];
                let (lp, bp, hp) = self.svf[c].tick(x, g, k);
                let y = match self.mode {
                    FilterMode::LowPass => lp,
                    FilterMode::BandPass => bp,
                    FilterMode::HighPass => hp,
                };
                ch[i] = x + (y - x) * self.mix;
            }
        }
    }
    fn reset(&mut self) {
        self.svf.iter_mut().for_each(|s| *s = Svf::default());
        self.env = 0.0;
    }
}

// ───────────────────────────── vowel (formant) filter ─────────────────────────────

/// Formant frequencies (F1, F2, F3) and relative gains for a, e, i, o, u.
const VOWELS: [[(f32, f32); 3]; 5] = [
    [(800.0, 1.0), (1150.0, 0.5), (2900.0, 0.25)],
    [(400.0, 1.0), (1600.0, 0.35), (2700.0, 0.2)],
    [(350.0, 1.0), (1700.0, 0.2), (2700.0, 0.12)],
    [(450.0, 1.0), (800.0, 0.6), (2830.0, 0.08)],
    [(325.0, 1.0), (700.0, 0.4), (2530.0, 0.05)],
];

pub struct Vowel {
    rate: f32,
    vowel: f32,
    q: f32,
    lfo_hz: f32,
    depth: f32,
    mix: f32,
    phase: f32,
    svf: Vec<[Svf; 3]>,
}

impl Vowel {
    pub fn new(rate: f32) -> Self {
        Self { rate, vowel: 0.0, q: 8.0, lfo_hz: 0.0, depth: 0.0, mix: 1.0, phase: 0.0, svf: Vec::new() }
    }
    pub fn set(&mut self, vowel: f32, resonance: f32, rate_hz: f32, depth: f32, mix: f32) {
        self.vowel = vowel.clamp(0.0, 4.0);
        self.q = 2.0 + resonance.clamp(0.0, 1.0) * 18.0;
        self.lfo_hz = rate_hz.clamp(0.0, 10.0);
        self.depth = depth.clamp(0.0, 4.0);
        self.mix = mix.clamp(0.0, 1.0);
    }
    fn formants(v: f32) -> [(f32, f32); 3] {
        let v = v.clamp(0.0, 4.0);
        let a = (v.floor() as usize).min(3);
        let t = v - a as f32;
        let mut out = [(0.0, 0.0); 3];
        for (k, o) in out.iter_mut().enumerate() {
            let (f0, g0) = VOWELS[a][k];
            let (f1, g1) = VOWELS[a + 1][k];
            *o = (f0 * (f1 / f0).powf(t), g0 + (g1 - g0) * t);
        }
        out
    }
}

impl Processor for Vowel {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        ensure(&mut self.svf, buf.len(), [Svf::default(); 3]);
        let k = 1.0 / self.q;
        let inc = self.lfo_hz / self.rate;
        let mut coefs = [(0.0f32, 0.0f32); 3];
        // band-pass peaks get louder with Q; scale back so the level stays put
        let norm = 2.0 * k.sqrt();
        for i in 0..n {
            if i % 32 == 0 {
                let v = self.vowel + self.depth * 0.5 * (1.0 - (self.phase * TAU).cos()) * if self.lfo_hz > 0.0 { 1.0 } else { 0.0 };
                let v = if v > 4.0 { 8.0 - v } else { v };
                for (c, (f, gn)) in coefs.iter_mut().zip(Self::formants(v)) {
                    *c = ((PI * f.min(self.rate * 0.45) / self.rate).tan(), gn);
                }
            }
            self.phase = (self.phase + inc).fract();
            for (c, ch) in buf.iter_mut().enumerate() {
                let x = ch[i];
                let mut y = 0.0;
                for (s, (g, gn)) in self.svf[c].iter_mut().zip(coefs) {
                    y += s.tick(x, g, k).1 * gn;
                }
                ch[i] = x + (y * norm * 2.5 - x) * self.mix;
            }
        }
    }
    fn reset(&mut self) {
        self.svf.iter_mut().for_each(|s| *s = [Svf::default(); 3]);
    }
}

// ───────────────────────────── tilt EQ ─────────────────────────────

pub struct Tilt {
    rate: f32,
    low: Vec<Biquad>,
    high: Vec<Biquad>,
    cl: Coefs,
    ch: Coefs,
}

impl Tilt {
    pub fn new(rate: f32) -> Self {
        let c = Coefs::new(FilterKind::LowShelf, rate, 800.0, 0.5, 0.0);
        Self { rate, low: Vec::new(), high: Vec::new(), cl: c, ch: c }
    }
    pub fn set(&mut self, tilt_db: f32, pivot: f32) {
        let t = tilt_db.clamp(-12.0, 12.0) * 0.5;
        let p = pivot.clamp(100.0, 8000.0);
        self.cl = Coefs::new(FilterKind::LowShelf, self.rate, p, 0.5, -t);
        self.ch = Coefs::new(FilterKind::HighShelf, self.rate, p, 0.5, t);
        self.low.iter_mut().for_each(|b| b.c = self.cl);
        self.high.iter_mut().for_each(|b| b.c = self.ch);
    }
}

impl Processor for Tilt {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.low.len() != buf.len() {
            self.low = (0..buf.len()).map(|_| Biquad::new(self.cl)).collect();
            self.high = (0..buf.len()).map(|_| Biquad::new(self.ch)).collect();
        }
        for (c, ch) in buf.iter_mut().enumerate() {
            for x in ch[..n].iter_mut() {
                *x = self.high[c].tick(self.low[c].tick(*x));
            }
        }
    }
    fn reset(&mut self) {
        self.low.iter_mut().chain(self.high.iter_mut()).for_each(|b| b.reset());
    }
}

// ───────────────────────────── comb filter ─────────────────────────────

pub struct Comb {
    rate: f32,
    freq: f32,
    feedback: f32,
    mix: f32,
    lines: Vec<Vec<f32>>,
    pos: usize,
}

impl Comb {
    pub fn new(rate: f32) -> Self {
        Self { rate, freq: 200.0, feedback: 0.7, mix: 0.5, lines: Vec::new(), pos: 0 }
    }
    pub fn set(&mut self, freq: f32, feedback: f32, mix: f32) {
        self.freq = freq.clamp(20.0, 5000.0);
        self.feedback = feedback.clamp(-0.97, 0.97);
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Processor for Comb {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let cap = (self.rate / 20.0) as usize + 4;
        if self.lines.len() != buf.len() || self.lines.first().map(|l| l.len()) != Some(cap) {
            self.lines = vec![vec![0.0; cap]; buf.len()];
            self.pos = 0;
        }
        let d = self.rate / self.freq;
        // feedback combs ring loudly near ±1: keep the peak gain in check
        let norm = 1.0 - self.feedback.abs() * 0.85;
        for i in 0..n {
            for (c, ch) in buf.iter_mut().enumerate() {
                let y = tap(&self.lines[c], self.pos, d);
                let x = ch[i];
                self.lines[c][self.pos] = x + y * self.feedback;
                ch[i] = x + ((x + y * self.feedback) * norm - x) * self.mix;
            }
            self.pos = (self.pos + 1) % cap;
        }
    }
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.fill(0.0));
    }
}

// ───────────────────────────── three-band compressor ─────────────────────────────

/// Linkwitz-Riley 4th-order crossover = two Butterworth biquads in series.
#[derive(Clone)]
struct Lr4 {
    a: Biquad,
    b: Biquad,
}

impl Lr4 {
    fn new(kind: FilterKind, rate: f32, f: f32) -> Self {
        let c = Coefs::new(kind, rate, f, std::f32::consts::FRAC_1_SQRT_2, 0.0);
        Self { a: Biquad::new(c), b: Biquad::new(c) }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        self.b.tick(self.a.tick(x))
    }
}

struct BandDyn {
    env_db: f32,
}

pub struct Multiband {
    rate: f32,
    lo_x: f32,
    hi_x: f32,
    thr: [f32; 3],
    gain: [f32; 3],
    ratio: f32,
    att: f32,
    rel: f32,
    // per channel: low LP, mid HP, mid LP, high HP, plus all-pass compensation for the low band
    xo: Vec<[Lr4; 4]>,
    ap: Vec<[Lr4; 2]>,
    dyn_: [BandDyn; 3],
    gr: f32,
}

impl Multiband {
    pub fn new(rate: f32) -> Self {
        Self {
            rate,
            lo_x: 200.0,
            hi_x: 3000.0,
            thr: [-20.0; 3],
            gain: [0.0; 3],
            ratio: 3.0,
            att: 0.0,
            rel: 0.0,
            xo: Vec::new(),
            ap: Vec::new(),
            dyn_: [BandDyn { env_db: 0.0 }, BandDyn { env_db: 0.0 }, BandDyn { env_db: 0.0 }],
            gr: 0.0,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn set(&mut self, low_x: f32, high_x: f32, thr: [f32; 3], gain: [f32; 3], ratio: f32, attack: f32, release: f32) {
        let lo = low_x.clamp(40.0, 2000.0);
        let hi = high_x.clamp(lo * 1.5, 16000.0);
        if (lo - self.lo_x).abs() > 0.5 || (hi - self.hi_x).abs() > 0.5 {
            self.xo.clear();
        }
        self.lo_x = lo;
        self.hi_x = hi;
        self.thr = thr;
        self.gain = gain;
        self.ratio = ratio.max(1.0);
        self.att = ms_to_coef(attack.max(0.1), self.rate);
        self.rel = ms_to_coef(release.max(5.0), self.rate);
    }
    fn build(&mut self, ch: usize) {
        let r = self.rate;
        self.xo = (0..ch)
            .map(|_| [Lr4::new(FilterKind::LowPass, r, self.lo_x), Lr4::new(FilterKind::HighPass, r, self.lo_x), Lr4::new(FilterKind::LowPass, r, self.hi_x), Lr4::new(FilterKind::HighPass, r, self.hi_x)])
            .collect();
        // the low band skips the high crossover: an LR4 all-pass at hi_x keeps its phase aligned
        self.ap = (0..ch).map(|_| [Lr4::new(FilterKind::LowPass, r, self.hi_x), Lr4::new(FilterKind::HighPass, r, self.hi_x)]).collect();
    }
}

impl Processor for Multiband {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let chs = buf.len();
        if self.xo.len() != chs {
            self.build(chs);
        }
        let mut bands = [0.0f32; 3];
        let mut max_gr = 0.0f32;
        let mut parts = vec![[0.0f32; 3]; chs];
        for i in 0..n {
            let mut peak = [0.0f32; 3];
            for (c, ch) in buf.iter().enumerate() {
                let x = ch[i];
                let [lp1, hp1, lp2, hp2] = &mut self.xo[c];
                let low = lp1.tick(x);
                let rest = hp1.tick(x);
                let mid = lp2.tick(rest);
                let high = hp2.tick(rest);
                let [a, b] = &mut self.ap[c];
                let low = a.tick(low) + b.tick(low);
                parts[c] = [low, mid, high];
                for k in 0..3 {
                    peak[k] = peak[k].max(parts[c][k].abs());
                }
            }
            for k in 0..3 {
                let x = gain_to_db(peak[k]);
                let over = (x - self.thr[k]).max(0.0);
                let want = over * (1.0 - 1.0 / self.ratio);
                let d = &mut self.dyn_[k];
                let c = if want > d.env_db { self.att } else { self.rel };
                d.env_db = want + c * (d.env_db - want);
                max_gr = max_gr.max(d.env_db);
                bands[k] = db_to_gain(self.gain[k] - d.env_db);
            }
            for (c, ch) in buf.iter_mut().enumerate() {
                ch[i] = parts[c][0] * bands[0] + parts[c][1] * bands[1] + parts[c][2] * bands[2];
            }
        }
        self.gr = max_gr;
    }
    fn reset(&mut self) {
        self.xo.clear();
        self.dyn_.iter_mut().for_each(|d| d.env_db = 0.0);
    }
    fn reduction_db(&self) -> f32 {
        self.gr
    }
}

// ───────────────────────────── transient shaper ─────────────────────────────

/// Attack/sustain shaping from the difference of a fast and a slow envelope.
pub struct Transient {
    rate: f32,
    attack: f32,
    sustain: f32,
    out: f32,
    fast: f32,
    slow: f32,
}

impl Transient {
    pub fn new(rate: f32) -> Self {
        Self { rate, attack: 0.0, sustain: 0.0, out: 1.0, fast: 0.0, slow: 0.0 }
    }
    pub fn set(&mut self, attack: f32, sustain: f32, output_db: f32) {
        self.attack = attack.clamp(-1.0, 1.0);
        self.sustain = sustain.clamp(-1.0, 1.0);
        self.out = db_to_gain(output_db);
    }
}

impl Processor for Transient {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let (fa, fr) = (ms_to_coef(1.0, self.rate), ms_to_coef(20.0, self.rate));
        let (sa, sr) = (ms_to_coef(30.0, self.rate), ms_to_coef(250.0, self.rate));
        for i in 0..n {
            let lvl = buf.iter().map(|c| c[i].abs()).fold(0.0f32, f32::max);
            self.fast = lvl + if lvl > self.fast { fa } else { fr } * (self.fast - lvl);
            self.slow = lvl + if lvl > self.slow { sa } else { sr } * (self.slow - lvl);
            // positive while a hit rises (attack), negative in the decay (sustain)
            let d = gain_to_db(self.fast + 1e-9) - gain_to_db(self.slow + 1e-9);
            let g_db = if d > 0.0 { self.attack * d.min(24.0) } else { self.sustain * (-d).min(24.0) };
            let g = db_to_gain(g_db.clamp(-18.0, 12.0)) * self.out;
            for ch in buf.iter_mut() {
                ch[i] *= g;
            }
        }
    }
    fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
    }
}

// ───────────────────────────── exciter ─────────────────────────────

pub struct Exciter {
    rate: f32,
    freq: f32,
    drive: f32,
    mix: f32,
    pre: Vec<Biquad>,
    post: Vec<Biquad>,
}

impl Exciter {
    pub fn new(rate: f32) -> Self {
        Self { rate, freq: 3000.0, drive: 4.0, mix: 0.3, pre: Vec::new(), post: Vec::new() }
    }
    pub fn set(&mut self, freq: f32, drive_db: f32, mix: f32) {
        let f = freq.clamp(500.0, 16000.0);
        if (f - self.freq).abs() > 0.5 {
            self.pre.clear();
        }
        self.freq = f;
        self.drive = db_to_gain(drive_db.clamp(0.0, 36.0));
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Processor for Exciter {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.pre.len() != buf.len() {
            let c = Coefs::new(FilterKind::HighPass, self.rate, self.freq, 0.707, 0.0);
            let c2 = Coefs::new(FilterKind::HighPass, self.rate, self.freq * 0.8, 0.707, 0.0);
            self.pre = (0..buf.len()).map(|_| Biquad::new(c)).collect();
            self.post = (0..buf.len()).map(|_| Biquad::new(c2)).collect();
        }
        for (c, ch) in buf.iter_mut().enumerate() {
            for x in ch[..n].iter_mut() {
                let h = self.pre[c].tick(*x);
                // asymmetric saturation adds both even and odd harmonics
                let s = (h * self.drive).tanh() + 0.2 * (h * self.drive).powi(2).min(1.0);
                let s = self.post[c].tick(s) / self.drive.sqrt();
                *x += s * self.mix;
            }
        }
    }
    fn reset(&mut self) {
        self.pre.iter_mut().chain(self.post.iter_mut()).for_each(|b| b.reset());
    }
}

// ───────────────────────────── clipper ─────────────────────────────

pub struct Clipper {
    input: f32,
    ceiling: f32,
    soft: f32,
    out: f32,
    gr: f32,
}

impl Clipper {
    pub fn new() -> Self {
        Self { input: 1.0, ceiling: 1.0, soft: 0.5, out: 1.0, gr: 0.0 }
    }
    pub fn set(&mut self, input_db: f32, ceiling_db: f32, softness: f32, output_db: f32) {
        self.input = db_to_gain(input_db);
        self.ceiling = db_to_gain(ceiling_db.min(0.0));
        self.soft = softness.clamp(0.0, 1.0);
        self.out = db_to_gain(output_db);
    }
    #[inline]
    fn clip(&self, x: f32) -> f32 {
        let c = self.ceiling;
        let knee = c * self.soft;
        let a = x.abs();
        let y = if a <= c - knee {
            a
        } else if knee > 0.0 && a < c + knee {
            // quadratic knee joining the linear part to the ceiling
            let t = a - (c - knee);
            (c - knee) + t - t * t / (4.0 * knee)
        } else {
            c
        };
        y.copysign(x)
    }
}

impl Default for Clipper {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for Clipper {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let mut gr = 0.0f32;
        for ch in buf.iter_mut() {
            for x in ch[..n].iter_mut() {
                let v = *x * self.input;
                let y = self.clip(v);
                if v.abs() > 1e-6 {
                    gr = gr.max(gain_to_db(v.abs()) - gain_to_db(y.abs()));
                }
                *x = y * self.out;
            }
        }
        self.gr = gr;
    }
    fn reduction_db(&self) -> f32 {
        self.gr
    }
}

// ───────────────────────────── tape ─────────────────────────────

pub struct Tape {
    rate: f32,
    drive: f32,
    wow: f32,
    flutter: f32,
    hiss: f32,
    mix: f32,
    tone: Vec<Biquad>,
    tone_hz: f32,
    lines: Vec<Vec<f32>>,
    pos: usize,
    ph_w: f32,
    ph_f: f32,
    seed: u32,
}

impl Tape {
    pub fn new(rate: f32) -> Self {
        Self { rate, drive: 1.5, wow: 0.3, flutter: 0.2, hiss: 0.0, mix: 1.0, tone: Vec::new(), tone_hz: 12000.0, lines: Vec::new(), pos: 0, ph_w: 0.0, ph_f: 0.0, seed: 0x1234_5678 }
    }
    pub fn set(&mut self, drive_db: f32, wow: f32, flutter: f32, hiss_db: f32, tone: f32, mix: f32) {
        self.drive = db_to_gain(drive_db.clamp(0.0, 24.0));
        self.wow = wow.clamp(0.0, 1.0);
        self.flutter = flutter.clamp(0.0, 1.0);
        self.hiss = if hiss_db <= -95.0 { 0.0 } else { db_to_gain(hiss_db) };
        let t = tone.clamp(1000.0, 20000.0).min(self.rate * 0.45);
        if (t - self.tone_hz).abs() > 1.0 {
            self.tone.clear();
        }
        self.tone_hz = t;
        self.mix = mix.clamp(0.0, 1.0);
    }
    #[inline]
    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl Processor for Tape {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let cap = (self.rate * 0.03) as usize + 4;
        if self.lines.len() != buf.len() {
            self.lines = vec![vec![0.0; cap]; buf.len()];
            self.pos = 0;
        }
        if self.tone.len() != buf.len() {
            let c = Coefs::new(FilterKind::LowPass, self.rate, self.tone_hz, 0.707, 0.0);
            self.tone = (0..buf.len()).map(|_| Biquad::new(c)).collect();
        }
        let base = self.rate * 0.005;
        let (iw, iff) = (0.6 / self.rate, 7.0 / self.rate);
        let norm = 1.0 / self.drive.tanh().max(0.1);
        for i in 0..n {
            let d = base + self.wow * self.rate * 0.0025 * (self.ph_w * TAU).sin() + self.flutter * self.rate * 0.0003 * (self.ph_f * TAU).sin();
            self.ph_w = (self.ph_w + iw).fract();
            self.ph_f = (self.ph_f + iff).fract();
            let h = self.noise() * self.hiss;
            for (c, ch) in buf.iter_mut().enumerate() {
                let x = ch[i];
                self.lines[c][self.pos] = x;
                let y = tap(&self.lines[c], self.pos, d);
                let y = (y * self.drive).tanh() * norm * 0.9 + h;
                let y = self.tone[c].tick(y);
                ch[i] = x + (y - x) * self.mix;
            }
            self.pos = (self.pos + 1) % cap;
        }
    }
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.fill(0.0));
        self.tone.iter_mut().for_each(|b| b.reset());
    }
    fn latency(&self) -> usize {
        (self.rate * 0.005) as usize
    }
}

// ───────────────────────────── ring modulator ─────────────────────────────

pub struct RingMod {
    rate: f32,
    freq: f32,
    mix: f32,
    phase: f32,
}

impl RingMod {
    pub fn new(rate: f32) -> Self {
        Self { rate, freq: 440.0, mix: 0.5, phase: 0.0 }
    }
    pub fn set(&mut self, freq: f32, mix: f32) {
        self.freq = freq.clamp(1.0, 5000.0);
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Processor for RingMod {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let inc = self.freq / self.rate;
        for i in 0..n {
            let m = (self.phase * TAU).sin();
            self.phase = (self.phase + inc).fract();
            for ch in buf.iter_mut() {
                let x = ch[i];
                ch[i] = x + (x * m - x) * self.mix;
            }
        }
    }
}

// ───────────────────────────── vibrato ─────────────────────────────

pub struct Vibrato {
    rate: f32,
    hz: f32,
    depth_ms: f32,
    lines: Vec<Vec<f32>>,
    pos: usize,
    phase: f32,
}

impl Vibrato {
    pub fn new(rate: f32) -> Self {
        Self { rate, hz: 5.0, depth_ms: 2.0, lines: Vec::new(), pos: 0, phase: 0.0 }
    }
    pub fn set(&mut self, rate_hz: f32, depth_ms: f32) {
        self.hz = rate_hz.clamp(0.1, 14.0);
        self.depth_ms = depth_ms.clamp(0.0, 10.0);
    }
}

impl Processor for Vibrato {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let cap = (self.rate * 0.025) as usize + 4;
        if self.lines.len() != buf.len() {
            self.lines = vec![vec![0.0; cap]; buf.len()];
            self.pos = 0;
        }
        let inc = self.hz / self.rate;
        let base = self.depth_ms * 0.001 * self.rate + 2.0;
        for i in 0..n {
            let d = base + self.depth_ms * 0.001 * self.rate * (self.phase * TAU).sin();
            self.phase = (self.phase + inc).fract();
            for (c, ch) in buf.iter_mut().enumerate() {
                self.lines[c][self.pos] = ch[i];
                ch[i] = tap(&self.lines[c], self.pos, d);
            }
            self.pos = (self.pos + 1) % cap;
        }
    }
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.fill(0.0));
    }
}

// ───────────────────────────── Haas widener ─────────────────────────────

pub struct Haas {
    rate: f32,
    delay: usize,
    right: bool,
    mix: f32,
    line: Vec<f32>,
    pos: usize,
}

impl Haas {
    pub fn new(rate: f32) -> Self {
        Self { rate, delay: 0, right: true, mix: 1.0, line: Vec::new(), pos: 0 }
    }
    pub fn set(&mut self, delay_ms: f32, right: bool, mix: f32) {
        self.delay = (delay_ms.clamp(0.0, 40.0) * 0.001 * self.rate) as usize;
        self.right = right;
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Processor for Haas {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if buf.len() < 2 {
            return;
        }
        let cap = (self.rate * 0.05) as usize + 2;
        if self.line.len() != cap {
            self.line = vec![0.0; cap];
            self.pos = 0;
        }
        let c = if self.right { 1 } else { 0 };
        for i in 0..n {
            let x = buf[c][i];
            self.line[self.pos] = x;
            let y = self.line[(self.pos + cap - self.delay.min(cap - 1)) % cap];
            buf[c][i] = x + (y - x) * self.mix;
            self.pos = (self.pos + 1) % cap;
        }
    }
    fn reset(&mut self) {
        self.line.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: f32, f: f32, n: usize, a: f32) -> Vec<Vec<f32>> {
        let s: Vec<f32> = (0..n).map(|i| a * (i as f32 * f * TAU / rate).sin()).collect();
        vec![s.clone(), s]
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }
    fn run(p: &mut dyn Processor, mut b: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        let n = b[0].len();
        // in blocks, as the engine does
        let mut out = vec![Vec::new(), Vec::new()];
        let mut i = 0;
        while i < n {
            let k = (n - i).min(512);
            let mut blk: Vec<Vec<f32>> = b.iter().map(|c| c[i..i + k].to_vec()).collect();
            p.process(&mut blk, k);
            for c in 0..2 {
                out[c].extend_from_slice(&blk[c]);
            }
            i += k;
        }
        b.clear();
        out
    }

    /// Every processor stays finite and bounded on loud input.
    #[test]
    fn all_are_stable() {
        let r = 48000.0;
        let input = sine(r, 220.0, 48000, 0.9);
        let mut procs: Vec<Box<dyn Processor>> = vec![
            Box::new({ let mut p = Ladder::new(r); p.set(FilterMode::LowPass, 800.0, 0.95, 12.0); p }),
            Box::new({ let mut p = AutoFilter::new(r); p.set(FilterMode::BandPass, 300.0, 4.0, 2.0, false, 10.0, 1.0, 1.0); p }),
            Box::new({ let mut p = Vowel::new(r); p.set(2.5, 1.0, 1.0, 2.0, 1.0); p }),
            Box::new({ let mut p = Tilt::new(r); p.set(10.0, 1000.0); p }),
            Box::new({ let mut p = Comb::new(r); p.set(220.0, 0.95, 1.0); p }),
            Box::new({ let mut p = Multiband::new(r); p.set(200.0, 3000.0, [-30.0; 3], [3.0; 3], 4.0, 5.0, 100.0); p }),
            Box::new({ let mut p = Transient::new(r); p.set(1.0, 1.0, 0.0); p }),
            Box::new({ let mut p = Exciter::new(r); p.set(2000.0, 24.0, 1.0); p }),
            Box::new({ let mut p = Clipper::new(); p.set(12.0, -1.0, 1.0, 0.0); p }),
            Box::new({ let mut p = Tape::new(r); p.set(12.0, 1.0, 1.0, -60.0, 8000.0, 1.0); p }),
            Box::new({ let mut p = RingMod::new(r); p.set(300.0, 1.0); p }),
            Box::new({ let mut p = Vibrato::new(r); p.set(6.0, 5.0); p }),
            Box::new({ let mut p = Haas::new(r); p.set(20.0, true, 1.0); p }),
        ];
        for (k, p) in procs.iter_mut().enumerate() {
            let out = run(p.as_mut(), input.clone());
            for c in &out {
                assert!(c.iter().all(|v| v.is_finite()), "processor {k}: not finite");
                let pk = c.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(pk < 8.0, "processor {k}: peak {pk}");
            }
        }
    }

    #[test]
    fn ladder_lowpass_cuts_highs() {
        let r = 48000.0;
        let mut p = Ladder::new(r);
        p.set(FilterMode::LowPass, 500.0, 0.0, 0.0);
        let lo = run(&mut p, sine(r, 100.0, 24000, 0.3));
        p.reset();
        let hi = run(&mut p, sine(r, 8000.0, 24000, 0.3));
        assert!(rms(&hi[0][4000..]) < rms(&lo[0][4000..]) * 0.05);
    }

    /// The crossover sums back to (nearly) the input when nothing is compressed.
    #[test]
    fn multiband_is_transparent_below_threshold() {
        let r = 48000.0;
        let mut p = Multiband::new(r);
        p.set(200.0, 3000.0, [0.0; 3], [0.0; 3], 4.0, 5.0, 100.0);
        for f in [80.0, 1000.0, 9000.0] {
            p.reset();
            let out = run(&mut p, sine(r, f, 24000, 0.2));
            let a = rms(&out[0][8000..]);
            assert!((a / (0.2 / 2f32.sqrt()) - 1.0).abs() < 0.08, "{f} Hz: {a}");
        }
    }

    #[test]
    fn clipper_respects_ceiling() {
        let mut p = Clipper::new();
        p.set(12.0, -3.0, 0.3, 0.0);
        let out = run(&mut p, sine(48000.0, 100.0, 4800, 0.9));
        let pk = out[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(pk <= db_to_gain(-3.0) + 1e-4, "{pk}");
    }
}
