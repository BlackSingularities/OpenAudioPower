//! Writing audio files: WAV (16/24-bit PCM, 32-bit float) and FLAC (16/24-bit).

use crate::buffer::AudioBuffer;
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Wav,
    Flac,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct EncodeOptions {
    pub format: Format,
    /// 16, 24 or 32 (32 = float, WAV only)
    pub bits: u32,
    /// TPDF dither when reducing to 16 bit
    #[serde(default = "yes")]
    pub dither: bool,
}
fn yes() -> bool {
    true
}

impl EncodeOptions {
    pub fn from_path(path: &Path) -> Self {
        let flac = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("flac")).unwrap_or(false);
        Self { format: if flac { Format::Flac } else { Format::Wav }, bits: 24, dither: true }
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn to_int(buf: &AudioBuffer, bits: u32, dither: bool) -> Vec<i32> {
    let max = ((1i64 << (bits - 1)) - 1) as f32;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let n = buf.frames();
    let k = buf.channels();
    let mut out = Vec::with_capacity(n * k);
    for i in 0..n {
        for c in 0..k {
            let mut v = buf.ch[c][i] * max;
            if dither && bits <= 16 {
                v += rng.next() - rng.next();
            }
            out.push(v.round().clamp(-max - 1.0, max) as i32);
        }
    }
    out
}

pub fn write(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if buf.frames() == 0 {
        return Err("Pusty dźwięk / empty audio".into());
    }
    match opt.format {
        Format::Wav => write_wav(path, buf, opt),
        Format::Flac => write_flac(path, buf, opt),
    }
}

fn write_wav(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    let float = opt.bits >= 32;
    let spec = hound::WavSpec {
        channels: buf.channels() as u16,
        sample_rate: buf.rate,
        bits_per_sample: if float { 32 } else { opt.bits.clamp(16, 24) as u16 },
        sample_format: if float { hound::SampleFormat::Float } else { hound::SampleFormat::Int },
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    if float {
        for i in 0..buf.frames() {
            for c in &buf.ch {
                w.write_sample(c[i]).map_err(|e| e.to_string())?;
            }
        }
    } else {
        for s in to_int(buf, spec.bits_per_sample as u32, opt.dither) {
            w.write_sample(s).map_err(|e| e.to_string())?;
        }
    }
    w.finalize().map_err(|e| e.to_string())
}

fn write_flac(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let bits = if opt.bits <= 16 { 16 } else { 24 };
    let samples = to_int(buf, bits, opt.dither);
    let config = flacenc::config::Encoder::default().into_verified().map_err(|e| format!("{e:?}"))?;
    let source = flacenc::source::MemSource::from_samples(&samples, buf.channels(), bits as usize, buf.rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).map_err(|e| format!("{e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| format!("{e:?}"))?;
    std::fs::write(path, sink.as_slice()).map_err(|e| e.to_string())
}
