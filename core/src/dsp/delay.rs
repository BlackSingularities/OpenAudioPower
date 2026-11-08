//! Stereo / ping-pong delay with filtered feedback.

use super::Processor;

pub struct Delay {
    rate: f32,
    lines: Vec<Vec<f32>>,
    pos: usize,
    time: usize,
    feedback: f32,
    mix: f32,
    pingpong: bool,
    lp: Vec<f32>,
    lp_coef: f32,
}

impl Delay {
    pub fn new(rate: f32) -> Self {
        let mut d = Self { rate, lines: Vec::new(), pos: 0, time: 1, feedback: 0.35, mix: 0.3, pingpong: false, lp: Vec::new(), lp_coef: 0.0 };
        d.set(350.0, 0.35, 0.3, false, 6000.0);
        d
    }
    pub fn set(&mut self, time_ms: f32, feedback: f32, mix: f32, pingpong: bool, tone_hz: f32) {
        self.time = ((time_ms.clamp(1.0, 2000.0) * 0.001 * self.rate) as usize).max(1);
        self.feedback = feedback.clamp(0.0, 0.95);
        self.mix = mix.clamp(0.0, 1.0);
        self.pingpong = pingpong;
        self.lp_coef = (-2.0 * std::f32::consts::PI * tone_hz.clamp(200.0, 20000.0) / self.rate).exp();
    }
}

impl Processor for Delay {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let cap = (self.rate * 2.1) as usize;
        if self.lines.len() != buf.len() {
            self.lines = vec![vec![0.0; cap]; buf.len()];
            self.lp = vec![0.0; buf.len()];
            self.pos = 0;
        }
        let k = buf.len();
        for i in 0..n {
            let rd = (self.pos + cap - self.time) % cap;
            let taps: Vec<f32> = (0..k).map(|c| self.lines[c][rd]).collect();
            for c in 0..k {
                let x = buf[c][i];
                let mut fb = taps[c];
                fb = fb + self.lp_coef * (self.lp[c] - fb);
                self.lp[c] = fb;
                let src = if self.pingpong && k == 2 {
                    // input enters the left line only; lines feed each other
                    let other = taps[1 - c];
                    (if c == 0 { (x + buf[1][i]) * 0.5 } else { 0.0 }) + other * self.feedback
                } else {
                    x + fb * self.feedback
                };
                self.lines[c][self.pos] = src;
                buf[c][i] = x + taps[c] * self.mix;
            }
            self.pos = (self.pos + 1) % cap;
        }
    }
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.iter_mut().for_each(|x| *x = 0.0));
    }
}
