//! Writing audio files.
//!
//! * lossless: WAV (16/24-bit PCM, 32-bit float), FLAC (16/24-bit),
//! * lossy: MP3 (LAME), Ogg Vorbis (aoTuV), Opus in Ogg (libopus), M4A / AAC (Windows Media
//!   Foundation, so no codec licence ships with the app).

use crate::buffer::AudioBuffer;
use crate::resample::resample;
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Wav,
    Flac,
    Mp3,
    Ogg,
    Opus,
    M4a,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
            Format::Opus => "opus",
            Format::M4a => "m4a",
        }
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "wav" | "wave" => Format::Wav,
            "flac" => Format::Flac,
            "mp3" => Format::Mp3,
            "ogg" | "oga" => Format::Ogg,
            "opus" => Format::Opus,
            "m4a" | "mp4" | "aac" => Format::M4a,
            _ => return None,
        })
    }

    pub fn lossy(self) -> bool {
        !matches!(self, Format::Wav | Format::Flac)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct EncodeOptions {
    pub format: Format,
    /// 16, 24 or 32 (32 = float, WAV only); ignored by lossy formats
    #[serde(default = "bits24")]
    pub bits: u32,
    /// TPDF dither when reducing to 16 bit
    #[serde(default = "yes")]
    pub dither: bool,
    /// lossy formats: total bitrate in kbit/s
    #[serde(default = "kbps192")]
    pub bitrate: u32,
}
fn yes() -> bool {
    true
}
fn bits24() -> u32 {
    24
}
fn kbps192() -> u32 {
    192
}

impl EncodeOptions {
    pub fn from_path(path: &Path) -> Self {
        let format = path.extension().and_then(|e| e.to_str()).and_then(Format::from_ext).unwrap_or(Format::Wav);
        Self { format, bits: 24, dither: true, bitrate: 192 }
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

/// Lossy encoders take at most two channels; anything wider is folded to stereo.
fn stereo_or_mono(buf: &AudioBuffer) -> AudioBuffer {
    if buf.channels() <= 2 { buf.clone() } else { buf.to_stereo() }
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
        Format::Mp3 => write_mp3(path, &stereo_or_mono(buf), opt),
        Format::Ogg => write_vorbis(path, &stereo_or_mono(buf), opt),
        Format::Opus => write_opus(path, &stereo_or_mono(buf), opt),
        Format::M4a => write_m4a(path, &stereo_or_mono(buf), opt),
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

// ───────────────────────────── MP3 (LAME) ─────────────────────────────

fn write_mp3(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, MonoPcm, Quality};
    // MPEG-1 layer III covers 32 / 44.1 / 48 kHz; anything else is resampled first
    let buf = match buf.rate {
        32000 | 44100 | 48000 => buf.clone(),
        r if r < 40000 => resample(buf, 44100),
        _ => resample(buf, 48000),
    };
    let rate = match opt.bitrate {
        0..=104 => Bitrate::Kbps96,
        105..=120 => Bitrate::Kbps112,
        121..=144 => Bitrate::Kbps128,
        145..=176 => Bitrate::Kbps160,
        177..=208 => Bitrate::Kbps192,
        209..=240 => Bitrate::Kbps224,
        241..=288 => Bitrate::Kbps256,
        _ => Bitrate::Kbps320,
    };
    let e = |x: &dyn std::fmt::Display| format!("MP3: {x}");
    let mut b = Builder::new().ok_or("MP3: LAME init")?;
    b.set_num_channels(buf.channels() as u8).map_err(|x| e(&x))?;
    b.set_sample_rate(buf.rate).map_err(|x| e(&x))?;
    b.set_brate(rate).map_err(|x| e(&x))?;
    b.set_quality(Quality::Best).map_err(|x| e(&x))?;
    b.set_to_write_vbr_tag(true).map_err(|x| e(&x))?;
    let mut enc = b.build().map_err(|x| e(&x))?;
    let mut out: Vec<u8> = Vec::new();
    const BLOCK: usize = 1152 * 16;
    let n = buf.frames();
    let mut i = 0;
    while i < n {
        let j = (i + BLOCK).min(n);
        out.reserve(mp3lame_encoder::max_required_buffer_size(j - i));
        if buf.channels() == 1 {
            enc.encode_to_vec(MonoPcm(&buf.ch[0][i..j]), &mut out).map_err(|x| e(&x))?;
        } else {
            enc.encode_to_vec(DualPcm { left: &buf.ch[0][i..j], right: &buf.ch[1][i..j] }, &mut out).map_err(|x| e(&x))?;
        }
        i = j;
    }
    out.reserve(7200);
    enc.flush_to_vec::<FlushNoGap>(&mut out).map_err(|x| e(&x))?;
    // LAME/Xing header at the start of the file: exact length and gapless playback
    let mut tag = Vec::with_capacity(enc.lame_tag_size().max(1));
    if enc.lame_tag_encode_to_vec(&mut tag).is_some() && tag.len() <= out.len() {
        out[..tag.len()].copy_from_slice(&tag);
    }
    std::fs::write(path, out).map_err(|x| x.to_string())
}

// ───────────────────────────── Ogg Vorbis ─────────────────────────────

fn write_vorbis(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    use std::num::{NonZeroU32, NonZeroU8};
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
    let file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let rate = NonZeroU32::new(buf.rate).ok_or("rate")?;
    let ch = NonZeroU8::new(buf.channels() as u8).ok_or("channels")?;
    let serial = (buf.frames() as i32) ^ 0x4F41_5000;
    let mut b = VorbisEncoderBuilder::new_with_serial(rate, ch, file, serial);
    b.bitrate_management_strategy(VorbisBitrateManagementStrategy::Vbr {
        target_bitrate: NonZeroU32::new(opt.bitrate.clamp(64, 500) * 1000).unwrap(),
    });
    b.comment_tag("ENCODER", "OpenAudioPower").map_err(|e| format!("Vorbis: {e}"))?;
    let mut enc = b.build().map_err(|e| format!("Vorbis: {e}"))?;
    const BLOCK: usize = 4096;
    let n = buf.frames();
    let mut i = 0;
    while i < n {
        let j = (i + BLOCK).min(n);
        let block: Vec<&[f32]> = buf.ch.iter().map(|c| &c[i..j]).collect();
        enc.encode_audio_block(&block).map_err(|e| format!("Vorbis: {e}"))?;
        i = j;
    }
    let mut w = enc.finish().map_err(|e| format!("Vorbis: {e}"))?;
    std::io::Write::flush(&mut w).map_err(|e| e.to_string())
}

// ───────────────────────────── Opus in Ogg ─────────────────────────────

fn write_opus(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    use ogg::writing::{PacketWriteEndInfo, PacketWriter};
    use unsafe_libopus as op;
    // Opus always runs at 48 kHz; the header keeps the original rate for decoders that care
    let input_rate = buf.rate;
    let buf = if buf.rate != 48000 { resample(buf, 48000) } else { buf.clone() };
    let ch = buf.channels();
    const FRAME: usize = 960; // 20 ms

    struct Enc(*mut op::OpusEncoder);
    impl Drop for Enc {
        fn drop(&mut self) {
            unsafe { op::opus_encoder_destroy(self.0) }
        }
    }
    let mut err = 0i32;
    let enc = Enc(unsafe { op::opus_encoder_create(48000, ch as i32, op::OPUS_APPLICATION_AUDIO, &mut err) });
    if err != 0 || enc.0.is_null() {
        return Err(format!("Opus: init {err}"));
    }
    let bitrate = (opt.bitrate.clamp(32, 512) * 1000) as i32;
    let mut lookahead = 0i32;
    unsafe {
        op::opus_encoder_ctl_impl(enc.0, op::OPUS_SET_BITRATE_REQUEST, op::varargs!(bitrate));
        op::opus_encoder_ctl_impl(enc.0, op::OPUS_SET_COMPLEXITY_REQUEST, op::varargs!(10i32));
        op::opus_encoder_ctl_impl(enc.0, op::OPUS_GET_LOOKAHEAD_REQUEST, op::varargs!(&mut lookahead));
    }
    let pre_skip = lookahead.max(0) as u16;

    let file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let mut w = PacketWriter::new(file);
    let serial = 0x4F41_5031u32 ^ buf.frames() as u32;
    // OpusHead (RFC 7845)
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(ch as u8);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&input_rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    w.write_packet(head, serial, PacketWriteEndInfo::EndPage, 0).map_err(|e| e.to_string())?;
    // OpusTags
    let vendor = b"OpenAudioPower";
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    w.write_packet(tags, serial, PacketWriteEndInfo::EndPage, 0).map_err(|e| e.to_string())?;

    let n = buf.frames();
    // pad with the encoder delay so the last real samples leave the encoder
    let total = n + pre_skip as usize;
    let frames = total.div_ceil(FRAME);
    let mut pcm = vec![0.0f32; FRAME * ch];
    let mut packet = vec![0u8; 4000];
    for f in 0..frames {
        for i in 0..FRAME {
            let s = f * FRAME + i;
            for c in 0..ch {
                pcm[i * ch + c] = if s < n { buf.ch[c][s] } else { 0.0 };
            }
        }
        let len = unsafe { op::opus_encode_float(enc.0, pcm.as_ptr(), FRAME as i32, packet.as_mut_ptr(), packet.len() as i32) };
        if len < 0 {
            return Err(format!("Opus: encode {len}"));
        }
        let last = f + 1 == frames;
        // granule position counts 48 kHz samples including pre-skip; the last page trims padding
        let granule = if last { total as u64 } else { ((f + 1) * FRAME) as u64 };
        let info = if last { PacketWriteEndInfo::EndStream } else if (f + 1) % 50 == 0 { PacketWriteEndInfo::EndPage } else { PacketWriteEndInfo::NormalPacket };
        w.write_packet(packet[..len as usize].to_vec(), serial, info, granule).map_err(|e| e.to_string())?;
    }
    let mut file = w.into_inner();
    std::io::Write::flush(&mut file).map_err(|e| e.to_string())
}

// ───────────────────────────── M4A / AAC ─────────────────────────────

#[cfg(windows)]
fn write_m4a(path: &Path, buf: &AudioBuffer, opt: &EncodeOptions) -> Result<(), String> {
    mf::write_aac(path, buf, opt.bitrate)
}

#[cfg(not(windows))]
fn write_m4a(_path: &Path, _buf: &AudioBuffer, _opt: &EncodeOptions) -> Result<(), String> {
    Err("M4A: available on Windows only".into())
}

#[cfg(windows)]
mod mf {
    use super::{resample, AudioBuffer};
    use std::path::Path;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    struct Session {
        com: bool,
    }
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
                if self.com {
                    CoUninitialize();
                }
            }
        }
    }

    fn e(what: &str, err: windows::core::Error) -> String {
        format!("M4A ({what}): {}", err.message())
    }

    pub fn write_aac(path: &Path, buf: &AudioBuffer, kbps: u32) -> Result<(), String> {
        // the Windows AAC encoder accepts 44.1 and 48 kHz only
        let buf = match buf.rate {
            44100 | 48000 => buf.clone(),
            r if r < 46000 => resample(buf, 44100),
            _ => resample(buf, 48000),
        };
        let ch = buf.channels() as u32;
        let rate = buf.rate;
        // the encoder supports 96, 128, 160 and 192 kbit/s (bytes per second below)
        let bytes_per_sec: u32 = match kbps {
            0..=111 => 12000,
            112..=143 => 16000,
            144..=175 => 20000,
            _ => 24000,
        };
        unsafe {
            let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|x| e("startup", x))?;
            let _session = Session { com };

            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 2).map_err(|x| e("attributes", x))?;
            let attrs = attrs.ok_or("M4A: attributes")?;
            attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4).map_err(|x| e("container", x))?;
            let url = HSTRING::from(path.as_os_str());
            let _ = std::fs::remove_file(path);
            let writer = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None, &attrs).map_err(|x| e("writer", x))?;

            let out = MFCreateMediaType().map_err(|x| e("type", x))?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|x| e("type", x))?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(|x| e("type", x))?;
            out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(|x| e("type", x))?;
            out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate).map_err(|x| e("type", x))?;
            out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch).map_err(|x| e("type", x))?;
            out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_sec).map_err(|x| e("type", x))?;
            let stream = writer.AddStream(&out).map_err(|x| e("stream", x))?;

            let inp = MFCreateMediaType().map_err(|x| e("type", x))?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|x| e("type", x))?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(|x| e("type", x))?;
            inp.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(|x| e("type", x))?;
            inp.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate).map_err(|x| e("type", x))?;
            inp.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch).map_err(|x| e("type", x))?;
            inp.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, ch * 2).map_err(|x| e("type", x))?;
            inp.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * ch * 2).map_err(|x| e("type", x))?;
            writer.SetInputMediaType(stream, &inp, None).map_err(|x| e("input", x))?;
            writer.BeginWriting().map_err(|x| e("begin", x))?;

            let pcm = super::to_int(&buf, 16, true);
            let n = buf.frames();
            const BLOCK: usize = 8192;
            let mut i = 0;
            while i < n {
                let j = (i + BLOCK).min(n);
                let bytes = (j - i) * ch as usize * 2;
                let mb = MFCreateMemoryBuffer(bytes as u32).map_err(|x| e("buffer", x))?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                mb.Lock(&mut ptr, None, None).map_err(|x| e("lock", x))?;
                let dst = std::slice::from_raw_parts_mut(ptr as *mut i16, (j - i) * ch as usize);
                for (d, s) in dst.iter_mut().zip(&pcm[i * ch as usize..j * ch as usize]) {
                    *d = *s as i16;
                }
                mb.Unlock().map_err(|x| e("unlock", x))?;
                mb.SetCurrentLength(bytes as u32).map_err(|x| e("length", x))?;
                let sample = MFCreateSample().map_err(|x| e("sample", x))?;
                sample.AddBuffer(&mb).map_err(|x| e("sample", x))?;
                // 100 ns units
                sample.SetSampleTime((i as i64 * 10_000_000) / rate as i64).map_err(|x| e("time", x))?;
                sample.SetSampleDuration(((j - i) as i64 * 10_000_000) / rate as i64).map_err(|x| e("time", x))?;
                writer.WriteSample(stream, &sample).map_err(|x| e("write", x))?;
                i = j;
            }
            writer.Finalize().map_err(|x| e("finalize", x))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f32) -> AudioBuffer {
        let n = (rate as f32 * secs) as usize;
        let l: Vec<f32> = (0..n).map(|i| 0.4 * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin()).collect();
        let r: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 660.0 * std::f32::consts::TAU / rate as f32).sin()).collect();
        AudioBuffer::from_channels(rate, vec![l, r])
    }

    /// Every lossy format writes a file that the decoder reads back with roughly the same length.
    #[test]
    fn lossy_round_trip() {
        let dir = std::env::temp_dir().join("oap-encode-test");
        let _ = std::fs::create_dir_all(&dir);
        let src = tone(44100, 3.0);
        let mut formats = vec![Format::Mp3, Format::Ogg, Format::Opus];
        if cfg!(windows) {
            formats.push(Format::M4a);
        }
        for f in formats {
            let p = dir.join(format!("tone.{}", f.ext()));
            write(&p, &src, &EncodeOptions { format: f, bits: 16, dither: true, bitrate: 160 }).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            if f == Format::Opus {
                // symphonia has no Opus decoder: check the container and the size (~160 kbit/s)
                let data = std::fs::read(&p).unwrap();
                assert_eq!(&data[..4], b"OggS");
                assert!(data.windows(8).any(|w| w == b"OpusHead"));
                let kbps = data.len() as f32 * 8.0 / 3.0 / 1000.0;
                assert!((100.0..220.0).contains(&kbps), "Opus: {kbps:.0} kbit/s");
                continue;
            }
            let (back, _) = crate::decode::decode_file(&p, &crate::Cancel::default(), &|_| {}).unwrap_or_else(|e| panic!("{f:?} decode: {e}"));
            let secs = back.duration();
            assert!((secs - 3.0).abs() < 0.15, "{f:?}: {secs:.3} s");
            assert_eq!(back.channels(), 2, "{f:?}");
            assert!(back.peak() > 0.25 && back.peak() < 0.6, "{f:?}: peak {}", back.peak());
        }
    }
}
