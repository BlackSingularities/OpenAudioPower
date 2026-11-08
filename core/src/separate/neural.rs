//! HT-Demucs inference through ONNX Runtime (DirectML GPU with CPU fallback).
//! Long audio is processed in overlapping 7.8 s segments and cross-faded (overlap-add).
//!
//! Two graph flavours are supported:
//! * `Full` - waveform in, stems out (STFT inside the graph; CPU only in practice, DirectML
//!   cannot run its huge ConvTranspose iSTFT),
//! * `Core` - only the encoder / transformer / decoder; the STFT and iSTFT run here in Rust
//!   exactly like the Demucs `_spec` / `_ispec`, which lets the network run on the GPU.

use super::models::{Model, ModelKind};
use super::{SepOptions, Stem};
use crate::buffer::AudioBuffer;
use crate::resample::resample;
use crate::Cancel;
use ort::execution_providers::{CPUExecutionProvider, DirectMLExecutionProvider};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once DirectML failed on this machine – later runs go straight to the CPU.
pub static DML_BROKEN: AtomicBool = AtomicBool::new(false);

fn session(path: &Path, gpu: bool) -> Result<Session, String> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let mut b = Session::builder()
        .map_err(|e| e.to_string())?
        .with_optimization_level(match std::env::var("OAP_DML_OPT").ok().as_deref() {
            Some("0") => GraphOptimizationLevel::Disable,
            Some("1") => GraphOptimizationLevel::Level1,
            Some("2") => GraphOptimizationLevel::Level2,
            _ if gpu => GraphOptimizationLevel::Level1,
            _ => GraphOptimizationLevel::Level3,
        })
        .map_err(|e| e.to_string())?
        .with_intra_threads(threads)
        .map_err(|e| e.to_string())?;
    if gpu {
        // DirectML needs sequential execution and no memory pattern
        b = b
            .with_memory_pattern(false)
            .map_err(|e| e.to_string())?
            .with_parallel_execution(false)
            .map_err(|e| e.to_string())?
            .with_execution_providers([DirectMLExecutionProvider::default().build().error_on_failure()])
            .map_err(|e| e.to_string())?;
    } else {
        b = b.with_execution_providers([CPUExecutionProvider::default().build()]).map_err(|e| e.to_string())?;
    }
    b.commit_from_file(path).map_err(|e| e.to_string())
}

// ───────────────────────── Demucs-compatible STFT ─────────────────────────

const NFFT: usize = 4096;
const HOP: usize = 1024;
const BINS: usize = NFFT / 2; // the Nyquist bin is dropped
const PAD: usize = HOP / 2 * 3;

/// Reflect padding like torch `F.pad(mode="reflect")` (the edge sample is not repeated).
fn reflect(x: &[f32], left: usize, right: usize) -> Vec<f32> {
    let n = x.len() as isize;
    let at = |i: isize| -> f32 {
        if n == 1 {
            return x[0];
        }
        let period = 2 * (n - 1);
        let mut j = i.rem_euclid(period);
        if j >= n {
            j = period - j;
        }
        x[j as usize]
    };
    (-(left as isize)..(n + right as isize)).map(at).collect()
}

pub struct DemucsStft {
    win: Vec<f32>,
    fwd: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    inv: std::sync::Arc<dyn realfft::ComplexToReal<f32>>,
}

