//! Application state: the registry of immutable audio sources, the engine and job tracking.

use crate::engine::Engine;
use openaudiopower_core::buffer::{AudioBuffer, Meta};
use openaudiopower_core::mix::Sources;
use openaudiopower_core::peaks;
use openaudiopower_core::Cancel;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct SourceEntry {
    pub id: u64,
    pub name: String,
    pub path: Option<String>,
    pub kind: String,
    pub parent: Option<u64>,
    pub stem: Option<String>,
    pub meta: Meta,
    pub buf: Arc<AudioBuffer>,
    pub peaks: Arc<Vec<Vec<f32>>>,
    /// File this source was last saved to inside a project's audio folder (skip re-encoding).
    pub saved_as: Mutex<Option<PathBuf>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: u64,
    pub name: String,
    pub path: Option<String>,
    pub kind: String,
    pub parent: Option<u64>,
    pub stem: Option<String>,
    pub rate: u32,
    pub channels: usize,
    pub frames: usize,
    pub duration: f64,
    pub meta: Meta,
    pub peak: f32,
}

impl SourceEntry {
    pub fn info(&self) -> SourceInfo {
        SourceInfo {
            id: self.id,
            name: self.name.clone(),
            path: self.path.clone(),
            kind: self.kind.clone(),
            parent: self.parent,
            stem: self.stem.clone(),
            rate: self.buf.rate,
            channels: self.buf.channels(),
            frames: self.buf.frames(),
            duration: self.buf.duration(),
            meta: self.meta.clone(),
            peak: self.buf.peak(),
        }
    }
}

pub struct App {
    pub engine: Arc<Engine>,
    pub sources: RwLock<HashMap<u64, Arc<SourceEntry>>>,
    next_id: AtomicU64,
    pub cancel: Mutex<Cancel>,
    pub models_dir: PathBuf,
    pub config_dir: PathBuf,
    pub startup: Vec<String>,
}

pub struct NewSource {
    pub name: String,
    pub path: Option<String>,
    pub kind: &'static str,
    pub parent: Option<u64>,
    pub stem: Option<String>,
    pub meta: Meta,
    pub buf: AudioBuffer,
}

impl App {
    pub fn new(engine: Arc<Engine>, models_dir: PathBuf, config_dir: PathBuf, startup: Vec<String>) -> Self {
        Self { engine, sources: RwLock::new(HashMap::new()), next_id: AtomicU64::new(1), cancel: Mutex::new(Cancel::default()), models_dir, config_dir, startup }
    }

    pub fn rate(&self) -> u32 {
        self.engine.rate
    }

    pub fn reserve_ids(&self, above: u64) {
        self.next_id.fetch_max(above + 1, Ordering::Relaxed);
    }

    pub fn add(&self, s: NewSource) -> SourceInfo {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.add_with_id(id, s)
    }

    pub fn add_with_id(&self, id: u64, s: NewSource) -> SourceInfo {
        self.reserve_ids(id);
        let pk = peaks::compute(&s.buf, peaks::PEAK_SPP);
        let e = Arc::new(SourceEntry {
            id,
            name: s.name,
            path: s.path,
            kind: s.kind.to_string(),
            parent: s.parent,
            stem: s.stem,
            meta: s.meta,
            buf: Arc::new(s.buf),
            peaks: Arc::new(pk),
            saved_as: Mutex::new(None),
        });
        let info = e.info();
        self.sources.write().insert(id, e);
        self.push_sources();
        info
    }

    pub fn get(&self, id: u64) -> Result<Arc<SourceEntry>, String> {
        self.sources.read().get(&id).cloned().ok_or_else(|| format!("source {id} not found"))
    }

    pub fn remove(&self, ids: &[u64]) {
        {
            let mut s = self.sources.write();
            for id in ids {
                s.remove(id);
            }
        }
        self.push_sources();
    }

    pub fn push_sources(&self) {
        let map: Sources = self.sources.read().iter().map(|(k, v)| (*k, v.buf.clone())).collect();
        self.engine.set_sources(Arc::new(map));
    }

    /// Fresh cancel token for a new job.
    pub fn new_job(&self) -> Cancel {
        let c = Cancel::default();
        *self.cancel.lock() = c.clone();
        c
    }
}
