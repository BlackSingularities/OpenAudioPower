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

/// One breakpoint of an automation envelope: timeline frame and value.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct AutoPoint {
    pub t: i64,
    pub v: f32,
}

/// Per-track automation. Points are kept sorted by time; values are linearly interpolated and
/// held before the first and after the last point.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    /// Gain in dB added to the track fader.
    #[serde(default)]
    pub volume: Vec<AutoPoint>,
    /// Pan position −1…1; when present it replaces the pan knob.
    #[serde(default)]
    pub pan: Vec<AutoPoint>,
}

/// Envelope value at frame `t` (`None` when the envelope is empty).
pub fn envelope_at(points: &[AutoPoint], t: i64) -> Option<f32> {
    let first = points.first()?;
    if t <= first.t {
        return Some(first.v);
    }
    let last = points.last()?;
    if t >= last.t {
        return Some(last.v);
    }
    let i = points.partition_point(|p| p.t <= t);
    let (a, b) = (points[i - 1], points[i]);
    let span = (b.t - a.t).max(1) as f32;
    Some(a.v + (b.v - a.v) * ((t - a.t) as f32 / span))
}

/// Fills `out[..n]` with the envelope from frame `pos`; returns false for an empty envelope.
fn envelope_block(points: &[AutoPoint], pos: i64, out: &mut [f32], n: usize) -> bool {
    if points.is_empty() {
        return false;
    }
    if points.len() == 1 {
        out[..n].iter_mut().for_each(|x| *x = points[0].v);
        return true;
    }
    let mut i = points.partition_point(|p| p.t <= pos);
    for (k, o) in out[..n].iter_mut().enumerate() {
        let t = pos + k as i64;
        while i < points.len() && points[i].t <= t {
            i += 1;
        }
        *o = if i == 0 {
            points[0].v
        } else if i >= points.len() {
            points[points.len() - 1].v
        } else {
            let (a, b) = (points[i - 1], points[i]);
            a.v + (b.v - a.v) * ((t - a.t) as f32 / (b.t - a.t).max(1) as f32)
        };
    }
    true
}

/// A send from a track to a bus (aux). `gain` in dB; `pre` taps before the fader.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Send {
    pub id: u64,
    pub target: u64,
    #[serde(default)]
    pub gain: f32,
    #[serde(default)]
    pub pre: bool,
    #[serde(default = "yes")]
    pub on: bool,
}
fn yes() -> bool {
    true
}

/// A frozen track plays this pre-rendered source (clips + effects) instead of its clips and chain.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Freeze {
    pub source: u64,
    /// Timeline frame of the source's first frame.
    #[serde(default)]
    pub start: i64,
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
    #[serde(default)]
    pub auto: Automation,
    /// A bus has no clips; it sums what other tracks route or send to it.
    #[serde(default)]
    pub bus: bool,
    /// Bus id this track feeds instead of the master (0 = master).
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub sends: Vec<Send>,
    #[serde(default)]
    pub freeze: Option<Freeze>,
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
        let clips = self.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.start + c.len).max().unwrap_or(0);
        clips
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

/// Fixed delay for plug-in delay compensation on one routing edge.
struct DelayLine {
    buf: [Vec<f32>; 2],
    w: usize,
}

impl DelayLine {
    fn new(d: usize) -> Self {
        Self { buf: [vec![0.0; d], vec![0.0; d]], w: 0 }
    }
    /// Adds `src * g`, delayed, into `dst`.
    fn run(&mut self, src: &[Vec<f32>], dst: &mut [Vec<f32>], g: f32, n: usize) {
        let d = self.buf[0].len();
        if d == 0 {
            for c in 0..2 {
                for i in 0..n {
                    dst[c][i] += src[c][i] * g;
                }
            }
            return;
        }
        let w0 = self.w;
        for c in 0..2 {
            let mut w = w0;
            let b = &mut self.buf[c];
            for i in 0..n {
                let out = b[w];
                b[w] = src[c][i] * g;
                dst[c][i] += out;
                w += 1;
                if w == d {
                    w = 0;
                }
            }
        }
        self.w = (w0 + n) % d;
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|b| b.fill(0.0));
    }
}

