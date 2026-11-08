//! Audio waveforms: per-column peaks of the opening seconds of a file, decoded in-tree.
//!
//! Reads WAV, AIFF/AIFC, MP4/MOV/M4A, MKV/WebM and Ogg Opus. Decodes PCM directly, AAC through
//! `craft-aac` and Opus through `craft-opus`. FLAC, MP3 and Vorbis have no decoder in this
//! workspace yet and return [`Error::UnsupportedCodec`].

use crate::{Error, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Waveform request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveformOptions {
    /// Number of peak columns.
    pub columns: usize,
    /// Seconds of audio read from the start.
    pub max_seconds: f64,
}

impl Default for WaveformOptions {
    fn default() -> Self {
        Self {
            columns: 640,
            max_seconds: 12.0,
        }
    }
}

/// Peaks of the opening of a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Waveform {
    /// Absolute peak per column across all channels, 0.0 (silence) to 1.0 (full scale).
    pub peaks: Vec<f32>,
    /// Seconds of audio the peaks cover.
    pub seconds: f64,
    /// Sample rate of the decoded audio.
    pub sample_rate: u32,
    /// Codec decoded (`pcm`, `aac`, `opus`).
    pub codec: &'static str,
}

/// Collects the per-sample absolute peak across channels, up to a sample budget.
struct Mono {
    samples: Vec<f32>,
    limit: usize,
    /// Leading samples to drop (encoder priming an edit list hides).
    skip: usize,
}

impl Mono {
    fn new(rate: u32, max_seconds: f64) -> Self {
        let limit = (f64::from(rate) * max_seconds.clamp(0.1, 600.0)) as usize;
        Self {
            samples: Vec::with_capacity(limit.min(1 << 22)),
            limit: limit.max(1),
            skip: 0,
        }
    }
    fn full(&self) -> bool {
        self.samples.len() >= self.limit
    }
    fn take(&mut self, peak: f32) {
        if self.skip > 0 {
            self.skip -= 1;
        } else {
            self.samples.push(peak);
        }
    }
    /// Planar channels from a decoder.
    fn push_planar(&mut self, planar: &[Vec<f32>]) {
        let n = planar.iter().map(Vec::len).min().unwrap_or(0);
        for i in 0..n {
            if self.full() {
                return;
            }
            let peak = planar
                .iter()
                .filter_map(|c| c.get(i))
                .fold(0f32, |a, s| a.max(s.abs()));
            self.take(peak);
        }
    }
    /// Interleaved integer or float PCM bytes.
    fn push_pcm(&mut self, bytes: &[u8], f: PcmFormat) {
        let width = usize::from(f.bits.div_ceil(8));
        let frame = width * usize::from(f.channels.max(1));
        if width == 0 || frame == 0 {
            return;
        }
        for chunk in bytes.chunks_exact(frame) {
            if self.full() {
                return;
            }
            let peak = chunk
                .chunks_exact(width)
                .map(|s| f.sample(s).abs())
                .fold(0f32, f32::max);
            self.take(peak);
        }
    }
    fn into_waveform(self, columns: usize, rate: u32, codec: &'static str) -> Result<Waveform> {
        if self.samples.is_empty() {
            return Err(Error::NoFrame);
        }
        let columns = columns.max(1);
        let n = self.samples.len();
        let peaks = (0..columns)
            .map(|c| {
                let (a, b) = (
                    c * n / columns,
                    ((c + 1) * n / columns).max(c * n / columns + 1).min(n),
                );
                self.samples
                    .get(a..b)
                    .map_or(0.0, |s| s.iter().fold(0f32, |m, v| m.max(*v)).min(1.0))
            })
            .collect();
        Ok(Waveform {
            peaks,
            seconds: n as f64 / f64::from(rate.max(1)),
            sample_rate: rate,
            codec,
        })
    }
}

#[derive(Clone, Copy)]
struct PcmFormat {
    bits: u16,
    channels: u16,
    float: bool,
    big_endian: bool,
}

impl PcmFormat {
    fn sample(&self, s: &[u8]) -> f32 {
        let mut b = [0u8; 8];
        let w = s.len().min(8);
        if self.big_endian {
            for (i, v) in s.iter().take(w).rev().enumerate() {
                b[i] = *v;
            }
        } else {
            b[..w].copy_from_slice(&s[..w]);
        }
        match (self.float, w) {
            (true, 4) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            (true, 8) => f64::from_le_bytes(b) as f32,
            // Only 32- and 64-bit float PCM exists; anything else is treated as silence.
            (true, _) => 0.0,
            // 8-bit PCM is unsigned in WAV.
            (false, 1) => (f32::from(b[0]) - 128.0) / 128.0,
            (false, w) => {
                let bits = (w * 8) as u32;
                let raw = u64::from_le_bytes(b) << (64 - bits);
                (raw as i64 >> (64 - bits)) as f32 / (1u64 << (bits - 1)) as f32
            }
        }
    }
}

