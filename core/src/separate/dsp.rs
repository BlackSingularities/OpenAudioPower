//! Fast model-free separation into vocals / drums / bass / other.
//! Harmonic–percussive median filtering plus stereo centre extraction; soft masks are
//! normalised so the stems always sum back to the original mix.

use super::Stem;
use crate::buffer::AudioBuffer;
use crate::spectral::Stft;
use crate::Cancel;
use rayon::prelude::*;
use realfft::num_complex::Complex32;

fn median(v: &mut [f32]) -> f32 {
    let m = v.len() / 2;
    v.select_nth_unstable_by(m, |a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[m]
}

pub fn separate(buf: &AudioBuffer, progress: &dyn Fn(f32, &str), cancel: &Cancel) -> Result<Vec<Stem>, String> {
    let st_buf = buf.to_stereo();
    let rate = st_buf.rate as f32;
    let n = 4096;
    let hop = 1024;
    progress(0.05, "stft");
    let specs: Vec<Vec<Vec<Complex32>>> = st_buf.ch.par_iter().map(|c| Stft::new(n, hop).forward(c)).collect();
    let (sl, sr) = (&specs[0], &specs[1]);
    let frames = sl.len();
    let bins = n / 2 + 1;
    if cancel.is_set() {
        return Err("cancelled".into());
    }
    // mono magnitude, frame-major
    let mag: Vec<Vec<f32>> = (0..frames).map(|f| (0..bins).map(|k| (sl[f][k] + sr[f][k]).norm() * 0.5).collect()).collect();
    progress(0.25, "hpss");
    let half_t = 8; // 17 frames
    let half_f = 8; // 17 bins
    // harmonic: median across time, computed per bin
    let harm_t: Vec<Vec<f32>> = (0..bins)
        .into_par_iter()
        .map(|k| {
            let mut col = vec![0.0f32; 2 * half_t + 1];
            (0..frames)
                .map(|f| {
                    let lo = f.saturating_sub(half_t);
                    let hi = (f + half_t + 1).min(frames);
                    let w = &mut col[..hi - lo];
                    for (i, j) in (lo..hi).enumerate() {
                        w[i] = mag[j][k];
                    }
                    median(w)
                })
                .collect()
        })
        .collect();
    if cancel.is_set() {
        return Err("cancelled".into());
    }
    progress(0.55, "hpss");
    let perc: Vec<Vec<f32>> = (0..frames)
        .into_par_iter()
        .map(|f| {
            let mut win = vec![0.0f32; 2 * half_f + 1];
            (0..bins)
                .map(|k| {
                    let lo = k.saturating_sub(half_f);
                    let hi = (k + half_f + 1).min(bins);
                    let w = &mut win[..hi - lo];
                    w.copy_from_slice(&mag[f][lo..hi]);
                    median(w)
                })
                .collect()
        })
        .collect();
    progress(0.7, "masks");
    let bin_hz = rate / n as f32;
    let smooth = |f: f32, lo: f32, hi: f32| -> f32 { ((f - lo) / (hi - lo)).clamp(0.0, 1.0) };
    // masks [stem][frame][bin]: 0 vocals 1 drums 2 bass 3 other
    let mut out_l: Vec<Vec<Vec<Complex32>>> = vec![vec![vec![Complex32::new(0.0, 0.0); bins]; frames]; 4];
    let mut out_r = out_l.clone();
    for f in 0..frames {
        for k in 0..bins {
            let hz = k as f32 * bin_hz;
            let h = harm_t[k][f];
            let p = perc[f][k];
            let hm = h * h / (h * h + p * p + 1e-12);
            let pm = 1.0 - hm;
            let l = sl[f][k];
            let r = sr[f][k];
            let (la, ra) = (l.norm(), r.norm());
            let centre = 1.0 - ((l - r).norm() / (la + ra + 1e-9)).min(1.0);
            let low = 1.0 - smooth(hz, 160.0, 320.0);
            let voice_band = smooth(hz, 120.0, 220.0) * (1.0 - smooth(hz, 9000.0, 14000.0));
            let mut bass = hm * low;
            let mut vocals = hm * (1.0 - low) * centre.powi(6) * voice_band;
            let mut drums = pm * (1.0 - 0.6 * low * hm);
            let sum = bass + vocals + drums;
            if sum > 1.0 {
                bass /= sum;
                vocals /= sum;
                drums /= sum;
            }
            let other = (1.0 - bass - vocals - drums).max(0.0);
            let m = [vocals, drums, bass, other];
            for s in 0..4 {
                out_l[s][f][k] = l * m[s];
                out_r[s][f][k] = r * m[s];
            }
        }
    }
    if cancel.is_set() {
        return Err("cancelled".into());
    }
    progress(0.85, "istft");
    let len = st_buf.frames();
    let names = ["vocals", "drums", "bass", "other"];
    let stems: Vec<Stem> = (0..4)
        .into_par_iter()
        .map(|s| {
            let mut st = Stft::new(n, hop);
            let l = st.inverse(&out_l[s], len);
            let r = st.inverse(&out_r[s], len);
            Stem { name: names[s].to_string(), buf: AudioBuffer::from_channels(st_buf.rate, vec![l, r]) }
        })
        .collect();
    progress(1.0, "DSP");
    Ok(stems)
}
