//! Command-line check of decoding + separation:
//! cargo run --release -p openaudiopower-core --example separate -- <input> <models_dir> <out_dir> [engine] [cpu]

use openaudiopower_core::encode::{write, EncodeOptions, Format};
use openaudiopower_core::separate::{models, separate, SepOptions};
use openaudiopower_core::{decode, Cancel, Progress};
use std::path::PathBuf;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let input = PathBuf::from(&a[0]);
    let mdir = PathBuf::from(&a[1]);
    let out = PathBuf::from(&a[2]);
    let engine = a.get(3).cloned().unwrap_or_else(|| "htdemucs_6s".into());
    let gpu = a.get(4).map(|s| s != "cpu").unwrap_or(true);
    let cancel = Cancel::default();
    let t = std::time::Instant::now();
    let (buf, meta) = decode::decode_file(&input, &cancel, &|_| {}).expect("decode");
    println!("decoded {:?} {} Hz {} ch {:.1}s in {:.2}s", meta.title, buf.rate, buf.channels(), buf.duration(), t.elapsed().as_secs_f32());
    let a = openaudiopower_core::analysis::analyze(&buf);
    println!("analysis: LUFS {:.1}, peak {:.1} dB, BPM {} ({:.2}), key {} ({:.2})", a.lufs, a.peak_db, a.bpm, a.bpm_confidence, a.key, a.key_confidence);
    if let Some(m) = models::find(&engine) {
        if !models::path(&mdir, m).exists() {
            println!("downloading {} …", m.file);
            let p = Progress::default();
            models::download(&mdir, m, &p, &cancel).expect("download");
        }
    }
    let t = std::time::Instant::now();
    let opt = SepOptions { engine: engine.clone(), gpu, quality: 0 };
    let last = std::cell::Cell::new(-1i32);
    let (stems, dev) = separate(&buf, &opt, &mdir, &|f, s| {
        let p = (f * 20.0) as i32;
        if p != last.get() {
            last.set(p);
            println!("  {:3.0}% {s}", f * 100.0);
        }
    }, &cancel)
    .expect("separate");
    println!("separated on {dev} in {:.1}s", t.elapsed().as_secs_f32());
    // energy check: stems should sum back close to the mix
    let n = buf.frames();
    let mut err = 0.0f64;
    let mut sig = 0.0f64;
    let st = buf.to_stereo();
    for c in 0..2 {
        for i in (0..n).step_by(7) {
            let s: f32 = stems.iter().map(|x| x.buf.ch[c][i]).sum();
            err += ((s - st.ch[c][i]) as f64).powi(2);
            sig += (st.ch[c][i] as f64).powi(2);
        }
    }
    println!("reconstruction SNR {:.1} dB", 10.0 * (sig / err.max(1e-12)).log10());
    for s in &stems {
        let rms = openaudiopower_core::analysis::rms(&s.buf);
        println!("  {:8} rms {:.1} dB", s.name, openaudiopower_core::buffer::gain_to_db(rms));
        write(&out.join(format!("{}.flac", s.name)), &s.buf, &EncodeOptions { format: Format::Flac, bits: 16, dither: true, bitrate: 0 }).expect("write");
    }
}
