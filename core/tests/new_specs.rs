//! The UI sends effects and operations as JSON with its own default values; make sure
//! every new kind deserialises and runs offline without blowing up.

use openaudiopower_core::buffer::AudioBuffer;
use openaudiopower_core::effects::EffectSpec;
use openaudiopower_core::ops::{apply, Op};
use openaudiopower_core::Cancel;

const EFFECTS: &[&str] = &[
    r#"{"type":"ladder","mode":"lowPass","cutoff":1200,"resonance":0.4,"drive":3}"#,
    r#"{"type":"autoFilter","mode":"bandPass","base":400,"depth":3,"rate":0.5,"envelope":true,"sensitivity":1,"q":3,"mix":1}"#,
    r#"{"type":"vowel","vowel":1.5,"resonance":0.4,"rate":0.5,"depth":1,"mix":1}"#,
    r#"{"type":"tilt","tilt":3,"pivot":1000}"#,
    r#"{"type":"comb","freq":220,"feedback":0.6,"mix":0.5}"#,
    r#"{"type":"multiband","lowX":200,"highX":3000,"thrLow":-20,"thrMid":-18,"thrHigh":-16,"gainLow":0,"gainMid":0,"gainHigh":0,"ratio":3,"attack":10,"release":150}"#,
    r#"{"type":"transient","attack":0.3,"sustain":0,"output":0}"#,
    r#"{"type":"clipper","input":3,"ceiling":-0.3,"softness":0.3,"output":0}"#,
    r#"{"type":"exciter","freq":3500,"drive":12,"mix":0.2}"#,
    r#"{"type":"tape","drive":6,"wow":0.2,"flutter":0.2,"hiss":-70,"tone":12000,"mix":1}"#,
    r#"{"type":"ringMod","freq":30,"mix":0.5}"#,
    r#"{"type":"vibrato","rate":5,"depth":2}"#,
    r#"{"type":"haas","delay":12,"right":true,"mix":1}"#,
];

const OPS: &[&str] = &[
    r#"{"op":"declip","tolerance":0.01,"gain":-3}"#,
    r#"{"op":"dehum","base":50,"harmonics":8,"q":30}"#,
    r#"{"op":"deplosive","freq":150,"sensitivity":0.6,"reduction":18}"#,
    r#"{"op":"level","target":-20,"maxGain":12,"gate":-50,"speed":800}"#,
];

fn noise(rate: u32, secs: f32) -> AudioBuffer {
    let n = (rate as f32 * secs) as usize;
    let mut s = 12345u32;
    let mut ch = || {
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.5
            })
            .collect::<Vec<f32>>()
    };
    let l = ch();
    let r = ch();
    AudioBuffer::from_channels(rate, vec![l, r])
}

#[test]
fn new_effects_run_offline() {
    let src = noise(44100, 0.5);
    for js in EFFECTS {
        let spec: EffectSpec = serde_json::from_str(js).unwrap_or_else(|e| panic!("{js}: {e}"));
        let op = Op::Effect { spec, tail: 0.0 };
        let out = apply(&src, 0, 0, &op, &Cancel::default()).unwrap();
        assert_eq!(out.frames(), src.frames(), "{js}");
        assert!(out.ch.iter().flatten().all(|x| x.is_finite() && x.abs() < 8.0), "{js}");
    }
}

#[test]
fn new_ops_run() {
    let src = noise(44100, 0.5);
    for js in OPS {
        let op: Op = serde_json::from_str(js).unwrap_or_else(|e| panic!("{js}: {e}"));
        // whole buffer and a range in the middle
        for (a, b) in [(0, 0), (5000, 15000)] {
            let out = apply(&src, a, b, &op, &Cancel::default()).unwrap();
            assert_eq!(out.frames(), src.frames(), "{js}");
            assert!(out.ch.iter().flatten().all(|x| x.is_finite()), "{js}");
        }
    }
}

/// Operations that change the length: they must deserialize and give finite audio.
#[test]
fn length_changing_ops_run() {
    let src = noise(44100, 0.5);
    for js in [
        r#"{"op":"truncateSilence","threshold":-45,"minLength":600,"keep":350}"#,
        r#"{"op":"paulstretch","factor":4,"window":0.1}"#,
        r#"{"op":"slidingStretch","tempoStart":0,"tempoEnd":-30,"pitchStart":0,"pitchEnd":3}"#,
        r#"{"op":"repeat","count":2}"#,
        r#"{"op":"resample","rate":8000,"reinterpret":false}"#,
        r#"{"op":"resample","rate":22050,"reinterpret":true}"#,
    ] {
        let op: Op = serde_json::from_str(js).unwrap_or_else(|e| panic!("{js}: {e}"));
        let out = apply(&src, 0, 0, &op, &Cancel::default()).unwrap();
        assert!(out.frames() > 0, "{js}");
        assert!(out.ch.iter().flatten().all(|x| x.is_finite()), "{js}");
    }
}

/// Linear-phase EQ reports its latency, so offline rendering lines the output up with the input;
/// a mid-only cut leaves the side signal alone.
#[test]
fn linear_and_mid_side_eq() {
    use openaudiopower_core::effects::render_offline;
    let rate = 48000.0;
    let n = 24000;
    let l: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * 440.0 * i as f32 / rate).sin()).collect();
    let r: Vec<f32> = l.iter().map(|v| -v).collect(); // pure side signal
    let flat: EffectSpec = serde_json::from_str(r#"{"type":"eq","linear":true,"bands":[{"kind":"peak","freq":1000,"q":1,"gain":0,"on":true}]}"#).unwrap();
    let mut ch = vec![l.clone(), r.clone()];
    render_offline(&mut ch, rate, &flat, 0);
    let err = ch[0][6000..18000].iter().zip(&l[6000..18000]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(err < 0.02, "linear flat EQ misaligned: {err}");
    // −24 dB on mid at 440 Hz must not touch a side-only signal
    let mid: EffectSpec = serde_json::from_str(r#"{"type":"eq","bands":[{"kind":"peak","freq":440,"q":1,"gain":-24,"on":true,"target":"mid"}]}"#).unwrap();
    let mut ch = vec![l.clone(), r.clone()];
    render_offline(&mut ch, rate, &mid, 0);
    let err = ch[0][6000..].iter().zip(&l[6000..]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-3, "mid band changed the side: {err}");
    // the same band on the side cuts it
    let side: EffectSpec = serde_json::from_str(r#"{"type":"eq","bands":[{"kind":"peak","freq":440,"q":1,"gain":-24,"on":true,"target":"side"}]}"#).unwrap();
    let mut ch = vec![l.clone(), r.clone()];
    render_offline(&mut ch, rate, &side, 0);
    let pk = ch[0][12000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(pk < 0.1, "side band did not cut: {pk}");
    for js in [
        r#"{"type":"dynEq","shape":"bell","freq":6000,"q":2,"threshold":-30,"ratio":3,"range":-9,"attack":2,"release":120}"#,
        r#"{"type":"imager","lowX":150,"highX":4000,"widthLow":0,"widthMid":1,"widthHigh":1.4}"#,
        r#"{"type":"limiter","ceiling":-1,"release":80,"input":3,"lookahead":3,"truePeak":true}"#,
    ] {
        let spec: EffectSpec = serde_json::from_str(js).unwrap_or_else(|e| panic!("{js}: {e}"));
        let mut ch = vec![l.clone(), r.clone()];
        render_offline(&mut ch, rate, &spec, 0);
        assert!(ch.iter().flatten().all(|v| v.is_finite()), "{js}");
    }
}
