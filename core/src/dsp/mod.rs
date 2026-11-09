//! Real-time capable DSP building blocks. Every processor works on planar blocks
//! (`&mut [Vec<f32>]`, one vector per channel) and keeps its own state.

pub mod biquad;
pub mod delay;
pub mod dynamics;
pub mod extra;
pub mod fir;
pub mod modulation;
pub mod reverb;
pub mod saturation;

/// Common interface for streaming processors.
pub trait Processor: Send {
    /// Process `n` frames in place (`buf[c][..n]`).
    fn process(&mut self, buf: &mut [Vec<f32>], n: usize);
    fn reset(&mut self) {}
    /// Gain reduction in dB for meters (dynamics only).
    fn reduction_db(&self) -> f32 {
        0.0
    }
    /// Processing delay in frames (look-ahead), compensated when rendering offline.
    fn latency(&self) -> usize {
        0
    }
}

#[inline]
pub fn ms_to_coef(ms: f32, rate: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (ms * 0.001 * rate)).exp()
    }
}

/// Dry/wet helper.
#[inline]
pub fn mix(dry: f32, wet: f32, amount: f32) -> f32 {
    dry + (wet - dry) * amount
}
