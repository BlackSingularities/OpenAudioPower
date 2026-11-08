//! Planar floating-point audio buffer – the one sample container used everywhere.

use serde::Serialize;

#[derive(Clone, Debug, Default)]
pub struct AudioBuffer {
    pub rate: u32,
    /// One vector per channel, all of equal length.
    pub ch: Vec<Vec<f32>>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Meta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub codec: String,
    pub source_rate: u32,
    pub bits: u32,
}

impl AudioBuffer {
    pub fn new(rate: u32, channels: usize, frames: usize) -> Self {
        Self { rate, ch: vec![vec![0.0; frames]; channels.max(1)] }
    }
    pub fn from_channels(rate: u32, ch: Vec<Vec<f32>>) -> Self {
        let n = ch.iter().map(|c| c.len()).min().unwrap_or(0);
        let mut ch = ch;
        for c in ch.iter_mut() {
            c.truncate(n);
        }
        Self { rate, ch }
    }
    #[inline]
    pub fn frames(&self) -> usize {
        self.ch.first().map(|c| c.len()).unwrap_or(0)
    }
    #[inline]
    pub fn channels(&self) -> usize {
        self.ch.len()
    }
    pub fn duration(&self) -> f64 {
        if self.rate == 0 {
            0.0
        } else {
            self.frames() as f64 / self.rate as f64
        }
    }
    /// Copy of frames `[start, end)` (clamped).
    pub fn slice(&self, start: usize, end: usize) -> AudioBuffer {
        let n = self.frames();
        let s = start.min(n);
        let e = end.clamp(s, n);
        AudioBuffer { rate: self.rate, ch: self.ch.iter().map(|c| c[s..e].to_vec()).collect() }
    }
    /// Replace frames `[start, end)` with `part` (lengths may differ – the buffer grows or shrinks).
    pub fn splice(&self, start: usize, end: usize, part: &AudioBuffer) -> AudioBuffer {
        let n = self.frames();
        let s = start.min(n);
        let e = end.clamp(s, n);
        let chans = self.channels().max(part.channels());
        let mut out = Vec::with_capacity(chans);
        for c in 0..chans {
            let src = &self.ch[c.min(self.channels() - 1)];
            let p = &part.ch[c.min(part.channels().saturating_sub(1))];
            let mut v = Vec::with_capacity(n - (e - s) + p.len());
            v.extend_from_slice(&src[..s]);
            v.extend_from_slice(p);
            v.extend_from_slice(&src[e..]);
            out.push(v);
        }
        AudioBuffer { rate: self.rate, ch: out }
    }
    /// Average of all channels.
    pub fn mono(&self) -> Vec<f32> {
        let n = self.frames();
        let k = self.channels().max(1) as f32;
        let mut m = vec![0.0f32; n];
        for c in &self.ch {
            for (o, s) in m.iter_mut().zip(c) {
                *o += *s;
            }
        }
        for o in m.iter_mut() {
            *o /= k;
        }
        m
    }
    /// Always two channels (mono is duplicated, extra channels are folded down).
    pub fn to_stereo(&self) -> AudioBuffer {
        match self.channels() {
            0 => AudioBuffer::new(self.rate, 2, 0),
            1 => AudioBuffer { rate: self.rate, ch: vec![self.ch[0].clone(), self.ch[0].clone()] },
            2 => self.clone(),
            n => {
                let f = self.frames();
                let mut l = vec![0.0f32; f];
                let mut r = vec![0.0f32; f];
                for (i, c) in self.ch.iter().enumerate() {
                    let tgt = if i % 2 == 0 { &mut l } else { &mut r };
                    for j in 0..f {
                        tgt[j] += c[j];
                    }
                }
                let k = 2.0 / n as f32;
                l.iter_mut().for_each(|x| *x *= k);
                r.iter_mut().for_each(|x| *x *= k);
                AudioBuffer { rate: self.rate, ch: vec![l, r] }
            }
        }
    }
    pub fn peak(&self) -> f32 {
        self.ch.iter().flat_map(|c| c.iter()).fold(0.0f32, |m, s| m.max(s.abs()))
    }
    pub fn interleaved(&self) -> Vec<f32> {
        let n = self.frames();
        let k = self.channels();
        let mut v = Vec::with_capacity(n * k);
        for i in 0..n {
            for c in &self.ch {
                v.push(c[i]);
            }
        }
        v
    }
    pub fn from_interleaved(rate: u32, channels: usize, data: &[f32]) -> Self {
        let k = channels.max(1);
        let n = data.len() / k;
        let mut ch = vec![Vec::with_capacity(n); k];
        for i in 0..n {
            for (c, v) in ch.iter_mut().enumerate() {
                v.push(data[i * k + c]);
            }
        }
        Self { rate, ch }
    }
}

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -120.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}
#[inline]
pub fn gain_to_db(g: f32) -> f32 {
    if g <= 1e-6 {
        -120.0
    } else {
        20.0 * g.log10()
    }
}
