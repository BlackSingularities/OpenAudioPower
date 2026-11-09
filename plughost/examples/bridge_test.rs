//! End-to-end check of the plug-in bridge, the way the app drives it:
//! `cargo run --release -p oap-plughost --example bridge_test -- <plugin.clap|vst3> <id> [--editor]`
//!
//! 1. starts `oap-plughost run`, 2. sends a block of a sine through shared memory and prints the
//! output level, 3. saves and restores the state, 4. (test plug-in only) loads the `CRASH`
//! state and checks that the next block is answered by nobody and the helper is gone.

#[path = "../src/shm.rs"]
mod shm;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let path = &a[0];
    let id = a.get(1).cloned().unwrap_or_default();
    let editor = a.iter().any(|x| x == "--editor");
    let format = if path.to_lowercase().ends_with(".vst3") { "vst3" } else { "clap" };
    let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("oap-plughost.exe");
    let name = format!("Local\\oap-bridge-test-{}", std::process::id());
    let sh = shm::Shared::create(&name).expect("shm");

    let mut child = Command::new(&exe)
        .args(["run", format, path, &id, "48000", "512", &name, &std::process::id().to_string(), "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let mut stdin = child.stdin.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let out = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let ready = rx.recv_timeout(Duration::from_secs(30)).expect("no answer");
    println!("helper: {ready}");
    assert!(ready.contains("\"ready\""), "plug-in failed to start");

    let mut seq = 0u32;
    let mut block = |gain_in: f32| -> Option<f32> {
        seq += 1;
        let n = 4800;
        unsafe {
            for c in 0..2 {
                for (i, v) in sh.buf(false, c)[..n].iter_mut().enumerate() {
                    *v = gain_in * (i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin();
                }
            }
        }
        let h = sh.header();
        h.frames.store(n as u32, Ordering::Release);
        h.seq_in.store(seq, Ordering::Release);
        sh.signal(sh.ev_in);
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(1500) {
            if sh.wait(sh.ev_out, 100) && sh.seq_out() == seq {
                let o = unsafe { &sh.buf(true, 0)[..n] };
                return Some(o.iter().fold(0.0f32, |m, x| m.max(x.abs())));
            }
        }
        None
    };
    let t = Instant::now();
    let peak = block(0.8).expect("no audio back");
    println!("audio: in peak 0.80 → out peak {peak:.3} ({:.1} ms round trip incl. first call)", t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    for _ in 0..100 {
        block(0.8).expect("no audio back");
    }
    println!("audio: 100 blocks of 100 ms in {:.1} ms ({:.0} µs per block)", t.elapsed().as_secs_f64() * 1000.0, t.elapsed().as_secs_f64() * 1e4);

    writeln!(stdin, r#"{{"cmd":"getState","req":1}}"#).unwrap();
    let st = rx.recv_timeout(Duration::from_secs(10)).expect("no state");
    println!("state: {}", &st[..st.len().min(120)]);
    let data = st.split("\"data\":\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
    writeln!(stdin, r#"{{"cmd":"setState","req":2,"data":"{data}"}}"#).unwrap();
    println!("restore: {}", rx.recv_timeout(Duration::from_secs(10)).expect("no answer"));

    if editor {
        writeln!(stdin, r#"{{"cmd":"editor"}}"#).unwrap();
        println!("editor: {}", rx.recv_timeout(Duration::from_secs(10)).unwrap_or_default());
        println!("editor open for 6 s…");
        std::thread::sleep(Duration::from_secs(6));
        writeln!(stdin, r#"{{"cmd":"closeEditor"}}"#).unwrap();
    }

    if id.contains("oap-test-gain") && !editor {
        // "CRASH" as base64
        writeln!(stdin, r#"{{"cmd":"setState","req":3,"data":"Q1JBU0g="}}"#).unwrap();
        println!("crash state: {}", rx.recv_timeout(Duration::from_secs(10)).unwrap_or_default());
        let r = block(0.8);
        std::thread::sleep(Duration::from_millis(300));
        let gone = child.try_wait().ok().flatten();
        println!("after crash: answer {:?}, helper exit {:?}", r, gone.map(|s| s.code()));
        assert!(r.is_none() && gone.is_some(), "crash not contained");
        println!("OK: crash contained, this process is still alive");
    } else {
        writeln!(stdin, r#"{{"cmd":"quit"}}"#).unwrap();
        let _ = child.wait();
    }
}
