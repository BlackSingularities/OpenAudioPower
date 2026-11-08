//! Stem separation: neural (HT-Demucs via ONNX Runtime, 4 or 6 stems) or a fast DSP mode
//! (harmonic/percussive + centre-channel masking) that needs no download.

pub mod dsp;
pub mod models;
#[cfg(feature = "neural")]
pub mod neural;

use crate::buffer::AudioBuffer;
use crate::Cancel;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SepOptions {
    /// Model id from [`models::CATALOG`] or "dsp".
    pub engine: String,
    /// Use the GPU (DirectML) when available.
    #[serde(default = "yes")]
    pub gpu: bool,
    /// 0 = fast (25 % overlap), 1 = high (50 % overlap), 2 = best (75 % overlap + averaged shifts)
    #[serde(default)]
    pub quality: u8,
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Serialize)]
pub struct Stem {
    pub name: String,
    #[serde(skip)]
    pub buf: AudioBuffer,
}

pub fn separate(buf: &AudioBuffer, opt: &SepOptions, models_dir: &Path, progress: &dyn Fn(f32, &str), cancel: &Cancel) -> Result<(Vec<Stem>, String), String> {
    if opt.engine == "dsp" {
        return Ok((dsp::separate(buf, progress, cancel)?, "DSP".into()));
    }
    #[cfg(feature = "neural")]
    {
        let model = models::find(&opt.engine).ok_or_else(|| format!("unknown model {}", opt.engine))?;
        let path = models::path(models_dir, model);
        if !path.exists() {
            return Err("model-missing".into());
        }
        neural::separate(buf, model, &path, opt, progress, cancel)
    }
    #[cfg(not(feature = "neural"))]
    {
        let _ = models_dir;
        Err("neural engine not compiled in".into())
    }
}
