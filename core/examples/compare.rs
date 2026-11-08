//! Compares the full ONNX graph with the core graph + host STFT (CPU and DirectML):
//! cargo run --release --example compare -- <input> <models_dir> [seconds]

use openaudiopower_core::separate::{separate, SepOptions, Stem};
use openaudiopower_core::{decode, Cancel};
use std::path::PathBuf;

fn snr(a: &[Stem], b: &[Stem]) -> Vec<(String, f64)> {
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let (mut s, mut e) = (0.0f64, 0.0f64);
            for c in 0..2 {
                for (p, q) in x.buf.ch[c].iter().zip(&y.buf.ch[c]) {
                    s += (*p as f64).powi(2);
                    e += (*p as f64 - *q as f64).powi(2);
                }
            }
            (x.name.clone(), 10.0 * (s / e.max(1e-20)).log10())
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let input = PathBuf::from(&a[0]);
    let mdir = PathBuf::from(&a[1]);
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(40.0);
    let cancel = Cancel::default();
    let (buf, _) = decode::decode_file(&input, &cancel, &|_| {}).expect("decode");
    let start = (buf.rate as f64 * 30.0) as usize;
    let buf = buf.slice(start, start + (buf.rate as f64 * secs) as usize);
    println!("{:.1} s of audio", buf.duration());

    let run = |engine: &str, gpu: bool| {
        let t = std::time::Instant::now();
        let (stems, dev) = separate(&buf, &SepOptions { engine: engine.into(), gpu, quality: 0 }, &mdir, &|_, _| {}, &cancel).expect(engine);
        let dt = t.elapsed().as_secs_f64();
        println!("{engine:<18} gpu={gpu:<5} -> {dev:<40} {dt:6.2} s  ({:.1}x realtime)", buf.duration() / dt);
        stems
    };
    let full = run("htdemucs_6s_full", false);
    let core_cpu = run("htdemucs_6s", false);
    let core_gpu = run("htdemucs_6s", true);
    // second GPU run: steady state without session warm-up
    let _ = run("htdemucs_6s", true);
    println!("\nSNR core(CPU) vs full(CPU):");
    for (n, v) in snr(&full, &core_cpu) {
        println!("  {n:<8} {v:6.1} dB");
    }
    println!("SNR core(GPU) vs full(CPU):");
    for (n, v) in snr(&full, &core_gpu) {
        println!("  {n:<8} {v:6.1} dB");
    }
}
