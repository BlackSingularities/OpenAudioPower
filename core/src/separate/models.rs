//! Catalogue of downloadable separation models and the downloader.

use crate::{Cancel, Progress};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModelKind {
    /// waveform to stems, STFT inside the graph
    Full,
    /// network only; STFT / iSTFT on the host (GPU capable)
    Core,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub kind: ModelKind,
    pub id: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub stems: &'static [&'static str],
    pub rate: u32,
    pub segment: usize,
    pub license: &'static str,
    pub source: &'static str,
}

/// Older single-file graphs with the STFT inside (CPU only). Kept so existing downloads
/// keep working and can be removed from the settings.
pub const LEGACY: &[Model] = &[
    Model {
        kind: ModelKind::Full,
        id: "htdemucs_6s_full",
        name: "HT-Demucs 6 (CPU)",
        file: "htdemucs_6s_fp16weights.onnx",
        url: "https://huggingface.co/Acapellas/htdemucs-6s-onnx/resolve/main/htdemucs_6s_fp16weights.onnx",
        size: 136_428_532,
        stems: &["drums", "bass", "other", "vocals", "guitar", "piano"],
        rate: 44100,
        segment: 343_980,
        license: "MIT",
        source: "huggingface.co/Acapellas/htdemucs-6s-onnx",
    },
    Model {
        kind: ModelKind::Full,
        id: "htdemucs_full",
        name: "HT-Demucs 4 (CPU)",
        file: "htdemucs_fp16weights.onnx",
        url: "https://huggingface.co/adowu/htdemucs-onnx/resolve/main/htdemucs_fp16weights.onnx",
        size: 165_612_636,
        stems: &["drums", "bass", "other", "vocals"],
        rate: 44100,
        segment: 343_980,
        license: "MIT",
        source: "huggingface.co/adowu/htdemucs-onnx",
    },
];

/// HT-Demucs network-only graphs (STFT / iSTFT on the host) - run on the GPU through
/// DirectML and about 2x faster than the full graphs on the CPU.
pub const CATALOG: &[Model] = &[
    Model {
        kind: ModelKind::Core,
        id: "htdemucs_6s",
        name: "HT-Demucs 6",
        file: "htdemucs_6s_core.onnx",
        url: "https://huggingface.co/HRSadeghi/demucs-onnx/resolve/main/htdemucs_6s.onnx",
        size: 110_396_023,
        stems: &["drums", "bass", "other", "vocals", "guitar", "piano"],
        rate: 44100,
        segment: 343_980,
        license: "CC BY-NC 4.0 (Meta)",
        source: "huggingface.co/HRSadeghi/demucs-onnx",
    },
    Model {
        kind: ModelKind::Core,
        id: "htdemucs",
        name: "HT-Demucs 4",
        file: "htdemucs_core.onnx",
        url: "https://huggingface.co/HRSadeghi/demucs-onnx/resolve/main/htdemucs.onnx",
        size: 168_683_222,
        stems: &["drums", "bass", "other", "vocals"],
        rate: 44100,
        segment: 343_980,
        license: "CC BY-NC 4.0 (Meta)",
        source: "huggingface.co/HRSadeghi/demucs-onnx",
    },
];

pub fn find(id: &str) -> Option<&'static Model> {
    CATALOG.iter().chain(LEGACY.iter()).find(|m| m.id == id)
}

pub fn path(dir: &Path, m: &Model) -> PathBuf {
    dir.join(m.file)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(flatten)]
    pub model: Model,
    pub installed: bool,
    pub path: String,
    /// older CPU-only download still on disk
    pub legacy: bool,
}

pub fn status(dir: &Path) -> Vec<ModelStatus> {
    let st = |m: &Model, legacy: bool| {
        let p = path(dir, m);
        let installed = std::fs::metadata(&p).map(|md| md.len() > 1_000_000).unwrap_or(false);
        ModelStatus { model: *m, installed, path: p.to_string_lossy().into_owned(), legacy }
    };
    CATALOG
        .iter()
        .map(|m| st(m, false))
        .chain(LEGACY.iter().map(|m| st(m, true)).filter(|s| s.installed))
        .collect()
}

pub fn download(dir: &Path, m: &Model, progress: &Progress, cancel: &Cancel) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = path(dir, m);
    let part = target.with_extension("part");
    let agent = ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(20)).build();
    let resp = agent
        .get(m.url)
        .set("User-Agent", "OpenAudioPower")
        .call()
        .map_err(|e| format!("download: {e}"))?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(m.size);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    let mut got: u64 = 0;
    loop {
        if cancel.is_set() {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err("cancelled".into());
        }
        let k = reader.read(&mut buf).map_err(|e| format!("download: {e}"))?;
        if k == 0 {
            break;
        }
        file.write_all(&buf[..k]).map_err(|e| e.to_string())?;
        got += k as u64;
        progress.set(got as f32 / total.max(1) as f32);
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if got < total.min(m.size) / 2 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("download incomplete ({got} / {total} B)"));
    }
    std::fs::rename(&part, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