impl DemucsStft {
    pub fn new() -> Self {
        let mut p = realfft::RealFftPlanner::<f32>::new();
        // periodic Hann (torch.hann_window default)
        let win = (0..NFFT).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / NFFT as f32).cos()).collect();
        Self { win, fwd: p.plan_fft_forward(NFFT), inv: p.plan_fft_inverse(NFFT) }
    }

    /// HTDemucs `_spec` for one channel; writes `[F, T]` real and imaginary planes.
    pub fn spec(&self, x: &[f32], frames: usize, re: &mut [f32], im: &mut [f32]) {
        let len = x.len();
        let le = len.div_ceil(HOP);
        debug_assert_eq!(le, frames);
        let padded = reflect(x, PAD, PAD + le * HOP - len);
        // torch.stft(center=True) reflect-pads n_fft/2 on both sides
        let centred = reflect(&padded, NFFT / 2, NFFT / 2);
        let scale = 1.0 / (NFFT as f32).sqrt(); // normalized=True
        let mut inp = self.fwd.make_input_vec();
        let mut out = self.fwd.make_output_vec();
        for t in 0..frames {
            let k = t + 2; // z[..., 2 : 2 + le]
            let s0 = k * HOP;
            for i in 0..NFFT {
                inp[i] = centred[s0 + i] * self.win[i];
            }
            let _ = self.fwd.process(&mut inp, &mut out);
            for f in 0..BINS {
                re[f * frames + t] = out[f].re * scale;
                im[f * frames + t] = out[f].im * scale;
            }
        }
    }

    /// HTDemucs `_ispec` for one channel: complex `[F, T]` back to `length` samples.
    pub fn ispec(&self, re: &[f32], im: &[f32], frames: usize, length: usize) -> Vec<f32> {
        // the Nyquist bin is added back as zero and two empty frames on each side
        let total_frames = frames + 4;
        let ola_len = (total_frames - 1) * HOP + NFFT;
        let mut ola = vec![0.0f32; ola_len];
        let mut env = vec![0.0f32; ola_len];
        let mut spec = self.inv.make_input_vec();
        let mut time = self.inv.make_output_vec();
        let scale = (NFFT as f32).sqrt() / NFFT as f32; // normalized=True, irfft 1/n
        for k in 0..total_frames {
            let s0 = k * HOP;
            for i in 0..NFFT {
                let w = self.win[i];
                env[s0 + i] += w * w;
            }
            if k < 2 || k >= frames + 2 {
                continue;
            }
            let t = k - 2;
            for f in 0..BINS {
                spec[f].re = re[f * frames + t];
                spec[f].im = im[f * frames + t];
            }
            spec[0].im = 0.0;
            spec[BINS].re = 0.0;
            spec[BINS].im = 0.0;
            let _ = self.inv.process(&mut spec, &mut time);
            for i in 0..NFFT {
                ola[s0 + i] += time[i] * scale * self.win[i];
            }
        }
        // center=True drops n_fft/2, then x[pad : pad + length]
        let off = NFFT / 2 + PAD;
        (0..length)
            .map(|i| {
                let j = off + i;
                if j < ola_len && env[j] > 1e-11 { ola[j] / env[j] } else { 0.0 }
            })
            .collect()
    }
}

