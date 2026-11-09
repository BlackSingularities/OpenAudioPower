//! Signal generators: tones, sweeps, coloured noise, DTMF, silence and a click track.

use crate::buffer::{db_to_gain, AudioBuffer};
use serde::Deserialize;
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Wave {
    Sine,
    Square,
    Saw,
    Triangle,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NoiseColor {
    White,
    Pink,
    Brown,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Gen {
    /// `level` in dBFS.
    Tone { wave: Wave, freq: f32, level: f32 },
    /// Sweep from `f0` to `f1`, exponential (musical) or linear.
    #[serde(rename_all = "camelCase")]
    Chirp { wave: Wave, f0: f32, f1: f32, level: f32, #[serde(default)] linear: bool },
    Noise { color: NoiseColor, level: f32 },
    /// Telephone keypad tones for `digits` (0-9, *, #, A-D; anything else is a pause).
    #[serde(rename_all = "camelCase")]
    Dtmf { digits: String, tone_ms: f32, gap_ms: f32, level: f32 },
    Silence,
    /// Metronome clicks, accent on the first beat of each bar.
    #[serde(rename_all = "camelCase")]
    Clicks { bpm: f32, beats: u32, level: f32 },
}

fn osc(w: Wave, ph: f64) -> f32 {
    let p = ph.rem_euclid(1.0);
    (match w {
        Wave::Sine => (TAU * p).sin(),
        Wave::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Saw => 2.0 * p - 1.0,
        Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
    }) as f32
}

/// Renders `seconds` of the generator at `rate` into `channels` identical channels.
pub fn generate(g: &Gen, rate: u32, seconds: f32, channels: usize) -> AudioBuffer {
    let n = (seconds.clamp(0.0, 3600.0) * rate as f32).round() as usize;
    let r = rate as f64;
    let mut x = vec![0.0f32; n];
    match g {
        Gen::Tone { wave, freq, level } => {
            let a = db_to_gain(*level);
            let f = freq.clamp(1.0, rate as f32 * 0.5) as f64;
            for (i, v) in x.iter_mut().enumerate() {
                *v = a * osc(*wave, f * i as f64 / r);
            }
        }
        Gen::Chirp { wave, f0, f1, level, linear } => {
            let a = db_to_gain(*level);
            let (f0, f1) = (f0.clamp(1.0, rate as f32 * 0.5) as f64, f1.clamp(1.0, rate as f32 * 0.5) as f64);
            let mut ph = 0.0f64;
            for (i, v) in x.iter_mut().enumerate() {
                let t = i as f64 / n.max(1) as f64;
                let f = if *linear { f0 + (f1 - f0) * t } else { f0 * (f1 / f0).powf(t) };
                *v = a * osc(*wave, ph);
                ph += f / r;
            }
        }
        Gen::Noise { color, level } => {
            let a = db_to_gain(*level);
            let mut s = 0x9E37_79B9u32;
            let mut white = || {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32) * 2.0 - 1.0
            };
            match color {
                NoiseColor::White => x.iter_mut().for_each(|v| *v = a * white()),
                NoiseColor::Pink => {
                    // Paul Kellet's refined pink filter
                    let mut b = [0.0f32; 7];
                    for v in x.iter_mut() {
                        let w = white();
                        b[0] = 0.99886 * b[0] + w * 0.0555179;
                        b[1] = 0.99332 * b[1] + w * 0.0750759;
                        b[2] = 0.96900 * b[2] + w * 0.1538520;
                        b[3] = 0.86650 * b[3] + w * 0.3104856;
                        b[4] = 0.55000 * b[4] + w * 0.5329522;
                        b[5] = -0.7616 * b[5] - w * 0.0168980;
                        let p = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
                        b[6] = w * 0.115926;
                        *v = a * p * 0.11;
                    }
                }
                NoiseColor::Brown => {
                    let mut y = 0.0f32;
                    for v in x.iter_mut() {
                        y = (y + 0.02 * white()) / 1.02;
                        *v = a * y * 3.5;
                    }
                }
            }
        }
        Gen::Dtmf { digits, tone_ms, gap_ms, level } => {
            let a = db_to_gain(*level) * 0.5;
            let tone = (tone_ms.max(10.0) * 0.001 * rate as f32) as usize;
            let gap = (gap_ms.max(0.0) * 0.001 * rate as f32) as usize;
            let mut at = 0usize;
            for d in digits.chars() {
                let pair = match d.to_ascii_uppercase() {
                    '1' => Some((697.0, 1209.0)),
                    '2' => Some((697.0, 1336.0)),
                    '3' => Some((697.0, 1477.0)),
                    'A' => Some((697.0, 1633.0)),
                    '4' => Some((770.0, 1209.0)),
                    '5' => Some((770.0, 1336.0)),
                    '6' => Some((770.0, 1477.0)),
                    'B' => Some((770.0, 1633.0)),
                    '7' => Some((852.0, 1209.0)),
                    '8' => Some((852.0, 1336.0)),
                    '9' => Some((852.0, 1477.0)),
                    'C' => Some((852.0, 1633.0)),
                    '*' => Some((941.0, 1209.0)),
                    '0' => Some((941.0, 1336.0)),
                    '#' => Some((941.0, 1477.0)),
                    'D' => Some((941.0, 1633.0)),
                    _ => None,
                };
                if let Some((lo, hi)) = pair {
                    let ramp = (rate as usize / 500).max(1);
                    for k in 0..tone {
                        if at + k >= n {
                            break;
                        }
                        let t = k as f64 / r;
                        let env = (k.min(tone - 1 - k) as f32 / ramp as f32).min(1.0);
                        x[at + k] = a * env * ((TAU * lo * t).sin() + (TAU * hi * t).sin()) as f32;
                    }
                }
                at += tone + gap;
                if at >= n {
                    break;
                }
            }
        }
        Gen::Silence => {}
        Gen::Clicks { bpm, beats, level } => {
            let a = db_to_gain(*level);
            let step = 60.0 / bpm.clamp(20.0, 400.0) as f64 * r;
            let len = (0.012 * r) as usize;
            let mut k = 0u64;
            loop {
                let at = (k as f64 * step).round() as usize;
                if at >= n {
                    break;
                }
                let accent = *beats > 0 && k % *beats as u64 == 0;
                let f = if accent { 1600.0 } else { 1000.0 };
                for j in 0..len.min(n - at) {
                    let env = (-(j as f64) / (len as f64 / 5.0)).exp();
                    x[at + j] = a * (if accent { 1.0 } else { 0.7 }) * (env * (TAU * f * j as f64 / r).sin()) as f32;
                }
                k += 1;
            }
        }
    }
    AudioBuffer::from_channels(rate, vec![x; channels.clamp(1, 2)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(b: &AudioBuffer) -> f32 {
        b.peak()
    }

    #[test]
    fn levels_and_lengths() {
        let r = 48000;
        let t = generate(&Gen::Tone { wave: Wave::Sine, freq: 1000.0, level: -6.0 }, r, 1.0, 2);
        assert_eq!(t.frames(), 48000);
        assert_eq!(t.channels(), 2);
        assert!((peak(&t) - 0.501).abs() < 0.01);
        for color in [NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown] {
            let nb = generate(&Gen::Noise { color, level: 0.0 }, r, 1.0, 1);
            let p = peak(&nb);
            assert!(p > 0.1 && p <= 1.5, "{color:?} peak {p}");
        }
        let c = generate(&Gen::Clicks { bpm: 120.0, beats: 4, level: 0.0 }, r, 2.0, 1);
        // four clicks in two seconds at 120 BPM
        let starts = (1..c.frames()).filter(|&i| c.ch[0][i - 1] == 0.0 && c.ch[0][i] != 0.0).count();
        assert!((3..=5).contains(&starts), "{starts}");
        let d = generate(&Gen::Dtmf { digits: "12#".into(), tone_ms: 100.0, gap_ms: 50.0, level: -3.0 }, r, 1.0, 1);
        assert!(peak(&d) > 0.3);
        let s = generate(&Gen::Silence, r, 0.5, 1);
        assert_eq!(peak(&s), 0.0);
        let ch = generate(&Gen::Chirp { wave: Wave::Sine, f0: 100.0, f1: 8000.0, level: -1.0, linear: false }, r, 1.0, 1);
        assert!(ch.ch[0].iter().all(|v| v.is_finite()));
    }
}
