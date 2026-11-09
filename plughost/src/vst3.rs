//! VST3 plug-ins through the `vst3-host` crate (in-process here – this whole executable is the
//! isolation boundary). Editor edits reach the audio processor through the crate's component
//! handler.

use crate::PluginDesc;
use parking_lot::Mutex;
use std::path::Path;
use vst3_host::{AudioBuffers, Plugin, Vst3Host, WindowHandle};

pub fn describe(path: &Path) -> Result<Vec<PluginDesc>, String> {
    let info = vst3_host::discovery::get_plugin_info(path).map_err(|e| e.to_string())?;
    let instrument = info.category.to_ascii_lowercase().contains("instrument");
    Ok(vec![PluginDesc {
        format: "vst3".into(),
        path: path.to_string_lossy().into_owned(),
        id: info.uid,
        name: info.name,
        vendor: info.vendor,
        version: info.version,
        category: info.category,
        instrument,
    }])
}

pub struct Vst3Plugin {
    plugin: Mutex<Plugin>,
    buffers: Mutex<AudioBuffers>,
    in_ch: usize,
    out_ch: usize,
    pub name: String,
    _host: Vst3Host,
}

impl Vst3Plugin {
    pub fn new(path: &Path, class_id: &str, rate: f64, block: usize) -> Result<Self, String> {
        let mut host = Vst3Host::builder().with_process_isolation(false).sample_rate(rate).block_size(block).build().map_err(|e| e.to_string())?;
        let mut plugin = if class_id.len() == 32 { host.load_plugin_class(path, class_id) } else { host.load_plugin(path) }.map_err(|e| e.to_string())?;
        let info = plugin.info().clone();
        let in_ch = (info.audio_inputs as usize).min(2);
        let out_ch = (info.audio_outputs as usize).clamp(1, 2);
        plugin.start_processing().map_err(|e| e.to_string())?;
        Ok(Self {
            plugin: Mutex::new(plugin),
            buffers: Mutex::new(AudioBuffers::new(in_ch.max(1), out_ch, block, rate)),
            in_ch,
            out_ch,
            name: info.name,
            _host: host,
        })
    }

    pub fn latency(&self) -> u32 {
        self.plugin.lock().latency_samples()
    }

    pub fn process(&self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32], n: usize) {
        let mut guard = self.buffers.lock();
        let b = &mut *guard;
        b.block_size = n;
        for c in b.inputs.iter_mut() {
            c.resize(n, 0.0);
        }
        for c in b.outputs.iter_mut() {
            c.resize(n, 0.0);
        }
        match self.in_ch {
            0 => {}
            1 => {
                for i in 0..n {
                    b.inputs[0][i] = 0.5 * (l[i] + r[i]);
                }
            }
            _ => {
                b.inputs[0][..n].copy_from_slice(&l[..n]);
                b.inputs[1][..n].copy_from_slice(&r[..n]);
            }
        }
        let ok = self.plugin.lock().process_audio(b).is_ok();
        if !ok {
            ol[..n].copy_from_slice(&l[..n]);
            or[..n].copy_from_slice(&r[..n]);
            return;
        }
        ol[..n].copy_from_slice(&b.outputs[0][..n]);
        or[..n].copy_from_slice(&b.outputs[self.out_ch - 1][..n]);
    }

    pub fn reset(&self) {
        let mut p = self.plugin.lock();
        let _ = p.stop_processing();
        let _ = p.start_processing();
    }

    pub fn idle(&self) {
        let _ = self.plugin.lock().service_host_requests();
    }

    pub fn has_editor(&self) -> bool {
        self.plugin.lock().has_editor()
    }

    pub fn open_editor(&self, hwnd: *mut std::ffi::c_void) -> Result<(u32, u32), String> {
        let mut p = self.plugin.lock();
        let (w, h) = p.get_editor_size().unwrap_or((640, 400));
        p.open_editor(unsafe { WindowHandle::from_raw(hwnd) }).map_err(|e| e.to_string())?;
        let (w, h) = p.attached_editor_size().ok().flatten().unwrap_or((w, h));
        Ok((w.max(50) as u32, h.max(50) as u32))
    }

    pub fn set_editor_size(&self, w: u32, h: u32) -> Option<(u32, u32)> {
        let mut p = self.plugin.lock();
        if !p.editor_can_resize() {
            return None;
        }
        p.resize_editor(w as i32, h as i32).ok().map(|(a, b)| (a as u32, b as u32))
    }

    pub fn take_resize_request(&self) -> Option<(u32, u32)> {
        self.plugin.lock().take_editor_resize_request().map(|(w, h)| (w as u32, h as u32))
    }

    pub fn close_editor(&self) {
        let _ = self.plugin.lock().close_editor();
    }

    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        self.plugin.lock().save_state().map_err(|e| e.to_string())
    }

    pub fn load_state(&self, data: &[u8]) -> Result<(), String> {
        self.plugin.lock().load_state(data).map_err(|e| e.to_string())
    }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        let mut p = self.plugin.lock();
        let _ = p.close_editor();
        let _ = p.stop_processing();
    }
}
