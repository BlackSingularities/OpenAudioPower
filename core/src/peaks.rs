//! Waveform overview data for drawing.

use crate::buffer::AudioBuffer;
use rayon::prelude::*;

/// Samples per peak bin of the stored overview. Finer zoom levels read raw samples.
pub const PEAK_SPP: usize = 64;

/// Per channel interleaved `[min, max, min, max, …]` for bins of `spp` samples.
pub fn compute(buf: &AudioBuffer, spp: usize) -> Vec<Vec<f32>> {
    let spp = spp.max(1);
    buf.ch
        .par_iter()
        .map(|c| {
            let bins = c.len().div_ceil(spp);
            let mut v = Vec::with_capacity(bins * 2);
            for chunk in c.chunks(spp) {
                let mut lo = f32::MAX;
                let mut hi = f32::MIN;
                for &s in chunk {
                    lo = lo.min(s);
                    hi = hi.max(s);
                }
                v.push(lo);
                v.push(hi);
            }
            v
        })
        .collect()
}

/// Binary layout sent to the UI: u32 channels, u32 bins, u32 spp, then f32 data channel after channel.
pub fn encode(peaks: &[Vec<f32>], spp: usize) -> Vec<u8> {
    let bins = peaks.first().map(|p| p.len() / 2).unwrap_or(0);
    let mut out = Vec::with_capacity(12 + peaks.len() * bins * 8);
    out.extend_from_slice(&(peaks.len() as u32).to_le_bytes());
    out.extend_from_slice(&(bins as u32).to_le_bytes());
    out.extend_from_slice(&(spp as u32).to_le_bytes());
    for p in peaks {
        for v in p {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

/// Samples per bin of the frequency-band overview used for coloured waveforms and RMS display.
pub const BANDS_SPP: usize = 256;

/// Per bin `[rms, low, mid, high]` (RMS of the mono mix and of three bands: < ~200 Hz,
/// 200 Hz – 2.5 kHz, > 2.5 kHz). Encoded as u32 bins, u32 spp, then f32 × 4 per bin.
pub fn bands(buf: &AudioBuffer) -> Vec<u8> {
    let mono = buf.mono();
    let rate = buf.rate.max(1) as f32;
    let coef = |hz: f32| (-2.0 * std::f32::consts::PI * hz / rate).exp();
    let (cl, ch) = (coef(200.0), coef(2500.0));
    let bins = mono.len().div_ceil(BANDS_SPP);
    let mut out = Vec::with_capacity(8 + bins * 16);
    out.extend_from_slice(&(bins as u32).to_le_bytes());
    out.extend_from_slice(&(BANDS_SPP as u32).to_le_bytes());
    // two cascaded one-pole low-passes for each split (12 dB/oct)
    let (mut l1, mut l2, mut h1, mut h2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for chunk in mono.chunks(BANDS_SPP) {
        let (mut s, mut sl, mut sm, mut sh) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for &x in chunk {
            l1 = x + cl * (l1 - x);
            l2 = l1 + cl * (l2 - l1);
            h1 = x + ch * (h1 - x);
            h2 = h1 + ch * (h2 - h1);
            let low = l2;
            let high = x - h2;
            let mid = x - low - high;
            s += (x * x) as f64;
            sl += (low * low) as f64;
            sm += (mid * mid) as f64;
            sh += (high * high) as f64;
        }
        let n = chunk.len().max(1) as f64;
        for v in [s, sl, sm, sh] {
            out.extend_from_slice(&((v / n).sqrt() as f32).to_le_bytes());
        }
    }
    out
}
