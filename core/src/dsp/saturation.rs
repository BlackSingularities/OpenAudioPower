//! Saturation / distortion, bit crusher and stereo tools (width, balance, gain).

use super::biquad::{Biquad, Coefs, FilterKind};
use super::Processor;
use crate::buffer::db_to_gain;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    Tape,
    Tube,
    Hard,
    Fuzz,
}

pub struct Saturator {
    rate: f32,
    drive: f32,
    curve: Curve,
    mix: f32,
    out: f32,
    tone: Vec<Biquad>,
    tone_hz: f32,
}

impl Saturator {
    pub fn new(rate: f32) -> Self {
        Self { rate, drive: 1.0, curve: Curve::Tape, mix: 1.0, out: 1.0, tone: Vec::new(), tone_hz: 12000.0 }
    }
    pub fn set(&mut self, drive_db: f32, curve: Curve, mix: f32, output_db: f32, tone_hz: f32) {
        self.drive = db_to_gain(drive_db.clamp(0.0, 48.0));
        self.curve = curve;
        self.mix = mix.clamp(0.0, 1.0);
        self.out = db_to_gain(output_db);
        if (tone_hz - self.tone_hz).abs() > 1.0 {
            self.tone.clear();
        }
        self.tone_hz = tone_hz;
    }
    #[inline]
    fn shape(&self, x: f32) -> f32 {
        let d = x * self.drive;
        match self.curve {
            Curve::Tape => d.tanh(),
            Curve::Tube => {
                // asymmetric – adds even harmonics
                if d >= 0.0 {
                    1.0 - (-d).exp()
                } else {
                    -(1.0 - (d * 1.25).exp()) / 1.25
                }
            }
            Curve::Hard => d.clamp(-1.0, 1.0),
            Curve::Fuzz => d.signum() * (1.0 - (-(d.abs() * 3.0)).exp()),
        }
    }
}

impl Processor for Saturator {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.tone.len() != buf.len() {
            self.tone = vec![Biquad::new(Coefs::new(FilterKind::LowPass, self.rate, self.tone_hz, 0.707, 0.0)); buf.len()];
        }
        let comp = 1.0 / self.drive.sqrt().max(1.0);
        for (c, ch) in buf.iter_mut().enumerate() {
            for s in ch[..n].iter_mut() {
                let sh = self.shape(*s);
                let w = self.tone[c].tick(sh) * comp;
                *s = (*s + (w - *s) * self.mix) * self.out;
            }
        }
    }
}

pub struct BitCrusher {
    bits: f32,
    hold: usize,
    count: usize,
    held: Vec<f32>,
    mix: f32,
}

impl BitCrusher {
    pub fn new() -> Self {
        Self { bits: 8.0, hold: 1, count: 0, held: Vec::new(), mix: 1.0 }
    }
    pub fn set(&mut self, bits: f32, downsample: usize, mix: f32) {
        self.bits = bits.clamp(1.0, 24.0);
        self.hold = downsample.clamp(1, 64);
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Default for BitCrusher {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for BitCrusher {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.held.len() != buf.len() {
            self.held = vec![0.0; buf.len()];
        }
        let q = 2f32.powf(self.bits - 1.0);
        for i in 0..n {
            if self.count == 0 {
                for (c, ch) in buf.iter().enumerate() {
                    self.held[c] = (ch[i] * q).round() / q;
                }
            }
            self.count = (self.count + 1) % self.hold;
            for (c, ch) in buf.iter_mut().enumerate() {
                ch[i] += (self.held[c] - ch[i]) * self.mix;
            }
        }
    }
}

/// Gain, balance, stereo width (mid/side), polarity, mono fold.
pub struct StereoTool {
    gain: f32,
    pan: f32,
    width: f32,
    invert_l: bool,
    invert_r: bool,
    mono: bool,
}

impl StereoTool {
    pub fn new() -> Self {
        Self { gain: 1.0, pan: 0.0, width: 1.0, invert_l: false, invert_r: false, mono: false }
    }
    pub fn set(&mut self, gain_db: f32, pan: f32, width: f32, invert_l: bool, invert_r: bool, mono: bool) {
        self.gain = db_to_gain(gain_db);
        self.pan = pan.clamp(-1.0, 1.0);
        self.width = width.clamp(0.0, 2.0);
        self.invert_l = invert_l;
        self.invert_r = invert_r;
        self.mono = mono;
    }
}

impl Default for StereoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for StereoTool {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let (gl, gr) = pan_gains(self.pan);
        if buf.len() < 2 {
            for s in buf[0][..n].iter_mut() {
                *s *= self.gain * if self.invert_l { -1.0 } else { 1.0 };
            }
            return;
        }
        let (a, b) = buf.split_at_mut(1);
        let (l, r) = (&mut a[0], &mut b[0]);
        for i in 0..n {
            let mut x = l[i] * if self.invert_l { -1.0 } else { 1.0 };
            let mut y = r[i] * if self.invert_r { -1.0 } else { 1.0 };
            let m = (x + y) * 0.5;
            let s = (x - y) * 0.5 * if self.mono { 0.0 } else { self.width };
            x = m + s;
            y = m - s;
            l[i] = x * gl * self.gain * std::f32::consts::SQRT_2;
            r[i] = y * gr * self.gain * std::f32::consts::SQRT_2;
        }
    }
}

/// Constant-power pan law (-3 dB centre).
#[inline]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (p.cos(), p.sin())
}