/// Routing graph worked out whenever the project changes.
#[derive(Default)]
struct Plan {
    /// Track indices in processing order (sources before the buses and keyed tracks they feed).
    order: Vec<usize>,
    /// Effective output per track index (0 = master).
    target: Vec<u64>,
    audible: Vec<bool>,
    /// Tracks used as sidechain keys (their post-effects signal is kept for one block).
    keyed: Vec<u64>,
    /// Chain latency per track index.
    lat: Vec<usize>,
    /// Latency from a bus input to the master (D in the notes below).
    down: HashMap<u64, usize>,
    /// Total latency compensated: tracks read the timeline this far ahead.
    total: usize,
}

pub struct Mixer {
    pub rate: u32,
    project: Project,
    chains: HashMap<u64, Chain>,
    master_chain: Chain,
    tmp: Vec<Vec<f32>>,
    post: Vec<Vec<f32>>,
    env: Vec<Vec<f32>>,
    plan: Plan,
    bus_in: HashMap<u64, Vec<Vec<f32>>>,
    keys: HashMap<u64, Vec<Vec<f32>>>,
    /// (from track, send id or 0 for the main output) → delay line.
    delays: HashMap<(u64, u64), DelayLine>,
    /// Automatic crossfade lengths (in, out) for overlapping clips.
    fades: HashMap<u64, (i64, i64)>,
    /// Tracks already rendered in the current block (kept to avoid allocating per block).
    processed: Vec<u64>,
    pub meters: Vec<TrackMeter>,
    pub master_gr: f32,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            project: Project::default(),
            chains: HashMap::new(),
            master_chain: Chain::new(rate as f32),
            tmp: vec![Vec::new(), Vec::new()],
            post: vec![Vec::new(), Vec::new()],
            env: vec![Vec::new(), Vec::new()],
            plan: Plan::default(),
            bus_in: HashMap::new(),
            keys: HashMap::new(),
            delays: HashMap::new(),
            fades: HashMap::new(),
            processed: Vec::new(),
            meters: Vec::new(),
            master_gr: 0.0,
        }
    }

    pub fn set_project(&mut self, p: Project) {
        let rate = self.rate as f32;
        self.chains.retain(|id, _| p.tracks.iter().any(|t| t.id == *id));
        for t in &p.tracks {
            self.chains.entry(t.id).or_insert_with(|| Chain::new(rate)).sync(&t.effects);
        }
        self.master_chain.sync(&p.master.effects);
        self.fades = crossfades(&p);
        self.project = p;
        self.replan();
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Frames by which the output trails the tracks' own processing (compensated internally).
    pub fn latency(&self) -> usize {
        self.plan.total
    }

    pub fn reset(&mut self) {
        self.chains.values_mut().for_each(|c| c.reset());
        self.master_chain.reset();
        self.delays.values_mut().for_each(|d| d.clear());
    }

    fn replan(&mut self) {
        let tracks = &self.project.tracks;
        let idx: HashMap<u64, usize> = tracks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
        let is_bus = |id: u64| idx.get(&id).map(|&i| tracks[i].bus).unwrap_or(false);

        // outputs: only to an existing bus other than itself, and never in a loop
        let mut target: Vec<u64> = tracks.iter().map(|t| if t.output != t.id && is_bus(t.output) { t.output } else { 0 }).collect();
        for i in 0..tracks.len() {
            let mut seen = vec![i];
            let mut cur = target[i];
            while cur != 0 {
                let j = idx[&cur];
                if seen.contains(&j) {
                    target[i] = 0;
                    break;
                }
                seen.push(j);
                cur = target[j];
            }
        }
        // edges a → b meaning "a must be processed before b"
        let mut edges: Vec<(usize, usize)> = Vec::new();
        let mut route: Vec<(usize, usize)> = Vec::new(); // audio flow only (for solo)
        for (i, t) in tracks.iter().enumerate() {
            if target[i] != 0 {
                edges.push((i, idx[&target[i]]));
                route.push((i, idx[&target[i]]));
            }
            for s in t.sends.iter().filter(|s| s.on && s.target != t.id && is_bus(s.target)) {
                edges.push((i, idx[&s.target]));
                route.push((i, idx[&s.target]));
            }
            if let Some(ch) = self.chains.get(&t.id) {
                for k in ch.keys() {
                    if let Some(&j) = idx.get(&k) {
                        if j != i {
                            edges.push((j, i));
                        }
                    }
                }
            }
        }
        // Kahn; whatever is left in a cycle goes last (its back edges just see silence)
        let n = tracks.len();
        let mut indeg = vec![0usize; n];
        for &(_, b) in &edges {
            indeg[b] += 1;
        }
        let mut order = Vec::with_capacity(n);
        let mut done = vec![false; n];
        loop {
            let Some(i) = (0..n).find(|&i| !done[i] && indeg[i] == 0) else { break };
            done[i] = true;
            order.push(i);
            for &(a, b) in &edges {
                if a == i {
                    indeg[b] -= 1;
                }
            }
        }
        order.extend((0..n).filter(|&i| !done[i]));

        // solo follows the signal: soloing a track keeps the buses it feeds, soloing a bus keeps its sources
        let any_solo = tracks.iter().any(|t| t.solo);
        let mut keep: Vec<bool> = tracks.iter().map(|t| t.solo).collect();
        if any_solo {
            let spread = |keep: &mut Vec<bool>, fwd: bool| {
                let mut changed = true;
                while changed {
                    changed = false;
                    for &(a, b) in &route {
                        let (from, to) = if fwd { (a, b) } else { (b, a) };
                        if keep[from] && !keep[to] {
                            keep[to] = true;
                            changed = true;
                        }
                    }
                }
            };
            let solo: Vec<bool> = keep.clone();
            let mut down = solo.clone();
            spread(&mut down, true);
            let mut up = solo;
            spread(&mut up, false);
            keep = down.iter().zip(&up).map(|(a, b)| *a || *b).collect();
        }
        let audible: Vec<bool> = tracks.iter().enumerate().map(|(i, t)| !t.mute && (!any_solo || keep[i])).collect();

        let mut keyed: Vec<u64> = self.chains.values().flat_map(|c| c.keys()).filter(|k| idx.contains_key(k)).collect();
        keyed.sort_unstable();
        keyed.dedup();

        // Plug-in delay compensation. D(bus) = latency from the bus input to the master.
        // Audio tracks read the timeline `total` frames ahead; every routing edge gets the delay
        // that makes all paths arrive at the master together.
        let lat: Vec<usize> = tracks
            .iter()
            .map(|t| if t.freeze.is_some() { 0 } else { self.chains.get_mut(&t.id).map(|c| c.latency()).unwrap_or(0) })
            .collect();
        let mut down: HashMap<u64, usize> = HashMap::new();
        fn d_of(id: u64, idx: &HashMap<u64, usize>, lat: &[usize], target: &[u64], memo: &mut HashMap<u64, usize>) -> usize {
            if id == 0 {
                return 0;
            }
            if let Some(&v) = memo.get(&id) {
                return v;
            }
            let i = idx[&id];
            let v = lat[i] + d_of(target[i], idx, lat, target, memo);
            memo.insert(id, v);
            v
        }
        for t in tracks.iter().filter(|t| t.bus) {
            d_of(t.id, &idx, &lat, &target, &mut down);
        }
        let d = |id: u64| if id == 0 { 0 } else { down.get(&id).copied().unwrap_or(0) };
        let mut total = 0usize;
        for (i, t) in tracks.iter().enumerate().filter(|(_, t)| !t.bus) {
            total = total.max(lat[i] + d(target[i]));
            for s in t.sends.iter().filter(|s| s.on && is_bus(s.target)) {
                total = total.max(lat[i] + d(s.target));
            }
        }
        let mut delays = HashMap::new();
        for (i, t) in tracks.iter().enumerate() {
            let edge = |to: u64| -> usize {
                if t.bus {
                    // bus → its own output needs nothing; a bus send lines up with the bus's main path
                    if to == target[i] { 0 } else { d(target[i]).saturating_sub(d(to)) }
                } else {
                    total - lat[i] - d(to)
                }
            };
            let main = edge(target[i]);
            delays.insert((t.id, 0), self.delays.remove(&(t.id, 0)).filter(|x| x.buf[0].len() == main).unwrap_or_else(|| DelayLine::new(main)));
            for s in t.sends.iter().filter(|s| s.on && s.target != t.id && is_bus(s.target)) {
                let k = edge(s.target);
                delays.insert((t.id, s.id), self.delays.remove(&(t.id, s.id)).filter(|x| x.buf[0].len() == k).unwrap_or_else(|| DelayLine::new(k)));
            }
        }
        self.delays = delays;
        self.bus_in.retain(|id, _| is_bus(*id));
        for t in tracks.iter().filter(|t| t.bus) {
            self.bus_in.entry(t.id).or_insert_with(|| vec![Vec::new(), Vec::new()]);
        }
        self.keys.retain(|id, _| keyed.contains(id));
        for k in &keyed {
            self.keys.entry(*k).or_insert_with(|| vec![Vec::new(), Vec::new()]);
        }
        self.plan = Plan { order, target, audible, keyed, lat, down, total };
    }

    /// Render `n` frames starting at timeline frame `pos` into `out` (2 channels, overwritten).
    pub fn render(&mut self, sources: &Sources, pos: i64, out: &mut [Vec<f32>], n: usize) {
        let Mixer { rate, project, chains, master_chain, tmp, post, env, plan, bus_in, keys, delays, fades, processed, meters, master_gr } = self;
        for c in out.iter_mut() {
            if c.len() < n {
                c.resize(n, 0.0);
            }
            c[..n].fill(0.0);
        }
        for t in tmp.iter_mut().chain(post.iter_mut()).chain(env.iter_mut()).chain(bus_in.values_mut().flatten()).chain(keys.values_mut().flatten()) {
            if t.len() < n {
                t.resize(n, 0.0);
            }
        }
        bus_in.values_mut().flatten().for_each(|b| b[..n].fill(0.0));
        meters.clear();
        let ahead = pos + plan.total as i64;
        let end = ahead + n as i64;
        processed.clear();
        processed.reserve(plan.order.len());
        for &ti in &plan.order {
            let track = &project.tracks[ti];
            let audible = plan.audible[ti];
            let is_key = plan.keyed.contains(&track.id);
            for t in tmp.iter_mut() {
                t[..n].fill(0.0);
            }
            if track.bus {
                if let Some(b) = bus_in.get(&track.id) {
                    for c in 0..2 {
                        tmp[c][..n].copy_from_slice(&b[c][..n]);
                    }
                }
            } else if let Some(fz) = &track.freeze {
                if audible || is_key {
                    if let Some(src) = sources.get(&fz.source) {
                        add_source(tmp, src, ahead - fz.start, n);
                    }
                }
            } else if audible || is_key {
                for clip in &track.clips {
                    if clip.muted || clip.start >= end || clip.start + clip.len <= ahead {
                        continue;
                    }
                    let Some(src) = sources.get(&clip.source) else { continue };
                    let (fi, fo) = fades.get(&clip.id).copied().unwrap_or((0, 0));
                    add_clip(tmp, src, clip, ahead, n, fi, fo);
                }
            }
            let chain = chains.entry(track.id).or_insert_with(|| Chain::new(*rate as f32));
            if track.freeze.is_none() && !chain.is_empty() {
                chain.process_keyed(tmp, n, &|k| keys.get(&k).filter(|_| processed.contains(&k)).map(|v| v.as_slice()));
            }
            if is_key {
                let k = keys.get_mut(&track.id).unwrap();
                for c in 0..2 {
                    k[c][..n].copy_from_slice(&tmp[c][..n]);
                }
            }
            // automation time: what the fader hears now left the timeline this long ago
            let t_auto = if track.bus { pos + plan.down.get(&plan.target[ti]).copied().unwrap_or(0) as i64 } else { ahead - plan.lat[ti] as i64 };
            let g = if audible { db_to_gain(track.gain) } else { 0.0 };
            let (pl, pr) = pan_gains(track.pan);
            let gl = (pl * std::f32::consts::SQRT_2).min(1.0) * g;
            let gr = (pr * std::f32::consts::SQRT_2).min(1.0) * g;
            let (ve, pe) = env.split_at_mut(1);
            let vol_auto = audible && envelope_block(&track.auto.volume, t_auto, &mut ve[0], n);
            let pan_auto = audible && envelope_block(&track.auto.pan, t_auto, &mut pe[0], n);
            let mut ml = 0.0f32;
            let mut mr = 0.0f32;
            for i in 0..n {
                let (mut l, mut r) = if pan_auto {
                    let (a, b) = pan_gains(pe[0][i].clamp(-1.0, 1.0));
                    (tmp[0][i] * (a * std::f32::consts::SQRT_2).min(1.0) * g, tmp[1][i] * (b * std::f32::consts::SQRT_2).min(1.0) * g)
                } else {
                    (tmp[0][i] * gl, tmp[1][i] * gr)
                };
                if vol_auto {
                    let k = db_to_gain(ve[0][i]);
                    l *= k;
                    r *= k;
                }
                ml = ml.max(l.abs());
                mr = mr.max(r.abs());
                post[0][i] = l;
                post[1][i] = r;
            }
            for s in track.sends.iter().filter(|s| s.on && s.target != track.id) {
                // a bus already processed (feedback loop) can no longer take input this block
                if processed.contains(&s.target) {
                    continue;
                }
                let (Some(dl), Some(dst)) = (delays.get_mut(&(track.id, s.id)), bus_in.get_mut(&s.target)) else { continue };
                let src: &[Vec<f32>] = if s.pre { &tmp[..] } else { &post[..] };
                let sg = if audible { db_to_gain(s.gain) } else { 0.0 };
                dl.run(src, dst, sg, n);
            }
            let tgt = plan.target[ti];
            if let Some(dl) = delays.get_mut(&(track.id, 0)) {
                match bus_in.get_mut(&tgt) {
                    Some(dst) if tgt != 0 => dl.run(post, dst, 1.0, n),
                    _ => dl.run(post, out, 1.0, n),
                }
            }
            let red = chain.reductions().iter().map(|x| x.1).fold(0.0f32, f32::max);
            meters.push(TrackMeter { id: track.id, l: ml, r: mr, gr: red });
            processed.push(track.id);
        }
        if !master_chain.is_empty() {
            master_chain.process(out, n);
        }
        *master_gr = master_chain.reductions().iter().map(|x| x.1).fold(0.0f32, f32::max);
        let mg = db_to_gain(project.master.gain);
        if (mg - 1.0).abs() > 1e-6 {
            for c in out.iter_mut() {
                c[..n].iter_mut().for_each(|x| *x *= mg);
            }
        }
    }
}