/// Read peaks from the opening of an audio (or video) file.
pub fn waveform(path: &Path, opts: &WaveformOptions) -> Result<Waveform> {
    let mut head = [0u8; 12];
    let n = File::open(path)?.read(&mut head)?;
    let head = head.get(..n).unwrap_or(&[]);
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        return riff_like(path, opts, false);
    }
    if head.starts_with(b"FORM") && matches!(head.get(8..12), Some(b"AIFF" | b"AIFC")) {
        return riff_like(path, opts, true);
    }
    if head.starts_with(b"OggS") {
        return ogg_opus(path, opts);
    }
    if matches!(
        head.get(4..8),
        Some(b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip")
    ) {
        return mp4(path, opts);
    }
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return mkv(path, opts);
    }
    Err(Error::UnsupportedContainer)
}

/// WAV (`fmt ` + `data`) and AIFF (`COMM` + `SSND`), streamed from the file.
fn riff_like(path: &Path, opts: &WaveformOptions, aiff: bool) -> Result<Waveform> {
    let bad = |why: &str| Error::Demux(why.into());
    let mut f = File::open(path)?;
    let mut hdr = vec![0u8; 12];
    f.read_exact(&mut hdr)?;
    let (mut fmt, mut rate) = (None, 0u32);
    loop {
        let mut ch = [0u8; 8];
        if f.read_exact(&mut ch).is_err() {
            return Err(bad("no audio data chunk"));
        }
        let id = [ch[0], ch[1], ch[2], ch[3]];
        let len = if aiff {
            u32::from_be_bytes([ch[4], ch[5], ch[6], ch[7]])
        } else {
            u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]])
        };
        let padded = u64::from(len) + u64::from(len & 1);
        match (&id, aiff) {
            (b"fmt ", false) | (b"COMM", true) => {
                let mut body = vec![0u8; (len as usize).min(64)];
                f.read_exact(&mut body)?;
                std::io::copy(
                    &mut (&mut f).take(padded - body.len() as u64),
                    &mut std::io::sink(),
                )?;
                let parsed = if aiff {
                    comm(&body, hdr.get(8..12) == Some(b"AIFC"))
                } else {
                    wav_fmt(&body)
                };
                let (p, r) = parsed.ok_or_else(|| bad("unsupported sample format"))?;
                fmt = Some(p);
                rate = r;
            }
            (b"data", false) | (b"SSND", true) => {
                let p = fmt.ok_or_else(|| bad("audio data before its format"))?;
                if aiff {
                    // SSND: offset and block size, then samples.
                    let mut ob = [0u8; 8];
                    f.read_exact(&mut ob)?;
                    let skip = u32::from_be_bytes([ob[0], ob[1], ob[2], ob[3]]);
                    std::io::copy(&mut (&mut f).take(u64::from(skip)), &mut std::io::sink())?;
                }
                let mut mono = Mono::new(rate, opts.max_seconds);
                let frame = usize::from(p.bits.div_ceil(8)) * usize::from(p.channels.max(1));
                let want = (mono.limit as u64)
                    .saturating_mul(frame as u64)
                    .min(u64::from(len));
                let mut buf = Vec::new();
                (&mut f).take(want).read_to_end(&mut buf)?;
                mono.push_pcm(&buf, p);
                return mono.into_waveform(opts.columns, rate, "pcm");
            }
            _ => {
                std::io::copy(&mut (&mut f).take(padded), &mut std::io::sink())?;
            }
        }
    }
}

fn wav_fmt(b: &[u8]) -> Option<(PcmFormat, u32)> {
    let u16le = |at: usize| b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let mut tag = u16le(0)?;
    if tag == 0xFFFE {
        tag = u16le(24)?;
    }
    let channels = u16le(2)?;
    let rate = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
    let bits = u16le(14)?;
    match tag {
        1 if (8..=32).contains(&bits) => Some((
            PcmFormat {
                bits,
                channels,
                float: false,
                big_endian: false,
            },
            rate,
        )),
        3 if matches!(bits, 32 | 64) => Some((
            PcmFormat {
                bits,
                channels,
                float: true,
                big_endian: false,
            },
            rate,
        )),
        _ => None,
    }
}

