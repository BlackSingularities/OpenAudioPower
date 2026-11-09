//! Serializable effect descriptions shared with the UI, and the insert chain that turns them
//! into running processors (used by the real-time engine and by offline rendering alike).

use crate::dsp::biquad::{EqBand, Equalizer, Filter, FilterKind};
use crate::dsp::delay::Delay;
use crate::dsp::dynamics::{Compressor, DeEsser, Gate, Limiter};
use crate::dsp::modulation::{ModDelay, Phaser, Tremolo};
use crate::dsp::reverb::Reverb;
use crate::dsp::saturation::{BitCrusher, Curve, Saturator, StereoTool};
use crate::dsp::extra::{AutoFilter, Clipper, Comb, Exciter, FilterMode, Haas, Ladder, Multiband, RingMod, Tape, Tilt, Transient, Vibrato, Vowel};
use crate::dsp::Processor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EffectSpec {
    #[serde(rename_all = "camelCase")]
    Eq { bands: Vec<EqBand> },
    #[serde(rename_all = "camelCase")]
    Filter { kind: FilterKind, freq: f32, q: f32, slope: usize },
    #[serde(rename_all = "camelCase")]
    Compressor { threshold: f32, ratio: f32, attack: f32, release: f32, knee: f32, makeup: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Limiter { ceiling: f32, release: f32, input: f32, lookahead: f32 },
    #[serde(rename_all = "camelCase")]
    Gate { threshold: f32, attack: f32, release: f32, hold: f32, range: f32 },
    #[serde(rename_all = "camelCase")]
    DeEsser { freq: f32, threshold: f32, reduction: f32 },
    #[serde(rename_all = "camelCase")]
    Reverb { room: f32, damping: f32, wet: f32, dry: f32, width: f32, predelay: f32, lowcut: f32 },
    #[serde(rename_all = "camelCase")]
    Delay { time: f32, feedback: f32, mix: f32, pingpong: bool, tone: f32 },
    #[serde(rename_all = "camelCase")]
    Chorus { rate: f32, depth: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Flanger { rate: f32, depth: f32, feedback: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Phaser { rate: f32, min: f32, max: f32, feedback: f32, mix: f32, stages: usize },
    #[serde(rename_all = "camelCase")]
    Tremolo { rate: f32, depth: f32, pan: bool },
    #[serde(rename_all = "camelCase")]
    Saturation { drive: f32, curve: Curve, mix: f32, output: f32, tone: f32 },
    #[serde(rename_all = "camelCase")]
    BitCrusher { bits: f32, downsample: usize, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Stereo { gain: f32, pan: f32, width: f32, invert_l: bool, invert_r: bool, mono: bool },
    #[serde(rename_all = "camelCase")]
    Ladder { mode: FilterMode, cutoff: f32, resonance: f32, drive: f32 },
    #[serde(rename_all = "camelCase")]
    AutoFilter { mode: FilterMode, base: f32, depth: f32, rate: f32, envelope: bool, q: f32, sensitivity: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Vowel { vowel: f32, resonance: f32, rate: f32, depth: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Tilt { tilt: f32, pivot: f32 },
    #[serde(rename_all = "camelCase")]
    Comb { freq: f32, feedback: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Multiband { low_x: f32, high_x: f32, thr_low: f32, thr_mid: f32, thr_high: f32, gain_low: f32, gain_mid: f32, gain_high: f32, ratio: f32, attack: f32, release: f32 },
    #[serde(rename_all = "camelCase")]
    Transient { attack: f32, sustain: f32, output: f32 },
    #[serde(rename_all = "camelCase")]
    Exciter { freq: f32, drive: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Clipper { input: f32, ceiling: f32, softness: f32, output: f32 },
    #[serde(rename_all = "camelCase")]
    Tape { drive: f32, wow: f32, flutter: f32, hiss: f32, tone: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    RingMod { freq: f32, mix: f32 },
    #[serde(rename_all = "camelCase")]
    Vibrato { rate: f32, depth: f32 },
    #[serde(rename_all = "camelCase")]
    Haas { delay: f32, right: bool, mix: f32 },
    /// A VST3 / CLAP plug-in running in `oap-plughost`. `instance` names the running helper;
    /// the rest lets a project start it again (`state` is the plug-in's own blob, base64).
    #[serde(rename_all = "camelCase")]
    Plugin {
        instance: u64,
        #[serde(default)]
        name: String,
        #[serde(default)]
        format: String,
        #[serde(default)]
        path: String,
        #[serde(default)]
        id: String,
        #[serde(default)]
        state: String,
    },
}

/// Creates the processor of a plug-in slot (registered by the app, which owns the helper
/// processes). Without it, or for an instance that is not running, the slot passes audio through.
pub type ExternalFactory = fn(instance: u64) -> Option<Box<dyn Processor>>;
static EXTERNAL: std::sync::OnceLock<ExternalFactory> = std::sync::OnceLock::new();

pub fn set_external_factory(f: ExternalFactory) {
    let _ = EXTERNAL.set(f);
}

struct Passthrough;
impl Processor for Passthrough {
    fn process(&mut self, _buf: &mut [Vec<f32>], _n: usize) {}
}

impl EffectSpec {
    fn kind(&self) -> std::mem::Discriminant<EffectSpec> {
        std::mem::discriminant(self)
    }

    fn instance(&self) -> u64 {
        if let EffectSpec::Plugin { instance, .. } = self { *instance } else { 0 }
    }
}

/// One slot of a chain as stored in the project.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectSlot {
    pub id: u64,
    #[serde(default = "yes")]
    pub on: bool,
    pub spec: EffectSpec,
}
fn yes() -> bool {
    true
}

enum Proc {
    Eq(Equalizer),
    Filter(Filter),
    Comp(Compressor),
    Lim(Limiter),
    Gate(Gate),
    DeEss(DeEsser),
    Rev(Box<Reverb>),
    Delay(Delay),
    Mod(ModDelay),
    Phaser(Phaser),
    Trem(Tremolo),
    Sat(Saturator),
    Crush(BitCrusher),
    Stereo(StereoTool),
    Ext(Box<dyn Processor>),
    Ladder(Ladder),
    AutoFilter(AutoFilter),
    Vowel(Vowel),
    Tilt(Tilt),
    Comb(Comb),
    Multi(Box<Multiband>),
    Transient(Transient),
    Exciter(Exciter),
    Clipper(Clipper),
    Tape(Box<Tape>),
    Ring(RingMod),
    Vibrato(Vibrato),
    Haas(Haas),
}

impl Proc {
    fn new(spec: &EffectSpec, rate: f32) -> Self {
        let mut p = match spec {
            EffectSpec::Eq { bands } => Proc::Eq(Equalizer::new(rate, bands)),
            EffectSpec::Filter { kind, freq, q, slope } => Proc::Filter(Filter::new(rate, *kind, *freq, *q, *slope)),
            EffectSpec::Compressor { .. } => Proc::Comp(Compressor::new(rate)),
            EffectSpec::Limiter { .. } => Proc::Lim(Limiter::new(rate)),
            EffectSpec::Gate { .. } => Proc::Gate(Gate::new(rate)),
            EffectSpec::DeEsser { .. } => Proc::DeEss(DeEsser::new(rate)),
            EffectSpec::Reverb { .. } => Proc::Rev(Box::new(Reverb::new(rate))),
            EffectSpec::Delay { .. } => Proc::Delay(Delay::new(rate)),
            EffectSpec::Chorus { .. } | EffectSpec::Flanger { .. } => Proc::Mod(ModDelay::new(rate)),
            EffectSpec::Phaser { .. } => Proc::Phaser(Phaser::new(rate)),
            EffectSpec::Tremolo { .. } => Proc::Trem(Tremolo::new(rate)),
            EffectSpec::Saturation { .. } => Proc::Sat(Saturator::new(rate)),
            EffectSpec::BitCrusher { .. } => Proc::Crush(BitCrusher::new()),
            EffectSpec::Stereo { .. } => Proc::Stereo(StereoTool::new()),
            EffectSpec::Ladder { .. } => Proc::Ladder(Ladder::new(rate)),
            EffectSpec::AutoFilter { .. } => Proc::AutoFilter(AutoFilter::new(rate)),
            EffectSpec::Vowel { .. } => Proc::Vowel(Vowel::new(rate)),
            EffectSpec::Tilt { .. } => Proc::Tilt(Tilt::new(rate)),
            EffectSpec::Comb { .. } => Proc::Comb(Comb::new(rate)),
            EffectSpec::Multiband { .. } => Proc::Multi(Box::new(Multiband::new(rate))),
            EffectSpec::Transient { .. } => Proc::Transient(Transient::new(rate)),
            EffectSpec::Exciter { .. } => Proc::Exciter(Exciter::new(rate)),
            EffectSpec::Clipper { .. } => Proc::Clipper(Clipper::new()),
            EffectSpec::Tape { .. } => Proc::Tape(Box::new(Tape::new(rate))),
            EffectSpec::RingMod { .. } => Proc::Ring(RingMod::new(rate)),
            EffectSpec::Vibrato { .. } => Proc::Vibrato(Vibrato::new(rate)),
            EffectSpec::Haas { .. } => Proc::Haas(Haas::new(rate)),
            EffectSpec::Plugin { instance, .. } => Proc::Ext(EXTERNAL.get().and_then(|f| f(*instance)).unwrap_or_else(|| Box::new(Passthrough))),
        };
        p.update(spec);
        p
    }

    fn update(&mut self, spec: &EffectSpec) {
        match (self, spec) {
            (Proc::Eq(p), EffectSpec::Eq { bands }) => p.set(bands),
            (Proc::Filter(p), EffectSpec::Filter { kind, freq, q, slope }) => p.set(*kind, *freq, *q, *slope),
            (Proc::Comp(p), EffectSpec::Compressor { threshold, ratio, attack, release, knee, makeup, mix }) => {
                p.set(*threshold, *ratio, *attack, *release, *knee, *makeup, *mix)
            }
            (Proc::Lim(p), EffectSpec::Limiter { ceiling, release, input, lookahead }) => p.set(*ceiling, *release, *input, *lookahead),
            (Proc::Gate(p), EffectSpec::Gate { threshold, attack, release, hold, range }) => p.set(*threshold, *attack, *release, *hold, *range, 4.0),
            (Proc::DeEss(p), EffectSpec::DeEsser { freq, threshold, reduction }) => p.set(*freq, *threshold, *reduction),
            (Proc::Rev(p), EffectSpec::Reverb { room, damping, wet, dry, width, predelay, lowcut }) => {
                p.set(*room, *damping, *wet, *dry, *width, *predelay, *lowcut)
            }
            (Proc::Delay(p), EffectSpec::Delay { time, feedback, mix, pingpong, tone }) => p.set(*time, *feedback, *mix, *pingpong, *tone),
            (Proc::Mod(p), EffectSpec::Chorus { rate, depth, mix }) => p.set(*rate, 12.0, *depth * 8.0, 0.0, *mix),
            (Proc::Mod(p), EffectSpec::Flanger { rate, depth, feedback, mix }) => p.set(*rate, 0.6, *depth * 4.0, *feedback, *mix),
            (Proc::Phaser(p), EffectSpec::Phaser { rate, min, max, feedback, mix, stages }) => p.set(*rate, *min, *max, *feedback, *mix, *stages),
            (Proc::Trem(p), EffectSpec::Tremolo { rate, depth, pan }) => p.set(*rate, *depth, *pan),
            (Proc::Sat(p), EffectSpec::Saturation { drive, curve, mix, output, tone }) => p.set(*drive, *curve, *mix, *output, *tone),
            (Proc::Crush(p), EffectSpec::BitCrusher { bits, downsample, mix }) => p.set(*bits, *downsample, *mix),
            (Proc::Stereo(p), EffectSpec::Stereo { gain, pan, width, invert_l, invert_r, mono }) => p.set(*gain, *pan, *width, *invert_l, *invert_r, *mono),
            (Proc::Ladder(p), EffectSpec::Ladder { mode, cutoff, resonance, drive }) => p.set(*mode, *cutoff, *resonance, *drive),
            (Proc::AutoFilter(p), EffectSpec::AutoFilter { mode, base, depth, rate, envelope, q, sensitivity, mix }) => p.set(*mode, *base, *depth, *rate, *envelope, *q, *sensitivity, *mix),
            (Proc::Vowel(p), EffectSpec::Vowel { vowel, resonance, rate, depth, mix }) => p.set(*vowel, *resonance, *rate, *depth, *mix),
            (Proc::Tilt(p), EffectSpec::Tilt { tilt, pivot }) => p.set(*tilt, *pivot),
            (Proc::Comb(p), EffectSpec::Comb { freq, feedback, mix }) => p.set(*freq, *feedback, *mix),
            (Proc::Multi(p), EffectSpec::Multiband { low_x, high_x, thr_low, thr_mid, thr_high, gain_low, gain_mid, gain_high, ratio, attack, release }) => {
                p.set(*low_x, *high_x, [*thr_low, *thr_mid, *thr_high], [*gain_low, *gain_mid, *gain_high], *ratio, *attack, *release)
            }
            (Proc::Transient(p), EffectSpec::Transient { attack, sustain, output }) => p.set(*attack, *sustain, *output),
            (Proc::Exciter(p), EffectSpec::Exciter { freq, drive, mix }) => p.set(*freq, *drive, *mix),
            (Proc::Clipper(p), EffectSpec::Clipper { input, ceiling, softness, output }) => p.set(*input, *ceiling, *softness, *output),
            (Proc::Tape(p), EffectSpec::Tape { drive, wow, flutter, hiss, tone, mix }) => p.set(*drive, *wow, *flutter, *hiss, *tone, *mix),
            (Proc::Ring(p), EffectSpec::RingMod { freq, mix }) => p.set(*freq, *mix),
            (Proc::Vibrato(p), EffectSpec::Vibrato { rate, depth }) => p.set(*rate, *depth),
            (Proc::Haas(p), EffectSpec::Haas { delay, right, mix }) => p.set(*delay, *right, *mix),
            _ => {}
        }
    }

    fn get(&mut self) -> &mut dyn Processor {
        match self {
            Proc::Eq(p) => p,
            Proc::Filter(p) => p,
            Proc::Comp(p) => p,
            Proc::Lim(p) => p,
            Proc::Gate(p) => p,
            Proc::DeEss(p) => p,
            Proc::Rev(p) => p.as_mut(),
            Proc::Delay(p) => p,
            Proc::Mod(p) => p,
            Proc::Phaser(p) => p,
            Proc::Trem(p) => p,
            Proc::Sat(p) => p,
            Proc::Crush(p) => p,
            Proc::Stereo(p) => p,
            Proc::Ext(p) => p.as_mut(),
            Proc::Ladder(p) => p,
            Proc::AutoFilter(p) => p,
            Proc::Vowel(p) => p,
            Proc::Tilt(p) => p,
            Proc::Comb(p) => p,
            Proc::Multi(p) => p.as_mut(),
            Proc::Transient(p) => p,
            Proc::Exciter(p) => p,
            Proc::Clipper(p) => p,
            Proc::Tape(p) => p.as_mut(),
            Proc::Ring(p) => p,
            Proc::Vibrato(p) => p,
            Proc::Haas(p) => p,
        }
    }
}

struct Slot {
    id: u64,
    on: bool,
    kind: std::mem::Discriminant<EffectSpec>,
    spec: EffectSpec,
    proc: Proc,
}

/// Insert chain: keeps processor state across parameter changes.
pub struct Chain {
    rate: f32,
    slots: Vec<Slot>,
}

impl Chain {
    pub fn new(rate: f32) -> Self {
        Self { rate, slots: Vec::new() }
    }

    /// Bring the chain in line with `specs`, reusing processors whose id and type match.
    pub fn sync(&mut self, specs: &[EffectSlot]) {
        let same_layout = specs.len() == self.slots.len() && specs.iter().zip(&self.slots).all(|(a, b)| a.id == b.id && a.spec.kind() == b.kind && a.spec.instance() == b.spec.instance());
        if same_layout {
            for (a, b) in specs.iter().zip(self.slots.iter_mut()) {
                b.on = a.on;
                if a.spec != b.spec {
                    b.proc.update(&a.spec);
                    b.spec = a.spec.clone();
                }
            }
            return;
        }
        let mut old: Vec<Slot> = std::mem::take(&mut self.slots);
        for a in specs {
            if let Some(i) = old.iter().position(|b| b.id == a.id && b.kind == a.spec.kind() && b.spec.instance() == a.spec.instance()) {
                let mut s = old.swap_remove(i);
                s.on = a.on;
                if s.spec != a.spec {
                    s.proc.update(&a.spec);
                    s.spec = a.spec.clone();
                }
                self.slots.push(s);
            } else {
                self.slots.push(Slot { id: a.id, on: a.on, kind: a.spec.kind(), spec: a.spec.clone(), proc: Proc::new(&a.spec, self.rate) });
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| !s.on)
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], n: usize) {
        for s in self.slots.iter_mut() {
            if s.on {
                s.proc.get().process(buf, n);
            }
        }
    }

    pub fn reset(&mut self) {
        for s in self.slots.iter_mut() {
            s.proc.get().reset();
        }
    }

    pub fn latency(&mut self) -> usize {
        self.slots.iter_mut().filter(|s| s.on).map(|s| s.proc.get().latency()).sum()
    }

    /// (slot id, gain reduction dB) for dynamics meters.
    pub fn reductions(&mut self) -> Vec<(u64, f32)> {
        self.slots.iter_mut().filter(|s| s.on).map(|s| (s.id, s.proc.get().reduction_db())).filter(|x| x.1 > 0.0).collect()
    }
}

/// Run a whole buffer through a one-off chain (offline "apply effect").
pub fn render_offline(ch: &mut [Vec<f32>], rate: f32, spec: &EffectSpec, tail_frames: usize) {
    let mut chain = Chain::new(rate);
    chain.sync(&[EffectSlot { id: 1, on: true, spec: spec.clone() }]);
    let n = ch.first().map(|c| c.len()).unwrap_or(0);
    let lat = chain.latency();
    for c in ch.iter_mut() {
        c.resize(n + tail_frames + lat, 0.0);
    }
    let total = n + tail_frames + lat;
    let block = 4096;
    let mut tmp: Vec<Vec<f32>> = vec![vec![0.0; block]; ch.len()];
    let mut pos = 0;
    while pos < total {
        let m = block.min(total - pos);
        for (t, c) in tmp.iter_mut().zip(ch.iter()) {
            t[..m].copy_from_slice(&c[pos..pos + m]);
        }
        chain.process(&mut tmp, m);
        for (t, c) in tmp.iter().zip(ch.iter_mut()) {
            c[pos..pos + m].copy_from_slice(&t[..m]);
        }
        pos += m;
    }
    if lat > 0 {
        for c in ch.iter_mut() {
            c.drain(..lat);
        }
    }
}

/// Frequency response of an EQ (for the curve editor), `points` log-spaced 20 Hz – 20 kHz.
pub fn eq_response(bands: &[EqBand], rate: f32, points: usize) -> Vec<f32> {
    use crate::dsp::biquad::Coefs;
    let coefs: Vec<Coefs> = bands.iter().filter(|b| b.on).map(|b| Coefs::new(b.kind, rate, b.freq, b.q, b.gain)).collect();
    (0..points)
        .map(|i| {
            let f = 20.0 * (1000.0f32).powf(i as f32 / (points - 1).max(1) as f32);
            coefs.iter().map(|c| c.response_db(f, rate)).sum()
        })
        .collect()
}