/// Overlapping clips on one track crossfade over the overlap (equal power), on top of their own fades.
fn crossfades(p: &Project) -> HashMap<u64, (i64, i64)> {
    let mut m: HashMap<u64, (i64, i64)> = HashMap::new();
    for t in &p.tracks {
        let mut cl: Vec<&Clip> = t.clips.iter().filter(|c| !c.muted).collect();
        cl.sort_by_key(|c| c.start);
        for w in 0..cl.len() {
            let a = cl[w];
            let a_end = a.start + a.len;
            // the next clip starting inside `a` and ending after it
            if let Some(b) = cl[w + 1..].iter().find(|b| b.start < a_end && b.start + b.len > a_end) {
                let ov = (a_end - b.start).min(a.len).min(b.len);
                if ov > 0 {
                    let ea = m.entry(a.id).or_insert((a.fade_in, a.fade_out));
                    ea.1 = ea.1.max(ov);
                    let eb = m.entry(b.id).or_insert((b.fade_in, b.fade_out));
                    eb.0 = eb.0.max(ov);
                }
            }
        }
    }
    m
}

/// Adds a whole source placed so that `t0` (timeline frame of out[0]) maps to source frame `t0`.
fn add_source(tmp: &mut [Vec<f32>], src: &AudioBuffer, t0: i64, n: usize) {
    let frames = src.frames() as i64;
    let mono = src.channels() == 1;
    for i in 0..n {
        let s = t0 + i as i64;
        if s < 0 || s >= frames {
            continue;
        }
        let s = s as usize;
        tmp[0][i] += src.ch[0][s];
        tmp[1][i] += src.ch[if mono { 0 } else { 1 }][s];
    }
}