fn comm(b: &[u8], aifc: bool) -> Option<(PcmFormat, u32)> {
    let channels = u16::from_be_bytes(b.get(0..2)?.try_into().ok()?);
    let bits = u16::from_be_bytes(b.get(6..8)?.try_into().ok()?);
    let e = b.get(8..18)?;
    let exp = i32::from(u16::from_be_bytes([e[0], e[1]]) & 0x7FFF);
    let mant = u64::from_be_bytes(e.get(2..10)?.try_into().ok()?);
    let rate = (mant as f64 * 2f64.powi(exp - 16383 - 63)).round();
    if !(1.0..1e7).contains(&rate) {
        return None;
    }
    let (float, big_endian) = match if aifc { b.get(18..22) } else { None } {
        None | Some(b"NONE") | Some(b"twos") => (false, true),
        Some(b"sowt") => (false, false),
        Some(b"fl32" | b"FL32") => (true, true),
        _ => return None,
    };
    Some((
        PcmFormat {
            bits,
            channels,
            float,
            big_endian,
        },
        rate as u32,
    ))
}

fn mp4(path: &Path, opts: &WaveformOptions) -> Result<Waveform> {
    use craft_isobmff::{CodecConfig, TrackKind};
    let file = File::open(path)?;
    let m = craft_isobmff::open(&file).map_err(|e| Error::Demux(e.to_string()))?;
    let ti = m.track_of_kind(TrackKind::Audio).ok_or(Error::NoAudio)?;
    let t = m.tracks.get(ti).ok_or(Error::NoAudio)?;
    let read = |i| {
        m.read_sample(&file, ti, i)
            .map_err(|e| Error::Demux(e.to_string()))
    };
    match t.codec() {
        Some(CodecConfig::Aac(c)) => {
            let mut dec =
                craft_aac::Decoder::new(&c.asc).map_err(|e| Error::Decode(e.to_string()))?;
            let rate = dec.sample_rate();
            let mut mono = Mono::new(rate, opts.max_seconds);
            mono.skip = priming(t, rate);
            for i in 0..t.samples.len() {
                if mono.full() {
                    break;
                }
                // A damaged access unit is skipped, as a player would.
                if let Ok(planar) = dec.decode(&read(i)?) {
                    mono.push_planar(&planar);
                }
            }
            mono.into_waveform(opts.columns, rate, "aac")
        }
        Some(CodecConfig::Pcm(p)) => {
            let rate = p.sample_rate.round().clamp(1.0, 1e7) as u32;
            let f = PcmFormat {
                bits: p.bits,
                channels: u16::try_from(p.channels).unwrap_or(1),
                float: p.float,
                big_endian: p.big_endian,
            };
            let mut mono = Mono::new(rate, opts.max_seconds);
            for i in 0..t.samples.len() {
                if mono.full() {
                    break;
                }
                mono.push_pcm(&read(i)?, f);
            }
            mono.into_waveform(opts.columns, rate, "pcm")
        }
        Some(other) => Err(Error::UnsupportedCodec(other.name().into())),
        None => Err(Error::UnsupportedCodec("unknown".into())),
    }
}

/// Samples (at `rate`) an MP4 edit list hides at the start: AAC encoder priming, typically
/// 1024 or 2112. Players and ffmpeg drop them.
fn priming(t: &craft_isobmff::Track, rate: u32) -> usize {
    if t.timescale == 0 {
        return 0;
    }
    let hidden = (-t.edit_offset).max(0) as u64;
    (hidden * u64::from(rate) / u64::from(t.timescale)).min(1 << 20) as usize
}

