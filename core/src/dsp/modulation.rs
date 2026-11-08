//! Modulation effects: chorus, flanger, phaser, tremolo, auto-pan.

use super::Processor;
use std::f32::consts::TAU;

/// Chorus / flanger share a modulated fractional delay line.
pub struct ModDelay {
    rate: f32,
    lines: Vec<Vec<f32>>,
    pos: usize,
    phase: f32,
    rate_hz: f32,
    base_ms: f32,
    depth_ms: f32,
    feedback: f32,
    mix: f32,
    last: Vec<f32>,
}

impl ModDelay {
    pub fn new(rate: f32) -> Self {
        Self { rate, lines: Vec::new(), pos: 0, phase: 0.0, rate_hz: 0.8, base_ms: 15.0, depth_ms: 5.0, feedback: 0.0, mix: 0.5, last: Vec::new() }
    }
    pub fn set(&mut self, rate_hz: f32, base_ms: f32, depth_ms: f32, feedback: f32, mix: f32) {
        self.rate_hz = rate_hz.clamp(0.01, 20.0);
        self.base_ms = base_ms.clamp(0.1, 50.0);
        self.depth_ms = depth_ms.clamp(0.0, 20.0);
        self.feedback = feedback.clamp(-0.95, 0.95);
        self.mix = mix.clamp(0.0, 1.0);
    }
}

impl Processor for ModDelay {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let cap = (self.rate * 0.08) as usize + 4;
        if self.lines.len() != buf.len() {
            self.lines = vec![vec![0.0; cap]; buf.len()];
            self.last = vec![0.0; buf.len()];
        }
        let inc = self.rate_hz / self.rate;
        for i in 0..n {
            for (c, ch) in buf.iter_mut().enumerate() {
                // channels get a quarter-period offset for stereo movement
                let ph = self.phase + c as f32 * 0.25;
                let lfo = (ph * TAU).sin() * 0.5 + 0.5;
                let d = (self.base_ms + self.depth_ms * lfo) * 0.001 * self.rate;
                let rp = self.pos as f32 - d;
                let rp = if rp < 0.0 { rp + cap as f32 } else { rp };
                let j = rp.floor() as usize % cap;
                let f = rp - rp.floor();
                let a = self.lines[c][j];
                let b = self.lines[c][(j + 1) % cap];
                let y = a + (b - a) * f;
                let x = ch[i];
                self.lines[c][self.pos] = x + y * self.feedback;
                self.last[c] = y;
                ch[i] = x * (1.0 - self.mix * 0.5) + y * self.mix;
            }
            self.pos = (self.pos + 1) % cap;
            self.phase = (self.phase + inc).fract();
        }
    }
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.iter_mut().for_each(|x| *x = 0.0));
    }
}

/// Classic multi-stage all-pass phaser.
pub struct Phaser {
    rate: f32,
    stages: usize,
    z: Vec<Vec<f32>>,
    fb: Vec<f32>,
    phase: f32,
    rate_hz: f32,
    min_hz: f32,
    max_hz: f32,
    feedback: f32,
    mix: f32,
}

impl Phaser {
    pub fn new(rate: f32) -> Self {
        Self { rate, stages: 6, z: Vec::new(), fb: Vec::new(), phase: 0.0, rate_hz: 0.4, min_hz: 300.0, max_hz: 3000.0, feedback: 0.5, mix: 0.5 }
    }
    pub fn set(&mut self, rate_hz: f32, min_hz: f32, max_hz: f32, feedback: f32, mix: f32, stages: usize) {
        self.rate_hz = rate_hz.clamp(0.01, 10.0);
        self.min_hz = min_hz.clamp(20.0, 10000.0);
        self.max_hz = max_hz.clamp(self.min_hz + 10.0, 18000.0);
        self.feedback = feedback.clamp(-0.95, 0.95);
        self.mix = mix.clamp(0.0, 1.0);
        let s = stages.clamp(2, 12);
        if s != self.stages {
            self.stages = s;
            self.z.clear();
        }
    }
}

impl Processor for Phaser {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        if self.z.len() != buf.len() {
            self.z = vec![vec![0.0; self.stages]; buf.len()];
            self.fb = vec![0.0; buf.len()];
        }
        let inc = self.rate_hz / self.rate;
        for i in 0..n {
            for (c, ch) in buf.iter_mut().enumerate() {
                let lfo = ((self.phase + c as f32 * 0.25) * TAU).sin() * 0.5 + 0.5;
                let f = self.min_hz * (self.max_hz / self.min_hz).powf(lfo);
                let t = (std::f32::consts::PI * f / self.rate).tan();
                let a = (t - 1.0) / (t + 1.0);
                let x = ch[i];
                let mut y = x + self.fb[c] * self.feedback;
                for z in self.z[c].iter_mut() {
                    let o = a * y + *z;
                    *z = y - a * o;
                    y = o;
                }
                self.fb[c] = y;
                ch[i] = x * (1.0 - self.mix * 0.5) + y * self.mix * 0.5;
            }
            self.phase = (self.phase + inc).fract();
        }
    }
    fn reset(&mut self) {
        self.z.iter_mut().flatten().for_each(|x| *x = 0.0);
    }
}

/// Tremolo (amplitude) and auto-pan (opposite phase per channel).
pub struct Tremolo {
    rate: f32,
    phase: f32,
    rate_hz: f32,
    depth: f32,
    pan: bool,
}

impl Tremolo {
    pub fn new(rate: f32) -> Self {
        Self { rate, phase: 0.0, rate_hz: 5.0, depth: 0.5, pan: false }
    }
    pub fn set(&mut self, rate_hz: f32, depth: f32, pan: bool) {
        self.rate_hz = rate_hz.clamp(0.05, 30.0);
        self.depth = depth.clamp(0.0, 1.0);
        self.pan = pan;
    }
}

impl Processor for Tremolo {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let inc = self.rate_hz / self.rate;
        for i in 0..n {
            let s = (self.phase * TAU).sin();
            for (c, ch) in buf.iter_mut().enumerate() {
                let m = if self.pan && c % 2 == 1 { -s } else { s };
                let g = 1.0 - self.depth * (0.5 - 0.5 * m);
                ch[i] *= g;
            }
            self.phase = (self.phase + inc).fract();
        }
    }
}
