//! Multitrack project model (shared 1:1 with the UI as JSON) and the mixer that renders it –
//! in real time for playback and offline for export.

use crate::buffer::{db_to_gain, AudioBuffer};
use crate::dsp::saturation::pan_gains;
use crate::effects::{Chain, EffectSlot};
use crate::ops::{fade_gain, FadeCurve};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: u64,
    pub source: u64,
    /// Timeline position in frames.
    pub start: i64,
    /// First frame of the source used.
    pub offset: i64,
    pub len: i64,
    #[serde(default)]
    pub gain: f32,
    #[serde(default)]
    pub fade_in: i64,
    #[serde(default)]
    pub fade_out: i64,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub gain: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub effects: Vec<EffectSlot>,
    #[serde(default)]
    pub clips: Vec<Clip>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Master {
    #[serde(default)]
    pub gain: f32,
    #[serde(default)]
    pub effects: Vec<EffectSlot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub master: Master,
}

impl Project {
    pub fn length(&self) -> i64 {
        self.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.start + c.len).max().unwrap_or(0)
    }
}

pub type Sources = HashMap<u64, Arc<AudioBuffer>>;

#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct TrackMeter {
    pub id: u64,
    pub l: f32,
    pub r: f32,
    pub gr: f32,
}

pub struct Mixer {
    pub rate: u32,
    project: Project,
    chains: HashMap<u64, Chain>,
    master_chain: Chain,
    tmp: Vec<Vec<f32>>,
    pub meters: Vec<TrackMeter>,
    pub master_gr: f32,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self { rate, project: Project::default(), chains: HashMap::new(), master_chain: Chain::new(rate as f32), tmp: vec![Vec::new(), Vec::new()], meters: Vec::new(), master_gr: 0.0 }
    }

    pub fn set_project(&mut self, p: Project) {
        let rate = self.rate as f32;
        self.chains.retain(|id, _| p.tracks.iter().any(|t| t.id == *id));
        for t in &p.tracks {
            self.chains.entry(t.id).or_insert_with(|| Chain::new(rate)).sync(&t.effects);
        }
        self.master_chain.sync(&p.master.effects);
        self.project = p;
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn reset(&mut self) {
        self.chains.values_mut().for_each(|c| c.reset());
        self.master_chain.reset();
    }

    /// Render `n` frames starting at timeline frame `pos` into `out` (2 channels, overwritten).
    pub fn render(&mut self, sources: &Sources, pos: i64, out: &mut [Vec<f32>], n: usize) {
        for c in out.iter_mut() {
            if c.len() < n {
                c.resize(n, 0.0);
            }
            c[..n].iter_mut().for_each(|x| *x = 0.0);
        }
        for t in self.tmp.iter_mut() {
            if t.len() < n {
                t.resize(n, 0.0);
            }
        }
        let any_solo = self.project.tracks.iter().any(|t| t.solo);
        self.meters.clear();
        let end = pos + n as i64;
        for track in &self.project.tracks {
            let audible = !track.mute && (!any_solo || track.solo);
            for t in self.tmp.iter_mut() {
                t[..n].iter_mut().for_each(|x| *x = 0.0);
            }
            if audible {
                for clip in &track.clips {
                    if clip.muted || clip.start >= end || clip.start + clip.len <= pos {
                        continue;
                    }
                    let Some(src) = sources.get(&clip.source) else { continue };
                    add_clip(&mut self.tmp, src, clip, pos, n);
                }
            }
            let chain = self.chains.entry(track.id).or_insert_with(|| Chain::new(self.rate as f32));
            if !chain.is_empty() {
                chain.process(&mut self.tmp, n);
            }
            let g = if audible { db_to_gain(track.gain) } else { 0.0 };
            let (pl, pr) = pan_gains(track.pan);
            let gl = (pl * std::f32::consts::SQRT_2).min(1.0) * g;
            let gr = (pr * std::f32::consts::SQRT_2).min(1.0) * g;
            let mut ml = 0.0f32;
            let mut mr = 0.0f32;
            for i in 0..n {
                let l = self.tmp[0][i] * gl;
                let r = self.tmp[1][i] * gr;
                ml = ml.max(l.abs());
                mr = mr.max(r.abs());
                out[0][i] += l;
                out[1][i] += r;
            }
            let red = chain.reductions().iter().map(|x| x.1).fold(0.0f32, f32::max);
            self.meters.push(TrackMeter { id: track.id, l: ml, r: mr, gr: red });
        }
        if !self.master_chain.is_empty() {
            self.master_chain.process(out, n);
        }
        self.master_gr = self.master_chain.reductions().iter().map(|x| x.1).fold(0.0f32, f32::max);
        let mg = db_to_gain(self.project.master.gain);
        if (mg - 1.0).abs() > 1e-6 {
            for c in out.iter_mut() {
                c[..n].iter_mut().for_each(|x| *x *= mg);
            }
        }
    }
}

fn add_clip(tmp: &mut [Vec<f32>], src: &AudioBuffer, clip: &Clip, pos: i64, n: usize) {
    let g = db_to_gain(clip.gain);
    let i0 = (clip.start - pos).max(0) as usize;
    let i1 = ((clip.start + clip.len - pos).min(n as i64)).max(0) as usize;
    let frames = src.frames() as i64;
    let mono = src.channels() == 1;
    for i in i0..i1 {
        let t = pos + i as i64 - clip.start; // position inside the clip
        let s = clip.offset + t;
        if s < 0 || s >= frames {
            continue;
        }
        let mut gg = g;
        if clip.fade_in > 0 && t < clip.fade_in {
            gg *= fade_gain(t as f32 / clip.fade_in as f32, FadeCurve::EqualPower);
        }
        let to_end = clip.len - t;
        if clip.fade_out > 0 && to_end < clip.fade_out {
            gg *= fade_gain(to_end as f32 / clip.fade_out as f32, FadeCurve::EqualPower);
        }
        let s = s as usize;
        if mono {
            let v = src.ch[0][s] * gg;
            tmp[0][i] += v;
            tmp[1][i] += v;
        } else {
            tmp[0][i] += src.ch[0][s] * gg;
            tmp[1][i] += src.ch[1][s] * gg;
        }
    }
}

/// Offline mixdown of `[start, end)` plus `tail` seconds (reverb/delay tails).
pub fn render_range(project: &Project, sources: &Sources, rate: u32, start: i64, end: i64, tail: f32, progress: &dyn Fn(f32), cancel: &crate::Cancel) -> AudioBuffer {
    let mut m = Mixer::new(rate);
    m.set_project(project.clone());
    let total = ((end - start).max(0) + (tail.max(0.0) * rate as f32) as i64) as usize;
    let mut out = AudioBuffer::new(rate, 2, total);
    let block = 4096;
    let mut tmp = vec![vec![0.0f32; block]; 2];
    let mut done = 0usize;
    while done < total {
        if cancel.is_set() {
            break;
        }
        let k = block.min(total - done);
        m.render(sources, start + done as i64, &mut tmp, k);
        for c in 0..2 {
            out.ch[c][done..done + k].copy_from_slice(&tmp[c][..k]);
        }
        done += k;
        if done % (block * 64) == 0 {
            progress(done as f32 / total as f32);
        }
    }
    out
}
