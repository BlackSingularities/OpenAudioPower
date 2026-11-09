//! RBJ "Audio EQ cookbook" biquads and a multi-band parametric equaliser.

use super::Processor;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterKind {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
    AllPass,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Coefs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coefs {
    pub fn new(kind: FilterKind, rate: f32, freq: f32, q: f32, gain_db: f32) -> Self {
        let f = freq.clamp(10.0, rate * 0.49);
        let q = q.max(0.05);
        let w0 = 2.0 * PI * f / rate;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q);
        let a = 10f32.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterKind::LowPass => ((1.0 - cs) / 2.0, 1.0 - cs, (1.0 - cs) / 2.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterKind::HighPass => ((1.0 + cs) / 2.0, -(1.0 + cs), (1.0 + cs) / 2.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterKind::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterKind::Notch => (1.0, -2.0 * cs, 1.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterKind::AllPass => (1.0 - alpha, -2.0 * cs, 1.0 + alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterKind::Peak => (1.0 + alpha * a, -2.0 * cs, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cs, 1.0 - alpha / a),
            FilterKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cs + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
                    a * ((a + 1.0) - (a - 1.0) * cs - s),
                    (a + 1.0) + (a - 1.0) * cs + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cs),
                    (a + 1.0) + (a - 1.0) * cs - s,
                )
            }
            FilterKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cs + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
                    a * ((a + 1.0) + (a - 1.0) * cs - s),
                    (a + 1.0) - (a - 1.0) * cs + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cs),
                    (a + 1.0) - (a - 1.0) * cs - s,
                )
            }
        };
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// Magnitude response in dB at `freq` – used to draw EQ curves.
    pub fn response_db(&self, freq: f32, rate: f32) -> f32 {
        let w = 2.0 * PI * freq / rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        let m = ((nr * nr + ni * ni) / (dr * dr + di * di).max(1e-20)).sqrt();
        20.0 * m.max(1e-9).log10()
    }
}

