//! High quality sample-rate conversion (windowed sinc, rubato).

use crate::buffer::AudioBuffer;
use rayon::prelude::*;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// Converts one channel by an arbitrary ratio `out_rate / in_rate` (also used for varispeed).
pub fn resample_channel(x: &[f32], ratio: f64) -> Vec<f32> {
    if (ratio - 1.0).abs() < 1e-9 || x.is_empty() {
        return x.to_vec();
    }
    let n = x.len();
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk = 8192;
    let mut r = match SincFixedIn::<f32>::new(ratio, 1.0, params, chunk, 1) {
        Ok(r) => r,
        Err(_) => return linear(x, ratio),
    };
    let delay = r.output_delay();
    let expected = (n as f64 * ratio).round() as usize;
    let mut out: Vec<f32> = Vec::with_capacity(expected + delay + chunk);
    let mut pos = 0;
    while pos < n {
        let need = r.input_frames_next();
        let end = (pos + need).min(n);
        let block = [&x[pos..end]];
        let res = if end - pos == need { r.process(&block, None) } else { r.process_partial(Some(&block), None) };
        match res {
            Ok(o) => out.extend_from_slice(&o[0]),
            Err(_) => return linear(x, ratio),
        }
        pos = end;
    }
    let mut guard = 0;
    while out.len() < expected + delay && guard < 64 {
        guard += 1;
        match r.process_partial(None::<&[Vec<f32>]>, None) {
            Ok(o) => {
                if o[0].is_empty() {
                    break;
                }
                out.extend_from_slice(&o[0]);
            }
            Err(_) => break,
        }
    }
    let d = delay.min(out.len());
    out.drain(..d);
    out.resize(expected, 0.0);
    out
}

fn linear(x: &[f32], ratio: f64) -> Vec<f32> {
    let m = (x.len() as f64 * ratio).round() as usize;
    (0..m)
        .map(|i| {
            let p = i as f64 / ratio;
            let j = p.floor() as usize;
            let f = (p - j as f64) as f32;
            let a = x[j.min(x.len() - 1)];
            let b = x[(j + 1).min(x.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

pub fn resample_ratio(ch: &[Vec<f32>], ratio: f64) -> Vec<Vec<f32>> {
    ch.par_iter().map(|c| resample_channel(c, ratio)).collect()
}

pub fn resample(buf: &AudioBuffer, rate: u32) -> AudioBuffer {
    if buf.rate == rate || buf.rate == 0 {
        let mut b = buf.clone();
        b.rate = rate;
        return b;
    }
    let ratio = rate as f64 / buf.rate as f64;
    AudioBuffer::from_channels(rate, resample_ratio(&buf.ch, ratio))
}