fn mkv(path: &Path, opts: &WaveformOptions) -> Result<Waveform> {
    use craft_matroska::{Codec, TrackKind};
    let file = File::open(path)?;
    let m = craft_matroska::open(&file).map_err(|e| Error::Demux(e.to_string()))?;
    let ti = m
        .tracks
        .iter()
        .position(|t| t.kind == TrackKind::Audio)
        .ok_or(Error::NoAudio)?;
    let t = m.tracks.get(ti).ok_or(Error::NoAudio)?;
    let audio = t.audio.as_ref().ok_or(Error::NoAudio)?;
    let read = |i| {
        m.read_sample(&file, ti, i)
            .map_err(|e| Error::Demux(e.to_string()))
    };
    match &t.codec {
        Codec::Aac { asc } => {
            let mut dec = craft_aac::Decoder::new(asc).map_err(|e| Error::Decode(e.to_string()))?;
            let rate = dec.sample_rate();
            let mut mono = Mono::new(rate, opts.max_seconds);
            for i in 0..t.samples.len() {
                if mono.full() {
                    break;
                }
                if let Ok(planar) = dec.decode(&read(i)?) {
                    mono.push_planar(&planar);
                }
            }
            mono.into_waveform(opts.columns, rate, "aac")
        }
        Codec::Opus { head } => {
            let mut dec =
                craft_opus::Decoder::new(head).map_err(|e| Error::Decode(e.to_string()))?;
            let mut mono = Mono::new(48_000, opts.max_seconds);
            for i in 0..t.samples.len() {
                if mono.full() {
                    break;
                }
                if let Ok(planar) = dec.decode(Some(&read(i)?)) {
                    mono.push_planar(&planar);
                }
            }
            mono.into_waveform(opts.columns, 48_000, "opus")
        }
        Codec::Pcm {
            float,
            big_endian,
            bits,
        } => {
            let rate = audio.sampling_frequency.round().clamp(1.0, 1e7) as u32;
            let f = PcmFormat {
                bits: *bits,
                channels: u16::try_from(audio.channels).unwrap_or(1),
                float: *float,
                big_endian: *big_endian,
            };
            let mut mono = Mono::new(rate, opts.max_seconds);
            for i in 0..t.samples.len() {
                if mono.full() {
                    break;
                }
                mono.push_pcm(&read(i)?, f);
            }
            mono.into_waveform(opts.columns, rate, "pcm")
        }
        other => Err(Error::UnsupportedCodec(other.name())),
    }
}

/// Ogg Opus (RFC 7845): reassemble packets from pages; the first is `OpusHead`, the second
/// `OpusTags`, the rest audio.
fn ogg_opus(path: &Path, opts: &WaveformOptions) -> Result<Waveform> {
    let mut data = Vec::new();
    // Twelve seconds of Opus is well under this; the cap bounds hostile files.
    File::open(path)?.take(16 << 20).read_to_end(&mut data)?;
    let packets = ogg_packets(&data, 4096);
    let head = packets
        .first()
        .filter(|p| p.starts_with(b"OpusHead"))
        .ok_or(Error::UnsupportedCodec("Ogg without Opus".into()))?;
    let mut dec = craft_opus::Decoder::new(head).map_err(|e| Error::Decode(e.to_string()))?;
    let mut mono = Mono::new(48_000, opts.max_seconds);
    for p in packets.iter().skip(2) {
        if mono.full() {
            break;
        }
        if let Ok(planar) = dec.decode(Some(p)) {
            mono.push_planar(&planar);
        }
    }
    mono.into_waveform(opts.columns, 48_000, "opus")
}

