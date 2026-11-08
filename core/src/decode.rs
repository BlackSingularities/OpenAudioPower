//! Decoding of every common format (MP3, FLAC, WAV, AIFF, OGG/Vorbis, M4A/AAC, ALAC, CAF, MKV/WebM audio)
//! through symphonia.

use crate::buffer::{AudioBuffer, Meta};
use crate::Cancel;
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

pub const EXTENSIONS: &[&str] = &[
    "wav", "wave", "flac", "mp3", "mp2", "ogg", "oga", "m4a", "mp4", "aac", "alac", "aif", "aiff", "aifc", "caf", "mkv", "webm", "mka",
];

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn read_tags(meta: &mut Meta, rev: &MetadataRevision) {
    for tag in rev.tags() {
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => meta.title = tag.value.to_string(),
            Some(StandardTagKey::Artist) => meta.artist = tag.value.to_string(),
            Some(StandardTagKey::Album) => meta.album = tag.value.to_string(),
            _ => {}
        }
    }
}

pub fn decode_file(path: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<(AudioBuffer, Meta), String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .map_err(|e| format!("Nieobsługiwany format / unsupported format: {e}"))?;

    let mut meta = Meta::default();
    if let Some(m) = probed.metadata.get() {
        if let Some(rev) = m.current() {
            read_tags(&mut meta, rev);
        }
    }
    let format = &mut probed.format;
    if let Some(rev) = format.metadata().current() {
        read_tags(&mut meta, rev);
    }
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some())
        .ok_or("Brak ścieżki audio / no audio track")?
        .clone();
    let params = &track.codec_params;
    let rate = params.sample_rate.unwrap_or(44100);
    meta.source_rate = rate;
    meta.bits = params.bits_per_sample.unwrap_or(0);
    meta.codec = symphonia::default::get_codecs()
        .get_codec(params.codec)
        .map(|d| d.short_name.to_string())
        .unwrap_or_default();
    let total_frames = params.n_frames.unwrap_or(0);
    let mut decoder = symphonia::default::get_codecs()
        .make(params, &DecoderOptions::default())
        .map_err(|e| format!("Kodek / codec: {e}"))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    let mut decoded: u64 = 0;
    let mut last_report = 0u64;
    loop {
        if cancel.is_set() {
            return Err("cancelled".into());
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymErr::ResetRequired) => break,
            Err(e) => {
                if channels.first().map(|c| c.is_empty()).unwrap_or(true) {
                    return Err(format!("{e}"));
                }
                break;
            }
        };
        if packet.track_id() != track.id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                let spec = *audio.spec();
                let k = spec.channels.count();
                if k == 0 {
                    continue;
                }
                let needed = audio.capacity() * k;
                if sbuf.as_ref().map(|b| b.capacity() < needed).unwrap_or(true) {
                    sbuf = Some(SampleBuffer::<f32>::new(audio.capacity() as u64, spec));
                }
                let b = sbuf.as_mut().unwrap();
                b.copy_interleaved_ref(audio);
                if channels.len() != k {
                    if channels.iter().all(|c| c.is_empty()) {
                        let cap = if total_frames > 0 { total_frames as usize } else { 0 };
                        channels = (0..k).map(|_| Vec::with_capacity(cap)).collect();
                    } else {
                        continue; // channel layout changed mid-stream – skip
                    }
                }
                let s = b.samples();
                let n = s.len() / k;
                for i in 0..n {
                    for c in 0..k {
                        channels[c].push(s[i * k + c]);
                    }
                }
                decoded += n as u64;
                if decoded - last_report > 300_000 {
                    last_report = decoded;
                    let f = if total_frames > 0 { decoded as f32 / total_frames as f32 } else { 0.5 };
                    progress(f.min(1.0));
                }
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(SymErr::IoError(_)) => continue,
            Err(e) => return Err(format!("{e}")),
        }
    }
    if channels.is_empty() || channels[0].is_empty() {
        return Err("Plik nie zawiera dźwięku / file contains no audio".into());
    }
    if meta.title.is_empty() {
        meta.title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    }
    progress(1.0);
    Ok((AudioBuffer::from_channels(rate, channels), meta))
}
