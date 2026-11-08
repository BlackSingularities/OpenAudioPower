//! Stereo algorithmic reverb (Freeverb topology with pre-delay, damping and width).

use super::Processor;

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}
impl Comb {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(1)], pos: 0, store: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

struct AllPass {
    buf: Vec<f32>,
    pos: usize,
}
impl AllPass {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(1)], pos: 0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        let y = -x + b;
        self.buf[self.pos] = x + b * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

pub struct Reverb {
    rate: f32,
    combs: [Vec<Comb>; 2],
    aps: [Vec<AllPass>; 2],
    pre: Vec<f32>,
    pre_pos: usize,
    pre_len: usize,
    feedback: f32,
    damp: f32,
    wet: f32,
    dry: f32,
    width: f32,
    lowcut: [f32; 2],
    lc_coef: f32,
}

impl Reverb {
    pub fn new(rate: f32) -> Self {
        let s = rate / 44100.0;
        let mk = |off: usize| -> (Vec<Comb>, Vec<AllPass>) {
            (
                COMBS.iter().map(|&n| Comb::new(((n + off) as f32 * s) as usize)).collect(),
                ALLPASS.iter().map(|&n| AllPass::new(((n + off) as f32 * s) as usize)).collect(),
            )
        };
        let (cl, al) = mk(0);
        let (cr, ar) = mk(SPREAD);
        let mut r = Self {
            rate,
            combs: [cl, cr],
            aps: [al, ar],
            pre: vec![0.0; (rate * 0.25) as usize + 1],
            pre_pos: 0,
            pre_len: 1,
            feedback: 0.84,
            damp: 0.2,
            wet: 0.3,
            dry: 1.0,
            width: 1.0,
            lowcut: [0.0; 2],
            lc_coef: 0.0,
        };
        r.set(0.7, 0.4, 0.3, 1.0, 1.0, 20.0, 200.0);
        r
    }
    /// `room` 0..1, `damping` 0..1, `wet`/`dry` 0..1, `width` 0..1, `predelay_ms`, `lowcut_hz` on the wet signal.
    pub fn set(&mut self, room: f32, damping: f32, wet: f32, dry: f32, width: f32, predelay_ms: f32, lowcut_hz: f32) {
        self.feedback = 0.7 + room.clamp(0.0, 1.0) * 0.28;
        self.damp = damping.clamp(0.0, 1.0) * 0.4;
        self.wet = wet.clamp(0.0, 1.0) * 3.0;
        self.dry = dry.clamp(0.0, 1.0);
        self.width = width.clamp(0.0, 1.0);
        self.pre_len = ((predelay_ms.clamp(0.0, 240.0) * 0.001 * self.rate) as usize).max(1).min(self.pre.len() - 1);
        self.lc_coef = (-2.0 * std::f32::consts::PI * lowcut_hz.max(10.0) / self.rate).exp();
    }
}

impl Processor for Reverb {
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        let stereo = buf.len() >= 2;
        let wet1 = self.wet * (self.width / 2.0 + 0.5);
        let wet2 = self.wet * ((1.0 - self.width) / 2.0);
        for i in 0..n {
            let l = buf[0][i];
            let r = if stereo { buf[1][i] } else { l };
            let input = (l + r) * 0.015;
            // pre-delay line
            let rd = (self.pre_pos + self.pre.len() - self.pre_len) % self.pre.len();
            let x = self.pre[rd];
            self.pre[self.pre_pos] = input;
            self.pre_pos = (self.pre_pos + 1) % self.pre.len();
            let mut out = [0.0f32; 2];
            for s in 0..2 {
                let mut acc = 0.0;
                for c in self.combs[s].iter_mut() {
                    acc += c.tick(x, self.feedback, self.damp);
                }
                for a in self.aps[s].iter_mut() {
                    acc = a.tick(acc);
                }
                // one-pole high-pass on the tail keeps the low end clean
                let lp = acc + self.lc_coef * (self.lowcut[s] - acc);
                self.lowcut[s] = lp;
                out[s] = acc - lp;
            }
            let ol = out[0] * wet1 + out[1] * wet2;
            let or = out[1] * wet1 + out[0] * wet2;
            if stereo {
                buf[0][i] = l * self.dry + ol;
                buf[1][i] = r * self.dry + or;
            } else {
                buf[0][i] = l * self.dry + (ol + or) * 0.5;
            }
        }
    }
    fn reset(&mut self) {
        for s in 0..2 {
            for c in self.combs[s].iter_mut() {
                c.buf.iter_mut().for_each(|x| *x = 0.0);
                c.store = 0.0;
            }
            for a in self.aps[s].iter_mut() {
                a.buf.iter_mut().for_each(|x| *x = 0.0);
            }
        }
        self.pre.iter_mut().for_each(|x| *x = 0.0);
    }
}
