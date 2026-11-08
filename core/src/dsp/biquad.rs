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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EqBand {
    pub kind: FilterKind,
    pub freq: f32,
    pub q: f32,
    pub gain: f32,
    #[serde(default = "on")]
    pub on: bool,
}
fn on() -> bool {
    true
}

/// Multi-band parametric EQ (any number of bands, any channel count).
pub struct Equalizer {
    rate: f32,
    bands: Vec<EqBand>,
    filters: Vec<Vec<Biquad>>, // [band][channel]
}

impl Equalizer {
    pub fn new(rate: f32, bands: &[EqBand]) -> Self {
        let mut e = Self { rate, bands: Vec::new(), filters: Vec::new() };
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

impl Processor for Equalizer {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        self.ensure(buf.len());
        for (bi, band) in self.bands.iter().enumerate() {
            if !band.on {
                continue;
            }
            for (c, ch) in buf.iter_mut().enumerate() {
                let f = &mut self.filters[bi][c];
                for s in ch[..n].iter_mut() {
                    *s = f.tick(*s);
                }
            }
        }
    }
    fn reset(&mut self) {
        self.filters.iter_mut().flatten().for_each(|f| f.reset());
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
