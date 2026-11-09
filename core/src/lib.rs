//! OpenAudioPower engine – everything that does not need a window: decoding, encoding,
//! resampling, DSP effects, analysis, multitrack mixing and stem separation.

pub mod analysis;
pub mod buffer;
pub mod decode;
pub mod dsp;
pub mod effects;
pub mod encode;
pub mod enhance;
pub mod generate;
pub mod mix;
pub mod ops;
pub mod peaks;
pub mod repair;
pub mod resample;
pub mod separate;
pub mod spectral;
pub mod stretch;

pub use buffer::{AudioBuffer, Meta};

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Cooperative cancellation flag shared between UI and worker.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn set(&self) {
        self.0.store(true, Ordering::Relaxed)
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed)
    }
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Progress 0..1 readable from another thread.
#[derive(Clone, Default)]
pub struct Progress(Arc<AtomicU32>);
impl Progress {
    pub fn set(&self, f: f32) {
        self.0.store(f.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed)
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}
