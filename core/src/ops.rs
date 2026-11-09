//! Destructive (offline) operations on a source or a range of it. Sources are immutable,
//! so every operation returns a new buffer – undo simply points the clip back.

use crate::analysis;
use crate::buffer::{db_to_gain, AudioBuffer};
use crate::effects::{render_offline, EffectSpec};
use crate::spectral;
use crate::Cancel;
use rayon::prelude::*;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FadeCurve {
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    EqualPower,
}

pub fn fade_gain(t: f32, curve: FadeCurve) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        FadeCurve::Linear => t,
        FadeCurve::Exponential => t * t * t,
        FadeCurve::Logarithmic => 1.0 - (1.0 - t).powi(3),
        FadeCurve::SCurve => t * t * (3.0 - 2.0 * t),
        FadeCurve::EqualPower => (t * std::f32::consts::FRAC_PI_2).sin(),
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    #[serde(rename_all = "camelCase")]
    Effect { spec: EffectSpec, #[serde(default)] tail: f32 },
    Gain { db: f32 },
    #[serde(rename_all = "camelCase")]
    Normalize { mode: NormMode, target: f32, #[serde(default)] per_channel: bool },
    FadeIn { curve: FadeCurve },
    FadeOut { curve: FadeCurve },
    Reverse,
    Invert,
    Silence,
    RemoveDc,
    Mono,
    Stereo,
    SwapChannels,
    Stretch { ratio: f32 },
    Pitch { semitones: f32 },
    Speed { ratio: f32 },
    #[serde(rename_all = "camelCase")]
    Denoise { reduction: f32, sensitivity: f32, #[serde(default)] profile_start: Option<usize>, #[serde(default)] profile_end: Option<usize> },
    Delete,
    Crop,
    #[serde(rename_all = "camelCase")]
    InsertSilence { seconds: f32 },
    #[serde(rename_all = "camelCase")]
    TrimSilence { threshold: f32 },
    Declick { strength: f32 },
    /// Shortens pauses inside the range (Audacity's Truncate Silence).
    #[serde(rename_all = "camelCase")]
    TruncateSilence { threshold: f32, min_length: f32, keep: f32 },
    /// Extreme stretch with randomised phases; `window` in seconds.
    Paulstretch { factor: f32, window: f32 },
    /// Tempo (percent) and pitch (semitones) gliding from start to end values.
    #[serde(rename_all = "camelCase")]
    SlidingStretch { tempo_start: f32, tempo_end: f32, pitch_start: f32, pitch_end: f32 },
    /// The range plays `count` more times right after itself.
    Repeat { count: u32 },
    /// Sample rate: `reinterpret` plays the audio as if it had been recorded at `rate` (speed and
    /// pitch change, like Audacity's Set Rate); otherwise it is converted to `rate` and back,
    /// which limits the bandwidth (e.g. 8000 Hz telephone sound).
    Resample { rate: u32, #[serde(default)] reinterpret: bool },
    /// Pencil: replaces the range with these sample values (all channels).
    Draw { values: Vec<f32> },
    #[serde(rename_all = "camelCase")]
    Declip { tolerance: f32, #[serde(default)] gain: f32 },
    #[serde(rename_all = "camelCase")]
    Dehum { base: f32, harmonics: usize, q: f32 },
    #[serde(rename_all = "camelCase")]
    Deplosive { freq: f32, sensitivity: f32, reduction: f32 },
    #[serde(rename_all = "camelCase")]
    Level { target: f32, max_gain: f32, gate: f32, speed: f32 },
    /// DeepFilterNet speech enhancement, optionally followed by late-reverb suppression.
    #[serde(rename_all = "camelCase")]
    VoiceDenoise { strength: f32, #[serde(default)] dereverb: f32, #[serde(default = "room_default")] room: f32 },
    #[serde(rename_all = "camelCase")]
    Dereverb { amount: f32, #[serde(default = "room_default")] room: f32 },
    /// Spectral repair of the range between two frequencies.
    #[serde(rename_all = "camelCase")]
    SpectralRepair { f_lo: f32, f_hi: f32, mode: crate::enhance::RepairMode, #[serde(default)] gain: f32 },
}

fn room_default() -> f32 {
    0.6
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NormMode {
    Peak,
    Lufs,
    Rms,
}

/// Short equal-power crossfade between original and processed audio at range borders.
fn blend_edges(orig: &AudioBuffer, processed: &mut AudioBuffer, frames: usize) {
    let n = processed.frames().min(orig.frames());
    let f = frames.min(n / 2);
    if f == 0 {
        return;
    }
    for (c, ch) in processed.ch.iter_mut().enumerate() {
        let o = &orig.ch[c.min(orig.channels() - 1)];
        for i in 0..f {
            let t = fade_gain(i as f32 / f as f32, FadeCurve::EqualPower);
            let u = fade_gain(1.0 - i as f32 / f as f32, FadeCurve::EqualPower);
            ch[i] = ch[i] * t + o[i] * u;
            let j = n - 1 - i;
            ch[j] = ch[j] * t + o[j] * u;
        }
    }
}

pub fn apply(src: &AudioBuffer, start: usize, end: usize, op: &Op, cancel: &Cancel) -> Result<AudioBuffer, String> {
    let n = src.frames();
    let s = start.min(n);
    let e = if end == 0 || end > n { n } else { end.max(s) };
    let whole = s == 0 && e == n;
    let rate = src.rate;
    let part = src.slice(s, e);
    let edge = (rate as f32 * 0.004) as usize;

    let processed: AudioBuffer = match op {
        Op::Delete => return Ok(src.splice(s, e, &AudioBuffer::new(rate, src.channels(), 0))),
        Op::Crop => return Ok(part),
        Op::InsertSilence { seconds } => {
            let k = (seconds.max(0.0) * rate as f32) as usize;
            return Ok(src.splice(s, s, &AudioBuffer::new(rate, src.channels(), k)));
        }
        Op::Mono => {
            let m = src.mono();
            return Ok(AudioBuffer { rate, ch: vec![m] });
        }
        Op::Stereo => return Ok(src.to_stereo()),
        Op::SwapChannels => {
            let mut b = src.clone();
            if b.channels() >= 2 {
                b.ch.swap(0, 1);
            }
            return Ok(b);
        }
        Op::TrimSilence { threshold } => {
            let thr = db_to_gain(*threshold);
            let loud = |i: usize| src.ch.iter().any(|c| c[i].abs() > thr);
            let first = (0..n).find(|&i| loud(i)).unwrap_or(0);
            let last = (0..n).rev().find(|&i| loud(i)).map(|i| i + 1).unwrap_or(n);
            let pad = (rate as f32 * 0.01) as usize;
            return Ok(src.slice(first.saturating_sub(pad), (last + pad).min(n)));
        }
        Op::Gain { db } => {
            let g = db_to_gain(*db);
            let mut p = part;
            p.ch.iter_mut().flatten().for_each(|x| *x *= g);
            p
        }
        Op::Normalize { mode, target, per_channel } => {
            let mut p = part;
            match mode {
                NormMode::Peak => {
                    let t = db_to_gain(*target);
                    if *per_channel {
                        for c in p.ch.iter_mut() {
                            let pk = c.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                            if pk > 1e-9 {
                                let g = t / pk;
                                c.iter_mut().for_each(|x| *x *= g);
                            }
                        }
                    } else {
                        let pk = p.peak();
                        if pk > 1e-9 {
                            let g = t / pk;
                            p.ch.iter_mut().flatten().for_each(|x| *x *= g);
                        }
                    }
                }
                NormMode::Rms => {
                    let rms = analysis::rms(&p);
                    if rms > 1e-9 {
                        let g = db_to_gain(*target) / rms;
                        p.ch.iter_mut().flatten().for_each(|x| *x *= g);
                    }
                }
                NormMode::Lufs => {
                    let l = analysis::integrated_lufs(&p);
                    if l.is_finite() && l > -70.0 {
                        let g = db_to_gain((*target as f64 - l) as f32);
                        p.ch.iter_mut().flatten().for_each(|x| *x *= g);
                    }
                }
            }
            p
        }
        Op::FadeIn { curve } => {
            let mut p = part;
            let k = p.frames().max(1);
            for c in p.ch.iter_mut() {
                for (i, x) in c.iter_mut().enumerate() {
                    *x *= fade_gain(i as f32 / k as f32, *curve);
                }
            }
            p
        }
        Op::FadeOut { curve } => {
            let mut p = part;
            let k = p.frames().max(1);
            for c in p.ch.iter_mut() {
                for (i, x) in c.iter_mut().enumerate() {
                    *x *= fade_gain(1.0 - i as f32 / k as f32, *curve);
                }
            }
            p
        }
        Op::Reverse => {
            let mut p = part;
            p.ch.iter_mut().for_each(|c| c.reverse());
            p
        }
        Op::Invert => {
            let mut p = part;
            p.ch.iter_mut().flatten().for_each(|x| *x = -*x);
            p
        }
        Op::Silence => AudioBuffer::new(rate, part.channels(), part.frames()),
        Op::RemoveDc => {
            let mut p = part;
            // 5 Hz one-pole high-pass, run forward and backward for zero phase
            let a = (-2.0 * std::f32::consts::PI * 5.0 / rate as f32).exp();
            for c in p.ch.iter_mut() {
                for _ in 0..2 {
                    let mut lp = c.first().copied().unwrap_or(0.0);
                    for x in c.iter_mut() {
                        lp = *x + a * (lp - *x);
                        *x -= lp;
                    }
                    c.reverse();
                }
            }
            p
        }
        Op::Effect { spec, tail } => {
            let mut p = part.clone();
            let tail_frames = if whole { (tail.max(0.0) * rate as f32) as usize } else { 0 };
            render_offline(&mut p.ch, rate as f32, spec, tail_frames);
            if !whole {
                blend_edges(&part, &mut p, edge);
            }
            p
        }
        Op::Stretch { ratio } => {
            let ch: Vec<Vec<f32>> = part.ch.par_iter().map(|c| spectral::time_stretch(c, *ratio, cancel)).collect();
            AudioBuffer::from_channels(rate, ch)
        }
        Op::Pitch { semitones } => {
            let ch: Vec<Vec<f32>> = part.ch.par_iter().map(|c| spectral::pitch_shift(c, *semitones, cancel)).collect();
            let mut p = AudioBuffer::from_channels(rate, ch);
            if !whole {
                blend_edges(&part, &mut p, edge);
            }
            p
        }
        Op::Speed { ratio } => {
            let r = ratio.clamp(0.1, 10.0) as f64;
            AudioBuffer::from_channels(rate, crate::resample::resample_ratio(&part.ch, 1.0 / r))
        }
        Op::Denoise { reduction, sensitivity, profile_start, profile_end } => {
            let profile: Option<AudioBuffer> = match (profile_start, profile_end) {
                (Some(a), Some(b)) if b > a => Some(src.slice(*a, *b)),
                _ => None,
            };
            let ch: Vec<Vec<f32>> = part
                .ch
                .par_iter()
                .enumerate()
                .map(|(i, c)| {
                    let pr = profile.as_ref().map(|p| p.ch[i.min(p.channels() - 1)].as_slice());
                    spectral::denoise(c, pr, *reduction, *sensitivity, cancel)
                })
                .collect();
            let mut p = AudioBuffer::from_channels(rate, ch);
            if !whole {
                blend_edges(&part, &mut p, edge);
            }
            p
        }
        Op::Declick { strength } => {
            let mut p = part;
            declick(&mut p, *strength);
            p
        }
        Op::TruncateSilence { threshold, min_length, keep } => crate::stretch::truncate_silence(&part, *threshold, *min_length, *keep).0,
        Op::Paulstretch { factor, window } => {
            let ch: Vec<Vec<f32>> = part.ch.par_iter().map(|c| crate::stretch::paulstretch(c, rate, *factor, *window, cancel)).collect();
            AudioBuffer::from_channels(rate, ch)
        }
        Op::SlidingStretch { tempo_start, tempo_end, pitch_start, pitch_end } => {
            let ch: Vec<Vec<f32>> = part.ch.par_iter().map(|c| crate::stretch::sliding_stretch(c, *tempo_start, *tempo_end, *pitch_start, *pitch_end, cancel)).collect();
            // channels can differ by a few samples; even them out
            let len = ch.iter().map(|c| c.len()).min().unwrap_or(0);
            AudioBuffer::from_channels(rate, ch.into_iter().map(|mut c| { c.truncate(len); c }).collect())
        }
        Op::Repeat { count } => {
            let k = (*count).clamp(1, 1000) as usize + 1;
            let ch: Vec<Vec<f32>> = part.ch.iter().map(|c| c.repeat(k)).collect();
            AudioBuffer::from_channels(rate, ch)
        }
        Op::Resample { rate: to, reinterpret } => {
            let to = (*to).clamp(1000, 384_000) as f64;
            let from = rate as f64;
            if *reinterpret {
                AudioBuffer::from_channels(rate, crate::resample::resample_ratio(&part.ch, from / to))
            } else {
                let down = crate::resample::resample_ratio(&part.ch, to / from);
                let mut back = crate::resample::resample_ratio(&down, from / to);
                back.iter_mut().for_each(|c| c.resize(part.frames(), 0.0));
                let mut p = AudioBuffer::from_channels(rate, back);
                if !whole {
                    blend_edges(&part, &mut p, edge);
                }
                p
            }
        }
        Op::Draw { values } => {
            let mut p = part;
            for c in p.ch.iter_mut() {
                for (x, v) in c.iter_mut().zip(values.iter()) {
                    *x = v.clamp(-4.0, 4.0);
                }
            }
            p
        }
        Op::Declip { tolerance, gain } => {
            let mut p = part;
            crate::repair::declip(&mut p, *tolerance, *gain);
            p
        }
        Op::Dehum { base, harmonics, q } => {
            let mut p = part;
            crate::repair::dehum(&mut p, *base, *harmonics, *q);
            if !whole {
                blend_edges(&src.slice(s, e), &mut p, edge);
            }
            p
        }
        Op::Deplosive { freq, sensitivity, reduction } => {
            let mut p = part;
            crate::repair::deplosive(&mut p, *freq, *sensitivity, *reduction);
            p
        }
        Op::Level { target, max_gain, gate, speed } => {
            let mut p = part;
            crate::repair::level(&mut p, *target, *max_gain, *gate, *speed);
            p
        }
        Op::VoiceDenoise { strength, dereverb, room } => {
            let ch: Result<Vec<Vec<f32>>, String> = part
                .ch
                .par_iter()
                .map(|c| {
                    let y = crate::enhance::voice_denoise(c, rate, *strength, cancel)?;
                    Ok(if *dereverb > 0.01 { crate::enhance::dereverb(&y, rate, *room, *dereverb, cancel) } else { y })
                })
                .collect();
            let mut p = AudioBuffer::from_channels(rate, ch?);
            if !whole {
                blend_edges(&part, &mut p, edge);
            }
            p
        }
        Op::Dereverb { amount, room } => {
            let ch: Vec<Vec<f32>> = part.ch.par_iter().map(|c| crate::enhance::dereverb(c, rate, *room, *amount, cancel)).collect();
            let mut p = AudioBuffer::from_channels(rate, ch);
            if !whole {
                blend_edges(&part, &mut p, edge);
            }
            p
        }
        Op::SpectralRepair { f_lo, f_hi, mode, gain } => {
            // the repair needs the sound around the range as background reference
            let margin = (rate as f32 * 0.4) as usize;
            let a = s.saturating_sub(margin);
            let b = (e + margin).min(n);
            let seg = src.slice(a, b);
            let ch: Vec<Vec<f32>> = seg
                .ch
                .par_iter()
                .map(|c| crate::enhance::spectral_repair(c, rate, s - a, e - a, *f_lo, *f_hi, *mode, *gain))
                .collect();
            if cancel.is_set() {
                return Err("cancelled".into());
            }
            return Ok(src.splice(a, b, &AudioBuffer::from_channels(rate, ch)));
        }
    };
    if cancel.is_set() {
        return Err("cancelled".into());
    }
    if whole {
        return Ok(processed);
    }
    Ok(src.splice(s, e, &processed))
}

/// Detects impulsive clicks (samples deviating strongly from a local prediction) and
/// replaces them by cubic interpolation.
fn declick(b: &mut AudioBuffer, strength: f32) {
    let k = 4.0 + (1.0 - strength.clamp(0.0, 1.0)) * 8.0;
    for c in b.ch.iter_mut() {
        let n = c.len();
        if n < 16 {
            continue;
        }
        // second-difference residual
        let mut res = vec![0.0f32; n];
        for i in 1..n - 1 {
            res[i] = (c[i] - 0.5 * (c[i - 1] + c[i + 1])).abs();
        }
        let win = 512;
        let mut i = 1;
        while i < n - 1 {
            let lo = i.saturating_sub(win);
            let hi = (i + win).min(n);
            // cheap local level: mean residual
            let mean = res[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
            if res[i] > mean * k && res[i] > 0.01 {
                let a = i.saturating_sub(3).max(1);
                let z = (i + 4).min(n - 2);
                let (y0, y1) = (c[a - 1], c[a]);
                let (y2, y3) = (c[z], c[z + 1]);
                let len = (z - a) as f32;
                for j in a + 1..z {
                    let t = (j - a) as f32 / len;
                    // Catmull-Rom between y1 and y2
                    let t2 = t * t;
                    let t3 = t2 * t;
                    c[j] = 0.5 * ((2.0 * y1) + (-y0 + y2) * t + (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3) * t2 + (-y0 + 3.0 * y1 - 3.0 * y2 + y3) * t3);
                }
                i = z;
            }
            i += 1;
        }
    }
}