fn add_clip(tmp: &mut [Vec<f32>], src: &AudioBuffer, clip: &Clip, pos: i64, n: usize, fade_in: i64, fade_out: i64) {
    let g = db_to_gain(clip.gain);
    let fade_in = fade_in.max(clip.fade_in);
    let fade_out = fade_out.max(clip.fade_out);
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
        if fade_in > 0 && t < fade_in {
            gg *= fade_gain(t as f32 / fade_in as f32, FadeCurve::EqualPower);
        }
        let to_end = clip.len - t;
        if fade_out > 0 && to_end < fade_out {
            gg *= fade_gain(to_end as f32 / fade_out as f32, FadeCurve::EqualPower);
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
    // pre-roll fills the compensation delays so the range starts complete
    let mut pre = m.latency();
    let mut at = start - pre as i64;
    while pre > 0 {
        let k = block.min(pre);
        m.render(sources, at, &mut tmp, k);
        at += k as i64;
        pre -= k;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_interpolates_and_holds() {
        let p = [AutoPoint { t: 100, v: 0.0 }, AutoPoint { t: 200, v: -10.0 }];
        assert_eq!(envelope_at(&p, 0), Some(0.0));
        assert_eq!(envelope_at(&p, 150), Some(-5.0));
        assert_eq!(envelope_at(&p, 500), Some(-10.0));
        let mut out = vec![0.0; 300];
        assert!(envelope_block(&p, 0, &mut out, 300));
        assert_eq!(out[50], 0.0);
        assert!((out[150] + 5.0).abs() < 1e-4);
        assert_eq!(out[299], -10.0);
        assert!(!envelope_block(&[], 0, &mut out, 10));
    }

    /// A −∞-ish volume envelope silences the track; a hard-left pan envelope empties the right side.
    #[test]
    fn automation_reaches_the_mix() {
        let rate = 1000;
        let src = Arc::new(AudioBuffer::from_channels(rate, vec![vec![0.5; 1000], vec![0.5; 1000]]));
        let mut sources = Sources::new();
        sources.insert(1, src);
        let clip = Clip { id: 1, source: 1, start: 0, offset: 0, len: 1000, ..Default::default() };
        let mut tr = Track { id: 1, clips: vec![clip], ..Default::default() };
        tr.auto.volume = vec![AutoPoint { t: 0, v: 0.0 }, AutoPoint { t: 500, v: 0.0 }, AutoPoint { t: 501, v: -120.0 }];
        tr.auto.pan = vec![AutoPoint { t: 0, v: -1.0 }];
        let p = Project { tracks: vec![tr], master: Master::default() };
        let out = render_range(&p, &sources, rate, 0, 1000, 0.0, &|_| {}, &crate::Cancel::default());
        assert!(out.ch[0][100] > 0.4, "left {}", out.ch[0][100]);
        assert!(out.ch[1][100].abs() < 1e-6, "right {}", out.ch[1][100]);
        assert!(out.ch[0][900].abs() < 1e-5, "after fade {}", out.ch[0][900]);
    }

    fn dc_sources(rate: u32, v: f32, frames: usize) -> Sources {
        let mut s = Sources::new();
        s.insert(1, Arc::new(AudioBuffer::from_channels(rate, vec![vec![v; frames], vec![v; frames]])));
        s
    }
    fn audio(id: u64, len: i64) -> Track {
        Track { id, clips: vec![Clip { id: id * 10, source: 1, start: 0, offset: 0, len, ..Default::default() }], ..Default::default() }
    }
    fn render(p: &Project, s: &Sources, rate: u32, len: i64) -> AudioBuffer {
        render_range(p, s, rate, 0, len, 0.0, &|_| {}, &crate::Cancel::default())
    }

    /// Output to a bus and a send both reach the master through the bus fader.
    #[test]
    fn buses_and_sends_sum() {
        let rate = 1000;
        let s = dc_sources(rate, 0.25, 1000);
        let mut a = audio(1, 1000);
        a.output = 3;
        let mut b = audio(2, 1000);
        b.sends = vec![Send { id: 7, target: 3, gain: 0.0, pre: false, on: true }];
        let bus = Track { id: 3, bus: true, gain: -6.0206, ..Default::default() };
        let p = Project { tracks: vec![bus.clone(), a, b], master: Master::default() };
        let out = render(&p, &s, rate, 1000);
        // master = b direct (0.25) + bus·(a + b) = 0.25 + 0.5·0.5 = 0.5
        assert!((out.ch[0][500] - 0.5).abs() < 1e-3, "{}", out.ch[0][500]);
        // muting the bus leaves only b
        let mut p2 = p.clone();
        p2.tracks[0].mute = true;
        assert!((render(&p2, &s, rate, 1000).ch[0][500] - 0.25).abs() < 1e-3);
        // soloing a keeps the bus it feeds
        let mut p3 = p.clone();
        p3.tracks[1].solo = true;
        assert!((render(&p3, &s, rate, 1000).ch[0][500] - 0.125).abs() < 1e-3);
    }

    /// A loop of buses must not hang or blow up.
    #[test]
    fn routing_loops_are_broken() {
        let rate = 1000;
        let s = dc_sources(rate, 0.25, 500);
        let mut x = Track { id: 1, bus: true, output: 2, ..Default::default() };
        x.sends = vec![Send { id: 1, target: 2, on: true, ..Default::default() }];
        let y = Track { id: 2, bus: true, output: 1, sends: vec![Send { id: 2, target: 1, on: true, ..Default::default() }], ..Default::default() };
        let mut a = audio(3, 500);
        a.output = 1;
        let p = Project { tracks: vec![x, y, a], master: Master::default() };
        let out = render(&p, &s, rate, 500);
        assert!(out.ch[0].iter().all(|v| v.is_finite() && v.abs() < 2.0));
        assert!(out.ch[0][250] > 0.1);
    }

    /// A compressor keyed by another track ducks only while that track plays.
    #[test]
    fn sidechain_ducks() {
        let rate = 8000;
        let mut s = dc_sources(rate, 0.5, 8000);
        // key: loud in the second half only
        let mut key = vec![0.0f32; 8000];
        key[4000..].fill(0.9);
        s.insert(2, Arc::new(AudioBuffer::from_channels(rate, vec![key.clone(), key])));
        let mut music = audio(1, 8000);
        music.effects = vec![EffectSlot {
            id: 1,
            on: true,
            spec: serde_json::from_str(r#"{"type":"compressor","threshold":-30,"ratio":10,"attack":1,"release":50,"knee":0,"makeup":0,"mix":1,"sidechain":2}"#).unwrap(),
        }];
        let voice = Track { id: 2, mute: true, clips: vec![Clip { id: 9, source: 2, start: 0, offset: 0, len: 8000, ..Default::default() }], ..Default::default() };
        let p = Project { tracks: vec![music, voice], master: Master::default() };
        let out = render(&p, &s, rate, 8000);
        assert!((out.ch[0][3000] - 0.5).abs() < 0.01, "before key {}", out.ch[0][3000]);
        assert!(out.ch[0][7000] < 0.2, "ducked {}", out.ch[0][7000]);
    }

    /// A look-ahead limiter on one track no longer shifts it against the others.
    #[test]
    fn delay_compensation_aligns_tracks() {
        let rate = 10000;
        let mut s = Sources::new();
        let mut click = vec![0.0f32; 2000];
        click[1000] = 0.5;
        s.insert(1, Arc::new(AudioBuffer::from_channels(rate, vec![click.clone(), click])));
        let mut a = audio(1, 2000);
        a.effects = vec![EffectSlot { id: 1, on: true, spec: serde_json::from_str(r#"{"type":"limiter","ceiling":0,"release":50,"input":0,"lookahead":5}"#).unwrap() }];
        let b = audio(2, 2000);
        let p = Project { tracks: vec![a, b], master: Master::default() };
        let out = render(&p, &s, rate, 2000);
        let peak = out.ch[0].iter().enumerate().fold((0, 0.0f32), |m, (i, v)| if v.abs() > m.1 { (i, v.abs()) } else { m });
        assert_eq!(peak.0, 1000, "both clicks at frame 1000");
        assert!(peak.1 > 0.9, "summed {}", peak.1);
    }

    /// Overlapping clips crossfade instead of summing.
    #[test]
    fn overlapping_clips_crossfade() {
        let rate = 1000;
        let s = dc_sources(rate, 0.5, 1000);
        let t = Track {
            id: 1,
            clips: vec![Clip { id: 1, source: 1, start: 0, offset: 0, len: 600, ..Default::default() }, Clip { id: 2, source: 1, start: 400, offset: 0, len: 600, ..Default::default() }],
            ..Default::default()
        };
        let p = Project { tracks: vec![t], master: Master::default() };
        let out = render(&p, &s, rate, 1000);
        // equal-power: the middle of the overlap sums to √2·0.5·0.5·… ≈ 0.707
        assert!(out.ch[0][500] < 0.75 && out.ch[0][500] > 0.6, "{}", out.ch[0][500]);
        assert!((out.ch[0][100] - 0.5).abs() < 1e-4);
        assert!((out.ch[0][900] - 0.5).abs() < 1e-4);
    }

    /// A frozen track plays its rendered source and skips its clips and effects.
    #[test]
    fn frozen_track_plays_its_render() {
        let rate = 1000;
        let mut s = dc_sources(rate, 0.5, 1000);
        s.insert(5, Arc::new(AudioBuffer::from_channels(rate, vec![vec![0.1; 1000], vec![0.1; 1000]])));
        let mut t = audio(1, 1000);
        t.freeze = Some(Freeze { source: 5, start: 0 });
        let p = Project { tracks: vec![t], master: Master::default() };
        assert!((render(&p, &s, rate, 1000).ch[0][500] - 0.1).abs() < 1e-5);
    }
}