/// Packets of the first logical stream, at most `max`.
fn ogg_packets(d: &[u8], max: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut p = 0usize;
    let mut serial = None;
    while out.len() < max {
        let Some(h) = d.get(p..p + 27) else { break };
        if &h[0..4] != b"OggS" {
            break;
        }
        let s = u32::from_le_bytes([h[14], h[15], h[16], h[17]]);
        let nseg = usize::from(h[26]);
        let Some(table) = d.get(p + 27..p + 27 + nseg) else {
            break;
        };
        let mut body = p + 27 + nseg;
        let ours = *serial.get_or_insert(s) == s;
        for &lace in table {
            let Some(seg) = d.get(body..body + usize::from(lace)) else {
                return out;
            };
            if ours {
                cur.extend_from_slice(seg);
                if lace < 255 {
                    out.push(std::mem::take(&mut cur));
                }
            }
            body += usize::from(lace);
        }
        p = body;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mc_preview_wave_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join(name);
        let _ = std::fs::write(&p, bytes);
        p
    }

    /// One second of 16-bit stereo 8 kHz: silence for the first half, a half-scale square after.
    fn wav() -> Vec<u8> {
        let mut pcm = Vec::new();
        for i in 0..8000 {
            let v: i16 = if i < 4000 {
                0
            } else if i % 2 == 0 {
                16384
            } else {
                -16384
            };
            pcm.extend_from_slice(&v.to_le_bytes());
            pcm.extend_from_slice(&v.to_le_bytes());
        }
        let mut b = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
        for v in [1u16, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&8000u32.to_le_bytes());
        b.extend_from_slice(&32_000u32.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        b.extend_from_slice(&pcm);
        b
    }

    #[test]
    fn wav_peaks_show_silence_then_signal() -> Result<()> {
        let w = waveform(
            &tmp("a.wav", &wav()),
            &WaveformOptions {
                columns: 10,
                max_seconds: 12.0,
            },
        )?;
        assert_eq!((w.peaks.len(), w.sample_rate, w.codec), (10, 8000, "pcm"));
        assert!((w.seconds - 1.0).abs() < 1e-9);
        assert!(w.peaks[..5].iter().all(|p| *p == 0.0), "{:?}", w.peaks);
        assert!(
            w.peaks[5..].iter().all(|p| (p - 0.5).abs() < 0.01),
            "{:?}",
            w.peaks
        );
        Ok(())
    }

    #[test]
    fn max_seconds_limits_what_is_read() -> Result<()> {
        let w = waveform(
            &tmp("b.wav", &wav()),
            &WaveformOptions {
                columns: 4,
                max_seconds: 0.25,
            },
        )?;
        assert!((w.seconds - 0.25).abs() < 1e-9);
        assert!(w.peaks.iter().all(|p| *p == 0.0));
        Ok(())
    }

    #[test]
    fn planar_and_pcm_paths_collect_and_skip_priming() {
        // Decoder output (planar) and raw PCM both go through `take`, which drops the priming
        // an edit list hides. A regression here once recursed forever.
        let mut m = Mono::new(10, 1.0);
        m.skip = 3;
        m.push_planar(&[
            vec![0.9, 0.9, 0.9, 0.1, -0.5],
            vec![0.0, 0.0, 0.0, 0.2, 0.0],
        ]);
        assert_eq!(m.samples, vec![0.2, 0.5]);
        let f = PcmFormat {
            bits: 16,
            channels: 1,
            float: false,
            big_endian: false,
        };
        m.push_pcm(&i16::MIN.to_le_bytes(), f);
        assert_eq!(m.samples, vec![0.2, 0.5, 1.0]);
        // The budget stops collection.
        let mut small = Mono::new(10, 0.1);
        small.push_planar(&[vec![0.5; 10]]);
        assert_eq!(small.samples.len(), 1);
    }

    #[test]
    fn pcm_sample_decoding() {
        let s16 = PcmFormat {
            bits: 16,
            channels: 1,
            float: false,
            big_endian: false,
        };
        assert_eq!(s16.sample(&i16::MIN.to_le_bytes()), -1.0);
        let s24be = PcmFormat {
            bits: 24,
            channels: 1,
            float: false,
            big_endian: true,
        };
        assert!((s24be.sample(&[0x40, 0, 0]) - 0.5).abs() < 1e-6);
        let u8f = PcmFormat {
            bits: 8,
            channels: 1,
            float: false,
            big_endian: false,
        };
        assert_eq!(u8f.sample(&[128]), 0.0);
        let f32be = PcmFormat {
            bits: 32,
            channels: 1,
            float: true,
            big_endian: true,
        };
        assert_eq!(f32be.sample(&0.25f32.to_be_bytes()), 0.25);
    }

    #[test]
    fn ogg_packets_span_pages() {
        let page = |serial: u32, lacing: &[u8], body: &[u8]| {
            let mut p = b"OggS\0\0".to_vec();
            p.extend_from_slice(&[0; 8]);
            p.extend_from_slice(&serial.to_le_bytes());
            p.extend_from_slice(&[0; 8]);
            p.push(lacing.len() as u8);
            p.extend_from_slice(lacing);
            p.extend_from_slice(body);
            p
        };
        let mut d = page(7, &[3, 255], &[b'a'; 258]);
        d.extend_from_slice(&page(9, &[2], b"xx")); // another stream: ignored
        d.extend_from_slice(&page(7, &[10], &[b'b'; 10]));
        let pk = ogg_packets(&d, 10);
        assert_eq!(pk.len(), 2);
        assert_eq!(pk[0], b"aaa");
        assert_eq!(pk[1].len(), 255 + 10);
    }

    #[test]
    fn unsupported_and_damaged_files_are_errors() {
        assert!(matches!(
            waveform(&tmp("c.bin", b"nothing here"), &WaveformOptions::default()),
            Err(Error::UnsupportedContainer)
        ));
        assert!(
            waveform(
                &tmp("d.wav", b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0"),
                &WaveformOptions::default()
            )
            .is_err()
        );
    }
}