impl Default for DemucsStft {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs one segment (`chunk` = 2 x seg, already normalised); returns stems x 2 x seg.
fn run_segment(s: &mut Session, model: &Model, stft: &DemucsStft, chunk: &[f32], seg: usize) -> Result<Vec<f32>, String> {
    let k = model.stems.len();
    match model.kind {
        ModelKind::Full => {
            let input = Tensor::from_array(([1usize, 2, seg], chunk.to_vec().into_boxed_slice())).map_err(|e| e.to_string())?;
            let outputs = s.run(ort::inputs!["mix" => input]).map_err(|e| e.to_string())?;
            let (shape, data) = outputs["stems"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            if data.len() != k * 2 * seg {
                return Err(format!("unexpected output shape {shape:?}"));
            }
            Ok(data.to_vec())
        }
        ModelKind::Core => {
            let frames = seg.div_ceil(HOP);
            let plane = BINS * frames;
            let mut mag = vec![0.0f32; 4 * plane];
            for c in 0..2 {
                let (a, b) = mag[c * 2 * plane..(c * 2 + 2) * plane].split_at_mut(plane);
                stft.spec(&chunk[c * seg..(c + 1) * seg], frames, a, b);
            }
            let mix = Tensor::from_array(([1usize, 2, seg], chunk.to_vec().into_boxed_slice())).map_err(|e| e.to_string())?;
            let magt = Tensor::from_array(([1usize, 4, BINS, frames], mag.into_boxed_slice())).map_err(|e| e.to_string())?;
            let outputs = s.run(ort::inputs!["mix" => mix, "mag" => magt]).map_err(|e| e.to_string())?;
            let (sshape, spec) = outputs["spec"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (wshape, wav) = outputs["wav"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            if spec.len() != k * 4 * plane || wav.len() != k * 2 * seg {
                return Err(format!("unexpected output shapes {sshape:?} / {wshape:?}"));
            }
            use rayon::prelude::*;
            let mut res = wav.to_vec();
            let parts: Vec<(usize, Vec<f32>)> = (0..k * 2)
                .into_par_iter()
                .map(|sc| {
                    let (st, c) = (sc / 2, sc % 2);
                    let base = (st * 4 + c * 2) * plane;
                    (sc, stft.ispec(&spec[base..base + plane], &spec[base + plane..base + 2 * plane], frames, seg))
                })
                .collect();
            for (sc, x) in parts {
                for (o, v) in res[sc * seg..(sc + 1) * seg].iter_mut().zip(x) {
                    *o += v;
                }
            }
            Ok(res)
        }
    }
}

pub fn separate(buf: &AudioBuffer, model: &Model, path: &Path, opt: &SepOptions, progress: &dyn Fn(f32, &str), cancel: &Cancel) -> Result<(Vec<Stem>, String), String> {
    progress(0.0, "load");
    let orig_rate = buf.rate;
    let stereo = buf.to_stereo();
    let x = if stereo.rate != model.rate { resample(&stereo, model.rate) } else { stereo };
    let total = x.frames();
    let seg = model.segment;
    let k = model.stems.len();

    // Demucs normalisation: zero-mean, unit-variance of the mono mix
    let mono = x.mono();
    let mean = mono.iter().map(|v| *v as f64).sum::<f64>() / total.max(1) as f64;
    let var = mono.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / total.max(1) as f64;
    let std = var.sqrt().max(1e-8);
    let (mean, std) = (mean as f32, std as f32);

    let use_gpu = opt.gpu && model.kind == ModelKind::Core && !DML_BROKEN.load(Ordering::Relaxed);
    let mut device: &str = if use_gpu { "DirectML (GPU)" } else { "CPU" };
    let mut gpu_error = String::new();
    let mut sess = match session(path, use_gpu) {
        Ok(s) => s,
        Err(e) if use_gpu => {
            DML_BROKEN.store(true, Ordering::Relaxed);
            gpu_error = e;
            device = "CPU";
            session(path, false)?
        }
        Err(e) => return Err(e),
    };

    let overlap_frac = match opt.quality {
        0 => 0.25,
        1 => 0.5,
        _ => 0.75,
    };
    let overlap = (seg as f32 * overlap_frac) as usize;
    let stride = seg - overlap;
    let chunks = total.saturating_sub(overlap).div_ceil(stride).max(1);
    // triangular-trapezoid window
    let fade = overlap.max(1);
    let window: Vec<f32> = (0..seg)
        .map(|i| {
            let a = ((i + 1) as f32 / fade as f32).min(1.0);
            let b = ((seg - i) as f32 / fade as f32).min(1.0);
            a.min(b).max(1e-3)
        })
        .collect();
    let mut out = vec![vec![vec![0.0f32; total]; 2]; k];
    let mut weight = vec![0.0f32; total];
    let mut chunk = vec![0.0f32; 2 * seg];
    let stft = DemucsStft::new();
    progress(0.02, device);
    for ci in 0..chunks {
        if cancel.is_set() {
            return Err("cancelled".into());
        }
        let start = ci * stride;
        let end = (start + seg).min(total);
        let len = end - start;
        chunk.iter_mut().for_each(|v| *v = 0.0);
        for c in 0..2 {
            for i in 0..len {
                chunk[c * seg + i] = (x.ch[c][start + i] - mean) / std;
            }
        }
        let res = match run_segment(&mut sess, model, &stft, &chunk, seg) {
            Ok(r) => r,
            Err(e) if device != "CPU" => {
                // GPU failed mid-way (driver, memory) – continue on CPU
                DML_BROKEN.store(true, Ordering::Relaxed);
                gpu_error = e;
                device = "CPU";
                sess = session(path, false)?;
                run_segment(&mut sess, model, &stft, &chunk, seg)?
            }
            Err(e) => return Err(e),
        };
        for s in 0..k {
            for c in 0..2 {
                let base = (s * 2 + c) * seg;
                let o = &mut out[s][c];
                for i in 0..len {
                    o[start + i] += (res[base + i] * std + mean) * window[i];
                }
            }
        }
        for i in 0..len {
            weight[start + i] += window[i];
        }
        progress(0.02 + 0.93 * (ci + 1) as f32 / chunks as f32, device);
    }
    for s in out.iter_mut() {
        for c in s.iter_mut() {
            for (v, w) in c.iter_mut().zip(weight.iter()) {
                *v /= w.max(1e-6);
            }
        }
    }
    let mut stems = Vec::with_capacity(k);
    for (i, ch) in out.into_iter().enumerate() {
        let b = AudioBuffer::from_channels(model.rate, ch);
        let b = if orig_rate != model.rate { resample(&b, orig_rate) } else { b };
        stems.push(Stem { name: model.stems[i].to_string(), buf: b });
    }
    progress(1.0, device);
    let dev = if gpu_error.is_empty() { device.to_string() } else { format!("{device} · GPU: {gpu_error}") };
    Ok((stems, dev))
}