#[derive(Clone, Copy, Default)]
pub struct Biquad {
    pub c: Coefs,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(c: Coefs) -> Self {
        Self { c, z1: 0.0, z2: 0.0 }
    }
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        y
    }
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Which part of a stereo signal a band works on.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum EqTarget {
    #[default]
    Stereo,
    Mid,
    Side,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EqBand {
    pub kind: FilterKind,
    pub freq: f32,
    pub q: f32,
    pub gain: f32,
    #[serde(default = "on")]
    pub on: bool,
    #[serde(default)]
    pub target: EqTarget,
}
fn on() -> bool {
    true
}

/// FIR length of the linear-phase mode (≈ 85 ms at 48 kHz, enough for low shelves).
const LIN_TAPS: usize = 4096;

/// Multi-band parametric EQ (any number of bands, any channel count). Bands can work on the
/// mid or side signal of a stereo pair; `linear` runs the whole curve as a linear-phase FIR.
pub struct Equalizer {
    rate: f32,
    bands: Vec<EqBand>,
    filters: Vec<Vec<Biquad>>, // [band][channel]
    linear: bool,
    /// linear-phase convolvers: [mid or mono, side]
    conv: Vec<super::fir::Convolver>,
}

impl Equalizer {
    pub fn new(rate: f32, bands: &[EqBand]) -> Self {
        let mut e = Self { rate, bands: Vec::new(), filters: Vec::new(), linear: false, conv: Vec::new() };
        e.set(bands);
        e
    }
    pub fn set(&mut self, bands: &[EqBand]) {
        if bands.len() != self.bands.len() {
            self.filters = vec![Vec::new(); bands.len()];
        }
        for (i, b) in bands.iter().enumerate() {
            let c = Coefs::new(b.kind, self.rate, b.freq, b.q, b.gain);
            for f in self.filters[i].iter_mut() {
                f.c = c;
            }
        }
        self.bands = bands.to_vec();
        if self.linear {
            self.design();
        }
    }
    pub fn set_linear(&mut self, on: bool) {
        if on != self.linear {
            self.linear = on;
            self.conv.clear();
            if on {
                self.design();
            }
        }
    }
    /// Response (dB) of the bands that reach the mid (`side` = false) or side signal.
    fn response(&self, side: bool, f: f32) -> f32 {
        self.bands
            .iter()
            .filter(|b| b.on && (b.target == EqTarget::Stereo || (b.target == EqTarget::Side) == side))
            .map(|b| Coefs::new(b.kind, self.rate, b.freq, b.q, b.gain).response_db(f, self.rate))
            .sum()
    }
    fn design(&mut self) {
        let rate = self.rate;
        let hm = super::fir::design(LIN_TAPS, rate, &|f| self.response(false, f));
        let hs = super::fir::design(LIN_TAPS, rate, &|f| self.response(true, f));
        if self.conv.len() == 2 {
            self.conv[0].set(&hm);
            self.conv[1].set(&hs);
        } else {
            self.conv = vec![super::fir::Convolver::new(&hm), super::fir::Convolver::new(&hs)];
        }
    }
    fn ensure(&mut self, chans: usize) {
        for (i, fs) in self.filters.iter_mut().enumerate() {
            if fs.len() < chans {
                let b = &self.bands[i];
                let c = Coefs::new(b.kind, self.rate, b.freq, b.q, b.gain);
                fs.resize(chans, Biquad::new(c));
            }
        }
    }
}

fn to_ms(buf: &mut [Vec<f32>], n: usize) {
    let (l, r) = buf.split_at_mut(1);
    for i in 0..n {
        let (a, b) = (l[0][i], r[0][i]);
        l[0][i] = (a + b) * 0.5;
        r[0][i] = (a - b) * 0.5;
    }
}
fn to_lr(buf: &mut [Vec<f32>], n: usize) {
    let (m, s) = buf.split_at_mut(1);
    for i in 0..n {
        let (a, b) = (m[0][i], s[0][i]);
        m[0][i] = a + b;
        s[0][i] = a - b;
    }
}

impl Processor for Equalizer {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let stereo = buf.len() == 2;
        if self.linear {
            if self.conv.len() != 2 {
                self.design();
            }
            if stereo {
                to_ms(buf, n);
                self.conv[0].process(&mut buf[0][..n]);
                self.conv[1].process(&mut buf[1][..n]);
                to_lr(buf, n);
            } else {
                for ch in buf.iter_mut() {
                    self.conv[0].process(&mut ch[..n]);
                }
            }
            return;
        }
        self.ensure(buf.len());
        let ms = stereo && self.bands.iter().any(|b| b.on && b.target != EqTarget::Stereo);
        // stereo bands on L/R (and on mono signals every band)
        for (bi, band) in self.bands.iter().enumerate() {
            if !band.on || (stereo && band.target != EqTarget::Stereo) || (!stereo && band.target == EqTarget::Side) {
                continue;
            }
            for (c, ch) in buf.iter_mut().enumerate() {
                let f = &mut self.filters[bi][c];
                for s in ch[..n].iter_mut() {
                    *s = f.tick(*s);
                }
            }
        }
        if ms {
            to_ms(buf, n);
            for (bi, band) in self.bands.iter().enumerate() {
                if !band.on || band.target == EqTarget::Stereo {
                    continue;
                }
                let c = if band.target == EqTarget::Mid { 0 } else { 1 };
                let f = &mut self.filters[bi][c];
                for s in buf[c][..n].iter_mut() {
                    *s = f.tick(*s);
                }
            }
            to_lr(buf, n);
        }
    }
    fn reset(&mut self) {
        self.filters.iter_mut().flatten().for_each(|f| f.reset());
        self.conv.iter_mut().for_each(|c| c.reset());
    }
    fn latency(&self) -> usize {
        if self.linear { self.conv.first().map(|c| c.block()).unwrap_or(LIN_TAPS) + LIN_TAPS / 2 } else { 0 }
    }
}

/// Simple single filter (low-cut / high-cut / band) usable as an effect.
pub struct Filter {
    rate: f32,
    kind: FilterKind,
    freq: f32,
    q: f32,
    /// Number of cascaded stages (1 = 12 dB/oct, 2 = 24 dB/oct …)
    stages: usize,
    f: Vec<Vec<Biquad>>,
}

impl Filter {
    pub fn new(rate: f32, kind: FilterKind, freq: f32, q: f32, stages: usize) -> Self {
        Self { rate, kind, freq, q, stages: stages.clamp(1, 4), f: Vec::new() }
    }
    pub fn set(&mut self, kind: FilterKind, freq: f32, q: f32, stages: usize) {
        let st = stages.clamp(1, 4);
        if st != self.stages {
            self.f.clear();
        }
        self.kind = kind;
        self.freq = freq;
        self.q = q;
        self.stages = st;
        let c = Coefs::new(kind, self.rate, freq, q, 0.0);
        self.f.iter_mut().flatten().for_each(|b| b.c = c);
    }
}

impl Processor for Filter {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.f.len() < buf.len() {
            let c = Coefs::new(self.kind, self.rate, self.freq, self.q, 0.0);
            self.f.resize(buf.len(), vec![Biquad::new(c); self.stages]);
        }
        for (c, ch) in buf.iter_mut().enumerate() {
            for st in self.f[c].iter_mut() {
                for s in ch[..n].iter_mut() {
                    *s = st.tick(*s);
                }
            }
        }
    }
    fn reset(&mut self) {
        self.f.iter_mut().flatten().for_each(|b| b.reset());
    }
}
