//! Stream probing with MediaCraft's own demuxers and parsers: no ffprobe.
//!
//! ISO BMFF (MP4, MOV, M4A, 3GP) goes through `craft-isobmff`, Matroska and WebM through
//! `craft-matroska`. Both read only box and element headers, never media payloads. WAV, AIFF,
//! FLAC, Ogg Opus/Vorbis, MP3 and ADTS AAC are read from their own headers. Containers
//! without a demuxer in this workspace (AVI, MXF, MPEG-TS/PS) give no stream info rather than a
//! guess. Codec names follow ffprobe's (`h264`, `hevc`, `prores`, `aac`, `pcm_s16le`, …) so
//! catalogues written by the ffprobe-based scanner read the same.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use crate::movie_meta::{self, MovieTags};
use crate::{AudioStreamInfo, VideoStreamInfo};

pub(crate) type Probed = (
    Option<VideoStreamInfo>,
    Option<AudioStreamInfo>,
    Option<f32>,
);

/// Bytes read from the start of audio-only files.
const HEAD: usize = 256 * 1024;
/// Bytes read from the end of Ogg files to find the last granule position.
const TAIL: u64 = 64 * 1024;

/// Probe a video or audio file. `None` when the format is not recognised or is damaged.
#[cfg(test)]
pub(crate) fn probe(path: &Path) -> Option<Probed> {
    probe_with_tags(path).map(|(p, _)| p)
}

/// [`probe`], plus the capture metadata MP4 and QuickTime movies carry.
pub(crate) fn probe_with_tags(path: &Path) -> Option<(Probed, MovieTags)> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut head = Vec::new();
    (&mut file).take(HEAD as u64).read_to_end(&mut head).ok()?;
    if is_bmff(&head) {
        return mp4(File::open(path).ok()?);
    }
    let probed = probe_other(path, &head, &mut file, size)?;
    Some((probed, MovieTags::default()))
}

fn probe_other(path: &Path, head: &[u8], file: &mut File, size: u64) -> Option<Probed> {
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return mkv(File::open(path).ok()?, size);
    }
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        return wav(head);
    }
    if head.starts_with(b"FORM") && matches!(head.get(8..12), Some(b"AIFF" | b"AIFC")) {
        return aiff(head);
    }
    if head.starts_with(b"fLaC") {
        return flac(head);
    }
    if head.starts_with(b"OggS") {
        return ogg(head, file, size);
    }
    if craft_mxf::sniff(head) {
        return mxf(path);
    }
    if craft_mpegts::sniff(head).is_some() {
        return mpeg_ts(path);
    }
    mpeg_audio(head, size)
}

/// A file as a positioned byte source for the MPEG-TS and MXF demuxers.
struct FileSource {
    file: Mutex<File>,
    len: u64,
}

impl FileSource {
    fn open(path: &Path) -> Option<Self> {
        let file = File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        Some(Self {
            file: Mutex::new(file),
            len,
        })
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let mut f = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(buf)
    }
}

impl craft_mpegts::ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        FileSource::read_at(self, offset, buf)
    }
}

impl craft_mxf::ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        FileSource::read_at(self, offset, buf)
    }
}

/// Size and bit depth from the first coded picture of an elementary stream.
fn elementary_video(codec: &str, unit: &[u8]) -> Option<(u32, u32, u8, Option<f32>)> {
    match codec {
        "mpeg1video" | "mpeg2video" => {
            let s = craft_mpeg2v::probe(unit)?;
            let fps = s
                .frame_rate
                .filter(|(_, d)| *d > 0)
                .map(|(n, d)| n as f32 / d as f32);
            Some((s.width, s.height, 8, fps))
        }
        "h264" => {
            let r = craft_h264::diagnose_h264_stream(unit);
            Some((
                r.width?,
                r.height?,
                r.bit_depth.and_then(|b| u8::try_from(b).ok()).unwrap_or(8),
                None,
            ))
        }
        "hevc" => {
            let r = craft_hevc::diagnose_hevc_stream(unit);
            Some((
                r.width?,
                r.height?,
                r.bit_depth.and_then(|b| u8::try_from(b).ok()).unwrap_or(8),
                None,
            ))
        }
        _ => None,
    }
}

/// MPEG transport and program streams (`.ts`, `.m2ts`, `.mts`, `.mpg`, `.vob`).
fn mpeg_ts(path: &Path) -> Option<Probed> {
    use craft_mpegts::{Codec, Kind};
    let src = FileSource::open(path)?;
    let m = craft_mpegts::open(&src).ok()?;
    let seconds = |s: &craft_mpegts::Stream| {
        s.pts_range()
            .map(|(a, b)| (b - a) as f64 / 90_000.0)
            .filter(|d| *d > 0.0)
    };

    let video = m.find(Kind::Video, |_| true).and_then(|si| {
        let s = m.streams.get(si)?;
        let codec = match &s.codec {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Mpeg2Video => "mpeg2video",
            Codec::Mpeg1Video => "mpeg1video",
            _ => return None,
        };
        let key = s.units.iter().position(|u| u.key).unwrap_or(0);
        let unit = m.read_unit(&src, si, key).ok()?;
        let (width, height, bit_depth, header_fps) = elementary_video(codec, &unit)?;
        // Frames over the timestamp span, one frame duration included.
        let fps = header_fps.or_else(|| {
            let n = s.units.len() as f64;
            seconds(s)
                .filter(|_| n > 1.0)
                .map(|d| ((n - 1.0) / d) as f32)
        });
        Some(VideoStreamInfo {
            codec: codec.into(),
            width,
            height,
            fps: fps.unwrap_or(0.0),
            color_space: "Unknown".into(),
            hdr: false,
            bit_depth,
        })
    });

    // Audio in a private stream (type 0x06 without a descriptor, as BDAV/M2TS writers do) is
    // identified by its first frame's header.
    let sniffed = |si: usize| -> Option<Codec> {
        let unit = m.read_unit(&src, si, 0).ok()?;
        [Codec::AacAdts, Codec::Ac3, Codec::Eac3, Codec::MpegAudio]
            .into_iter()
            .find(|c| craft_mpegts::frame_info(c, &unit).is_some())
    };
    let audio_stream = m
        .find(Kind::Audio, |_| true)
        .map(|si| (si, None))
        .or_else(|| {
            m.streams
                .iter()
                .enumerate()
                .filter(|(_, s)| matches!(s.codec, Codec::Unknown(_)))
                .find_map(|(si, _)| sniffed(si).map(|c| (si, Some(c))))
        });
    let audio = audio_stream.and_then(|(si, sniffed_codec)| {
        let s = m.streams.get(si)?;
        let stream_codec = sniffed_codec.unwrap_or_else(|| s.codec.clone());
        let unit = m.read_unit(&src, si, 0).ok()?;
        let info = craft_mpegts::frame_info(&stream_codec, &unit);
        let codec = match (&stream_codec, info.map(|i| i.variant)) {
            (Codec::MpegAudio, Some(3)) => "mp3".to_string(),
            (Codec::MpegAudio, Some(1)) => "mp1".to_string(),
            (Codec::MpegAudio, _) => "mp2".to_string(),
            (Codec::AacAdts | Codec::AacLatm, _) => "aac".to_string(),
            (Codec::Ac3, _) => "ac3".to_string(),
            (Codec::Eac3, _) => "eac3".to_string(),
            (Codec::Dts, _) => "dts".to_string(),
            (Codec::TrueHd, _) => "truehd".to_string(),
            (Codec::LpcmBluray | Codec::LpcmDvd, _) => "pcm".to_string(),
            (other, _) => other.name().to_ascii_lowercase(),
        };
        let (sample_rate, channels) = match (info, s.lpcm) {
            (Some(i), _) => (i.sample_rate, i.channels),
            (None, Some(l)) => (l.sample_rate, l.channels),
            (None, None) => (0, 0),
        };
        let bytes: u64 = s.units.iter().map(|u| u64::from(u.size)).sum();
        let kbps = seconds(s).map_or(0, |d| (bytes as f64 * 8.0 / d / 1000.0).round() as u32);
        Some(AudioStreamInfo {
            codec,
            sample_rate,
            channels: u8::try_from(channels).unwrap_or(u8::MAX),
            bit_depth: s.lpcm.and_then(|l| u8::try_from(l.bits).ok()).unwrap_or(0),
            bitrate_kbps: kbps,
        })
    });

    let duration = m
        .streams
        .iter()
        .filter(|s| matches!(s.kind(), Kind::Video | Kind::Audio))
        .filter_map(seconds)
        .fold(None, |a: Option<f64>, d| Some(a.map_or(d, |a| a.max(d))))
        .and_then(finite);
    (video.is_some() || audio.is_some()).then_some((video, audio, duration))
}

/// MXF: picture and sound descriptors from the header metadata.
fn mxf(path: &Path) -> Option<Probed> {
    use craft_mxf::{Codec, TrackKind};
    let src = FileSource::open(path)?;
    let m = craft_mxf::open(&src).ok()?;
    let rate = |r: craft_mxf::Rational| {
        (r.den > 0 && r.num > 0).then(|| f64::from(r.num) / f64::from(r.den))
    };

    let pic = m.tracks.iter().find(|t| t.kind == TrackKind::Picture);
    let video = pic.and_then(|t| {
        let p = t.picture.as_ref()?;
        let codec = match &t.codec {
            Codec::Avc { .. } => "h264",
            Codec::Vc3 => "dnxhd",
            Codec::ProRes { .. } => "prores",
            Codec::Mpeg2 => "mpeg2video",
            Codec::Mpeg4Visual => "mpeg4",
            Codec::Jpeg2000 => "jpeg2000",
            Codec::Dv => "dvvideo",
            Codec::Uncompressed => "rawvideo",
            _ => "unknown",
        };
        let (width, height) = if p.display_width > 0 && p.display_height > 0 {
            (p.display_width, p.display_height)
        } else {
            (p.stored_width, p.stored_height)
        };
        Some(VideoStreamInfo {
            codec: codec.into(),
            width,
            height,
            fps: rate(t.edit_rate).unwrap_or(0.0) as f32,
            color_space: "Unknown".into(),
            hdr: false,
            bit_depth: u8::try_from(p.component_depth)
                .ok()
                .filter(|b| *b > 0)
                .unwrap_or(8),
        })
    });
    let audio = m
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Sound)
        .and_then(|t| {
            let s = t.sound.as_ref()?;
            let sample_rate = rate(s.sample_rate)?.round() as u32;
            let bits = u8::try_from(s.bits).unwrap_or(0);
            Some(AudioStreamInfo {
                codec: pcm_name(false, false, s.bits as u16),
                sample_rate,
                channels: u8::try_from(s.channels).unwrap_or(u8::MAX),
                bit_depth: bits,
                bitrate_kbps: (u64::from(sample_rate) * u64::from(s.channels) * u64::from(s.bits)
                    / 1000) as u32,
            })
        });
    let duration = pic
        .or_else(|| m.tracks.first())
        .and_then(|t| {
            Some(t.duration.unwrap_or(t.samples.len() as i64) as f64 / rate(t.edit_rate)?)
        })
        .and_then(finite);
    (video.is_some() || audio.is_some()).then_some((video, audio, duration))
}

fn is_bmff(b: &[u8]) -> bool {
    matches!(
        b.get(4..8),
        Some(b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip")
    )
}

fn finite(v: f64) -> Option<f32> {
    (v.is_finite() && v > 0.0).then_some(v as f32)
}

/// ISO/IEC 23091-2 matrix coefficients as ffprobe names them.
fn matrix_name(m: u64) -> &'static str {
    match m {
        0 => "gbr",
        1 => "bt709",
        4 => "fcc",
        5 => "bt470bg",
        6 => "smpte170m",
        7 => "smpte240m",
        8 => "ycgco",
        9 => "bt2020nc",
        10 => "bt2020c",
        14 => "ictcp",
        _ => "Unknown",
    }
}

/// PQ (16) and HLG (18) transfer characteristics.
fn is_hdr_transfer(t: u64) -> bool {
    matches!(t, 16 | 18)
}

fn mp4(file: File) -> Option<(Probed, MovieTags)> {
    use craft_isobmff::{CodecConfig, ColorInfo, TrackKind};
    let m = craft_isobmff::open(file).ok()?;
    let secs = |dur: u64, scale: u32| (scale > 0).then(|| dur as f64 / f64::from(scale));
    let movie = secs(m.duration, m.timescale);

    let video = m
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .and_then(|t| {
            let codec = t.codec()?;
            let vp = t.video()?;
            // Frames over the summed sample durations (`stts`), as ffprobe's average rate is.
            // Neither `mdhd` nor the edit-list duration works: phones write `mdhd` longer than
            // the samples, and edit lists trim the presented length below them.
            let sample_ticks: u64 = t.samples.iter().map(|s| u64::from(s.duration)).sum();
            let fps = secs(sample_ticks, t.timescale)
                .filter(|s| *s > 0.0)
                .or_else(|| secs(t.duration, t.timescale).filter(|s| *s > 0.0))
                .map_or(0.0, |s| t.samples.len() as f64 / s);
            let (matrix, transfer) = match &vp.color {
                Some(
                    ColorInfo::Nclx {
                        matrix, transfer, ..
                    }
                    | ColorInfo::Nclc {
                        matrix, transfer, ..
                    },
                ) => (Some(u64::from(*matrix)), Some(u64::from(*transfer))),
                _ => (None, None),
            };
            let bit_depth = match codec {
                CodecConfig::Hevc(h) => h.bit_depth_luma,
                CodecConfig::Avc(a) => avc_bit_depth(a.profile, &a.ext),
                CodecConfig::Av1(a) => match (a.high_bitdepth, a.twelve_bit) {
                    (true, true) => 12,
                    (true, false) => 10,
                    _ => 8,
                },
                CodecConfig::Vp9(v) => v.bit_depth,
                CodecConfig::ProRes { fourcc } if matches!(&fourcc.0, b"ap4h" | b"ap4x") => 12,
                CodecConfig::ProRes { .. } | CodecConfig::Dnx { .. } => 10,
                _ => 8,
            };
            Some(VideoStreamInfo {
                codec: entry_name(t),
                width: u32::from(vp.width),
                height: u32::from(vp.height),
                fps: fps as f32,
                color_space: matrix.map_or("Unknown", matrix_name).to_string(),
                hdr: transfer.is_some_and(is_hdr_transfer),
                bit_depth,
            })
        });

    let audio = m
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .and_then(|t| {
            let codec = t.codec()?;
            let ap = t.audio()?;
            let (name, bit_depth) = match codec {
                CodecConfig::Pcm(p) => (pcm_name(p.float, p.big_endian, p.bits), p.bits),
                CodecConfig::Flac(f) => (entry_name(t), u16::from(f.bits_per_sample)),
                _ => (entry_name(t), 0),
            };
            let track_secs = secs(t.duration, t.timescale).filter(|s| *s > 0.0);
            let bytes: u64 = t.samples.iter().map(|s| u64::from(s.size)).sum();
            let kbps = track_secs.map_or(0.0, |s| bytes as f64 * 8.0 / s / 1000.0);
            let entry_rate = ap.sample_rate.round().clamp(0.0, f64::from(u32::MAX)) as u32;
            let (sample_rate, channels) = match codec {
                CodecConfig::Aac(aac) => aac_output(aac, entry_rate, ap.channels),
                _ => (entry_rate, ap.channels),
            };
            Some(AudioStreamInfo {
                codec: name,
                sample_rate,
                channels: u8::try_from(channels).unwrap_or(u8::MAX),
                bit_depth: u8::try_from(bit_depth).unwrap_or(0),
                bitrate_kbps: kbps.round().clamp(0.0, f64::from(u32::MAX)) as u32,
            })
        });

    let duration = movie.and_then(finite).or_else(|| {
        m.tracks
            .iter()
            .filter_map(|t| secs(t.duration, t.timescale))
            .fold(None, |a: Option<f64>, s| Some(a.map_or(s, |a| a.max(s))))
            .and_then(finite)
    });
    (video.is_some() || audio.is_some())
        .then(|| ((video, audio, duration), movie_meta::from_mp4(&m)))
}

/// The codec name, falling back to the sample entry's four-character code when the codec
/// configuration box is missing or unreadable (common in minimal or damaged files).
fn entry_name(t: &craft_isobmff::Track) -> String {
    let Some(entry) = t.entries.first() else {
        return "unknown".into();
    };
    if !matches!(entry.codec, craft_isobmff::CodecConfig::Unknown { .. }) {
        return entry.codec.name().to_string();
    }
    match &entry.format.0 {
        b"mp4a" => "aac".into(),
        b"mp4v" => "mpeg4".into(),
        // XDCAM, HDV and IMX: MPEG-2 video under QuickTime codes.
        f if f.starts_with(b"xd")
            || f.starts_with(b"hdv")
            || f.starts_with(b"mx")
            || f == b"mpg2"
            || f == b"m2v1" =>
        {
            "mpeg2video".into()
        }
        b"avc1" | b"avc3" => "h264".into(),
        b"hvc1" | b"hev1" => "hevc".into(),
        other => String::from_utf8_lossy(other).trim().to_ascii_lowercase(),
    }
}

/// AAC's real output rate and channel count come from the AudioSpecificConfig, not the sample
/// entry: entries often say "2 channels" regardless, and their 16.16 rate field cannot hold
/// 96 kHz. Explicitly signalled HE-AAC (SBR, object type 5) doubles the core rate, and
/// parametric stereo (object type 29) turns a mono core into stereo output.
fn aac_output(aac: &craft_isobmff::AacConfig, entry_rate: u32, entry_channels: u32) -> (u32, u32) {
    let core = if aac.sample_rate > 0 {
        aac.sample_rate
    } else {
        entry_rate
    };
    let rate = if matches!(aac.object_type, 5 | 29) && core <= 24_000 {
        core * 2
    } else {
        core
    };
    let channels = match aac.channels() {
        0 => entry_channels,
        1 if aac.object_type == 29 => 2,
        n => n,
    };
    (rate, channels)
}

/// High-profile AVC records carry the luma bit depth after the PPS list (ISO/IEC 14496-15
/// §5.3.3.1.2); other profiles are 8-bit.
fn avc_bit_depth(profile: u8, ext: &[u8]) -> u8 {
    match ext.get(1) {
        Some(b) if matches!(profile, 100 | 110 | 122 | 144 | 244) => 8 + (b & 7),
        _ => 8,
    }
}

fn pcm_name(float: bool, big_endian: bool, bits: u16) -> String {
    let kind = if float {
        'f'
    } else if bits == 8 {
        'u'
    } else {
        's'
    };
    let endian = if bits == 8 {
        ""
    } else if big_endian {
        "be"
    } else {
        "le"
    };
    format!("pcm_{kind}{bits}{endian}")
}

fn mkv(file: File, size: u64) -> Option<Probed> {
    use craft_matroska::TrackKind;
    let opts = craft_matroska::OpenOptions {
        index: false,
        ..Default::default()
    };
    let m = craft_matroska::open_with(file, &opts).ok()?;
    let duration = m
        .info
        .duration
        .map(|d| d * m.info.timestamp_scale as f64 / 1e9)
        .and_then(finite);
    let overall_kbps = duration.map_or(0, |d| {
        (size as f64 * 8.0 / f64::from(d) / 1000.0).round() as u32
    });

    let video = m
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .and_then(|t| {
            let v = t.video.as_ref()?;
            let colour = v.colour.as_ref();
            let bit_depth = colour
                .and_then(|c| c.bits_per_channel)
                .and_then(|b| u8::try_from(b).ok())
                .filter(|b| *b > 0)
                .unwrap_or(8);
            Some(VideoStreamInfo {
                codec: t.codec.name(),
                width: v.pixel_width,
                height: v.pixel_height,
                fps: t
                    .default_duration_ns
                    .filter(|d| *d > 0)
                    .map_or(0.0, |d| (1e9 / d as f64) as f32),
                color_space: colour
                    .and_then(|c| c.matrix_coefficients)
                    .map_or("Unknown", matrix_name)
                    .to_string(),
                hdr: colour
                    .and_then(|c| c.transfer_characteristics)
                    .is_some_and(is_hdr_transfer),
                bit_depth,
            })
        });
    let audio = m
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Audio)
        .and_then(|t| {
            let a = t.audio.as_ref()?;
            let rate = a.output_sampling_frequency.unwrap_or(a.sampling_frequency);
            Some(AudioStreamInfo {
                codec: t.codec.name(),
                sample_rate: rate.round().clamp(0.0, f64::from(u32::MAX)) as u32,
                channels: u8::try_from(a.channels).unwrap_or(u8::MAX),
                bit_depth: a.bit_depth.and_then(|b| u8::try_from(b).ok()).unwrap_or(0),
                // Without a cluster scan only the whole file's rate is known; use it for audio-only files.
                bitrate_kbps: if video.is_none() { overall_kbps } else { 0 },
            })
        });
    (video.is_some() || audio.is_some()).then_some((video, audio, duration))
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// RIFF WAVE: the `fmt ` and `data` chunks, wherever they are.
fn wav(b: &[u8]) -> Option<Probed> {
    let mut p = 12usize;
    let (mut fmt, mut data_len) = (None, None);
    while let (Some(id), Some(len)) = (b.get(p..p + 4), le32(b, p + 4)) {
        let body = p + 8;
        match id {
            b"fmt " => fmt = b.get(body..body + (len as usize).min(40)),
            // A streamed file may record 0 or 0xFFFFFFFF; the duration is then unknown.
            b"data" => {
                data_len = (len != 0 && len != u32::MAX).then_some(len);
                break;
            }
            _ => {}
        }
        p = body.checked_add(len as usize)? + (len as usize & 1);
    }
    let f = fmt?;
    let (tag, channels, rate, byte_rate, bits) = (
        le16(f, 0)?,
        le16(f, 2)?,
        le32(f, 4)?,
        le32(f, 8)?,
        le16(f, 14)?,
    );
    // WAVE_FORMAT_EXTENSIBLE keeps the real tag in the sub-format GUID.
    let tag = if tag == 0xFFFE {
        le16(f, 24).unwrap_or(1)
    } else {
        tag
    };
    let codec = match tag {
        1 => pcm_name(false, false, bits),
        3 => pcm_name(true, false, bits),
        6 => "pcm_alaw".into(),
        7 => "pcm_mulaw".into(),
        0x55 => "mp3".into(),
        _ => format!("wav_0x{tag:04x}"),
    };
    let duration = data_len
        .filter(|_| byte_rate > 0)
        .and_then(|d| finite(f64::from(d) / f64::from(byte_rate)));
    let audio = AudioStreamInfo {
        codec,
        sample_rate: rate,
        channels: u8::try_from(channels).ok()?,
        bit_depth: u8::try_from(bits).unwrap_or(0),
        bitrate_kbps: byte_rate * 8 / 1000,
    };
    Some((None, Some(audio), duration))
}

/// AIFF/AIFC: the `COMM` chunk (80-bit extended sample rate).
fn aiff(b: &[u8]) -> Option<Probed> {
    let aifc = b.get(8..12) == Some(b"AIFC");
    let mut p = 12usize;
    while let (Some(id), Some(len)) = (b.get(p..p + 4), be32(b, p + 4)) {
        let body = p + 8;
        if id == b"COMM" {
            let c = b.get(body..body + (len as usize).min(64))?;
            let channels = be16(c, 0)?;
            let frames = be32(c, 2)?;
            let bits = be16(c, 6)?;
            let rate = extended_to_f64(c.get(8..18)?)?;
            let compression = if aifc { c.get(18..22) } else { None };
            let codec = match compression {
                None | Some(b"NONE") => pcm_name(false, true, bits),
                Some(b"sowt") => pcm_name(false, false, bits),
                Some(b"fl32" | b"FL32") => pcm_name(true, true, 32),
                Some(b"fl64" | b"FL64") => pcm_name(true, true, 64),
                Some(other) => String::from_utf8_lossy(other).trim().to_ascii_lowercase(),
            };
            let duration = finite(f64::from(frames) / rate);
            let kbps = (rate * f64::from(channels) * f64::from(bits) / 1000.0).round() as u32;
            let audio = AudioStreamInfo {
                codec,
                sample_rate: rate.round() as u32,
                channels: u8::try_from(channels).ok()?,
                bit_depth: u8::try_from(bits).unwrap_or(0),
                bitrate_kbps: kbps,
            };
            return Some((None, Some(audio), duration));
        }
        p = body.checked_add(len as usize)? + (len as usize & 1);
    }
    None
}

/// IEEE 754 80-bit extended, as AIFF stores sample rates.
fn extended_to_f64(b: &[u8]) -> Option<f64> {
    let exp = i32::from(u16::from_be_bytes([b[0], b[1]]) & 0x7FFF);
    let mantissa = u64::from_be_bytes(b.get(2..10)?.try_into().ok()?);
    if exp == 0 || mantissa == 0 {
        return None;
    }
    let v = mantissa as f64 * 2f64.powi(exp - 16383 - 63);
    (v.is_finite() && v > 0.0 && v < 1e7).then_some(v)
}

/// FLAC: STREAMINFO, the mandatory first metadata block.
fn flac(b: &[u8]) -> Option<Probed> {
    if b.get(4).map(|t| t & 0x7F) != Some(0) {
        return None;
    }
    let s = b.get(8..8 + 34)?;
    let rate = (u32::from(s[10]) << 12) | (u32::from(s[11]) << 4) | (u32::from(s[12]) >> 4);
    let channels = ((s[12] >> 1) & 7) + 1;
    let bits = (((s[12] & 1) << 4) | (s[13] >> 4)) + 1;
    let total = (u64::from(s[13] & 0x0F) << 32) | u64::from(be32(s, 14)?);
    if rate == 0 {
        return None;
    }
    let duration = finite(total as f64 / f64::from(rate));
    let audio = AudioStreamInfo {
        codec: "flac".into(),
        sample_rate: rate,
        channels,
        bit_depth: bits,
        bitrate_kbps: 0,
    };
    Some((None, Some(audio), duration))
}

/// Ogg: the first page's identification header (Opus or Vorbis), and the last page's granule
/// position for the duration.
fn ogg(head: &[u8], file: &mut File, size: u64) -> Option<Probed> {
    let segments = usize::from(*head.get(26)?);
    let packet = head.get(27 + segments..)?;
    let (codec, rate, channels, pre_skip, granule_rate) = if packet.starts_with(b"OpusHead") {
        let channels = *packet.get(9)?;
        let pre_skip = u64::from(le16(packet, 10)?);
        let input_rate = le32(packet, 12)?;
        // Opus granules always count 48 kHz samples; the input rate is informational.
        (
            "opus",
            if input_rate > 0 { input_rate } else { 48_000 },
            channels,
            pre_skip,
            48_000u32,
        )
    } else if packet.get(1..7) == Some(b"vorbis") && packet.first() == Some(&1) {
        let channels = *packet.get(11)?;
        let rate = le32(packet, 12)?;
        ("vorbis", rate, channels, 0, rate)
    } else {
        return None;
    };
    let start = size.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.take(TAIL).read_to_end(&mut tail).ok()?;
    let granule = tail
        .windows(4)
        .enumerate()
        .rev()
        .filter(|(_, w)| *w == b"OggS")
        .find_map(|(i, _)| {
            tail.get(i + 6..i + 14)
                .map(|g| u64::from_le_bytes(g.try_into().unwrap_or([0; 8])))
        })
        .filter(|g| *g != u64::MAX);
    let duration = granule
        .filter(|_| granule_rate > 0)
        .and_then(|g| finite(g.saturating_sub(pre_skip) as f64 / f64::from(granule_rate)));
    let kbps = duration.map_or(0, |d| {
        (size as f64 * 8.0 / f64::from(d) / 1000.0).round() as u32
    });
    let audio = AudioStreamInfo {
        codec: codec.into(),
        sample_rate: rate,
        channels,
        bit_depth: 0,
        bitrate_kbps: kbps,
    };
    Some((None, Some(audio), duration))
}

/// MP3 (MPEG-1/2/2.5 Layer III, after any ID3v2 tag; Xing/Info frame count when present) and
/// ADTS AAC.
fn mpeg_audio(b: &[u8], size: u64) -> Option<Probed> {
    let mut p = 0usize;
    if b.starts_with(b"ID3") {
        let s = b.get(6..10)?;
        let tag = (usize::from(s[0] & 0x7F) << 21)
            | (usize::from(s[1] & 0x7F) << 14)
            | (usize::from(s[2] & 0x7F) << 7)
            | usize::from(s[3] & 0x7F);
        p = 10
            + tag
            + if b.get(5).is_some_and(|f| f & 0x10 != 0) {
                10
            } else {
                0
            };
    }
    let h = b.get(p..p + 4)?;
    if h[0] != 0xFF || h[1] & 0xE0 != 0xE0 {
        return None;
    }
    let layer = (h[1] >> 1) & 3;
    if layer == 0 && h[1] & 0xF6 == 0xF0 {
        return adts(b, p, size);
    }
    if layer != 1 {
        return None; // Layer I/II: not probed
    }
    let version = (h[1] >> 3) & 3; // 3 = MPEG-1, 2 = MPEG-2, 0 = MPEG-2.5
    let bitrate_idx = usize::from(h[2] >> 4);
    let rate_idx = usize::from((h[2] >> 2) & 3);
    let mono = (h[3] >> 6) == 3;
    const V1: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const V2: [u32; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    let base_rate = *[44_100u32, 48_000, 32_000].get(rate_idx)?;
    let (rate, kbps, samples_per_frame) = match version {
        3 => (base_rate, V1[bitrate_idx], 1152u32),
        2 => (base_rate / 2, V2[bitrate_idx], 576),
        0 => (base_rate / 4, V2[bitrate_idx], 576),
        _ => return None,
    };
    if kbps == 0 {
        return None;
    }
    // Xing/Info header: side info is 32/17 bytes (MPEG-1 stereo/mono) or 17/9 (MPEG-2/2.5).
    let side = match (version == 3, mono) {
        (true, false) => 32,
        (true, true) => 17,
        (false, false) => 17,
        (false, true) => 9,
    };
    let x = p + 4 + side;
    let frames = match b.get(x..x + 4) {
        Some(b"Xing" | b"Info") if be32(b, x + 4).is_some_and(|f| f & 1 == 1) => be32(b, x + 8),
        _ => None,
    };
    let audio_bytes = size.saturating_sub(p as u64);
    let duration = match frames {
        Some(n) => finite(f64::from(n) * f64::from(samples_per_frame) / f64::from(rate)),
        None => finite(audio_bytes as f64 * 8.0 / (f64::from(kbps) * 1000.0)),
    };
    let avg_kbps = match (frames, duration) {
        (Some(_), Some(d)) => (audio_bytes as f64 * 8.0 / f64::from(d) / 1000.0).round() as u32,
        _ => kbps,
    };
    let audio = AudioStreamInfo {
        codec: "mp3".into(),
        sample_rate: rate,
        channels: if mono { 1 } else { 2 },
        bit_depth: 0,
        bitrate_kbps: avg_kbps,
    };
    Some((None, Some(audio), duration))
}

/// ADTS AAC: the header's sample rate and channels, and the average frame size of the frames
/// in the probed prefix for the duration.
fn adts(b: &[u8], start: usize, size: u64) -> Option<Probed> {
    const RATES: [u32; 13] = [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ];
    let h = b.get(start..start + 7)?;
    let rate = *RATES.get(usize::from((h[2] >> 2) & 0x0F))?;
    let channels = ((h[2] & 1) << 2) | (h[3] >> 6);
    let (mut p, mut frames, mut bytes) = (start, 0u64, 0u64);
    while let Some(f) = b.get(p..p + 7) {
        if f[0] != 0xFF || f[1] & 0xF6 != 0xF0 {
            break;
        }
        let len = (usize::from(f[3] & 3) << 11) | (usize::from(f[4]) << 3) | usize::from(f[5] >> 5);
        if len < 7 || p + len > b.len() {
            break;
        }
        frames += 1;
        bytes += len as u64;
        p += len;
    }
    if frames == 0 {
        return None;
    }
    let frame_secs = 1024.0 / f64::from(rate);
    let total_frames = size.saturating_sub(start as u64) as f64 / (bytes as f64 / frames as f64);
    let duration = finite(total_frames * frame_secs);
    let kbps = (bytes as f64 * 8.0 / (frames as f64 * frame_secs) / 1000.0).round() as u32;
    let audio = AudioStreamInfo {
        codec: "aac".into(),
        sample_rate: rate,
        channels,
        bit_depth: 0,
        bitrate_kbps: kbps,
    };
    Some((None, Some(audio), duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mc_av_probe_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(name);
        let _ = std::fs::write(&path, bytes);
        path
    }

    #[test]
    fn m4a_through_craft_isobmff() -> Result<(), String> {
        let bytes = mediacraft_bitstream::isobmff::build_synthetic_m4a(
            &[0x11; 4000],
            48_000,
            2,
            1024,
            true,
        );
        let (video, audio, duration) = probe(&write("tone.m4a", &bytes)).ok_or("not probed")?;
        assert!(video.is_none());
        let a = audio.ok_or("no audio")?;
        assert_eq!(
            (a.codec.as_str(), a.sample_rate, a.channels),
            ("aac", 48_000, 2)
        );
        assert!(duration.is_some_and(|d| d > 0.0), "{duration:?}");
        Ok(())
    }

    #[test]
    fn webm_through_craft_matroska() -> Result<(), String> {
        use craft_matroska::{MkvWriter, MuxOptions, TrackKind, TrackSpec};
        let mut v = TrackSpec::new(TrackKind::Video, "V_VP9");
        v.video_size = Some((1280, 720));
        v.default_duration_ns = Some(40_000_000);
        let mut a = TrackSpec::new(TrackKind::Audio, "A_PCM/INT/LIT");
        a.audio = Some((48_000.0, 2, Some(16)));
        let mut w = MkvWriter::new(
            std::io::Cursor::new(Vec::new()),
            vec![v, a],
            MuxOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        for i in 0..25i64 {
            w.write_frame(
                0,
                i * 40_000_000,
                i == 0,
                &[0x82, 0x49, 0x83, 0x42, 0],
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        let bytes = w.finish().map_err(|e| e.to_string())?.into_inner();
        let (video, audio, _) = probe(&write("clip.mkv", &bytes)).ok_or("not probed")?;
        let v = video.ok_or("no video")?;
        assert_eq!((v.codec.as_str(), v.width, v.height), ("vp9", 1280, 720));
        assert!((v.fps - 25.0).abs() < 0.01, "{}", v.fps);
        let a = audio.ok_or("no audio")?;
        assert_eq!(
            (a.codec.as_str(), a.sample_rate, a.channels, a.bit_depth),
            ("pcm_s16le", 48_000, 2, 16)
        );
        Ok(())
    }

    #[test]
    fn wav_with_extra_chunks_before_data() -> Result<(), String> {
        let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
        b.extend_from_slice(b"fmt \x10\0\0\0");
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&44_100u32.to_le_bytes());
        b.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"LIST\x03\0\0\0abc\0"); // odd length, padded
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(44_100u32 * 4 * 2).to_le_bytes());
        let (_, audio, duration) = wav(&b).ok_or("not probed")?;
        let a = audio.ok_or("no audio")?;
        assert_eq!(
            (a.codec.as_str(), a.sample_rate, a.channels, a.bitrate_kbps),
            ("pcm_s16le", 44_100, 2, 1411)
        );
        assert_eq!(duration, Some(2.0));
        Ok(())
    }

    #[test]
    fn flac_streaminfo() -> Result<(), String> {
        let mut b = b"fLaC\x00\0\0\x22".to_vec();
        let mut si = [0u8; 34];
        // 48000 Hz, 2 channels, 24 bits, 96000 samples.
        let rate = 48_000u32;
        si[10] = (rate >> 12) as u8;
        si[11] = (rate >> 4) as u8;
        si[12] = ((rate & 0xF) << 4) as u8 | (1 << 1) | ((23 >> 4) & 1);
        si[13] = ((23 & 0xF) << 4) as u8;
        si[14..18].copy_from_slice(&96_000u32.to_be_bytes());
        b.extend_from_slice(&si);
        let (_, audio, duration) = flac(&b).ok_or("not probed")?;
        let a = audio.ok_or("no audio")?;
        assert_eq!((a.sample_rate, a.channels, a.bit_depth), (48_000, 2, 24));
        assert_eq!(duration, Some(2.0));
        Ok(())
    }

    #[test]
    fn mp3_cbr_after_id3_and_adts() -> Result<(), String> {
        // ID3v2 with a 16-byte body, then an MPEG-1 Layer III 128 kbps 44.1 kHz stereo frame.
        let mut b = b"ID3\x04\0\0\0\0\0\x10".to_vec();
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        b.resize(26 + 16_000, 0);
        let (_, audio, duration) = mpeg_audio(&b, b.len() as u64).ok_or("mp3 not probed")?;
        let a = audio.ok_or("no audio")?;
        assert_eq!(
            (a.codec.as_str(), a.sample_rate, a.channels, a.bitrate_kbps),
            ("mp3", 44_100, 2, 128)
        );
        assert!(
            duration.is_some_and(|d| (d - 1.0).abs() < 0.01),
            "{duration:?}"
        );

        // Ten 48 kHz stereo ADTS frames of 200 bytes.
        let mut adts_bytes = Vec::new();
        for _ in 0..10 {
            let len = 200usize;
            let mut f = vec![
                0xFF,
                0xF1,
                (1 << 6) | (3 << 2),
                (2 << 6) | ((len >> 11) as u8 & 3),
                (len >> 3) as u8,
                ((len & 7) << 5) as u8 | 0x1F,
                0xFC,
            ];
            f.resize(len, 0);
            adts_bytes.extend_from_slice(&f);
        }
        let (_, audio, _) =
            mpeg_audio(&adts_bytes, adts_bytes.len() as u64).ok_or("adts not probed")?;
        let a = audio.ok_or("no audio")?;
        assert_eq!(
            (a.codec.as_str(), a.sample_rate, a.channels),
            ("aac", 48_000, 2)
        );
        Ok(())
    }

    #[test]
    fn aac_output_comes_from_the_audio_specific_config() {
        let aac = |object_type, sample_rate, channel_config| craft_isobmff::AacConfig {
            object_type,
            sample_rate,
            channel_config,
            ..craft_isobmff::AacConfig::from_asc(Vec::new())
        };
        // The entry says 30464 Hz (96 kHz overflowing 16.16) and 2 channels; the ASC is right.
        assert_eq!(aac_output(&aac(2, 96_000, 1), 30_464, 2), (96_000, 1));
        // HE-AAC: 24 kHz core plays at 48 kHz. HE-AAC v2: mono core plays as stereo.
        assert_eq!(aac_output(&aac(5, 24_000, 2), 24_000, 2), (48_000, 2));
        assert_eq!(aac_output(&aac(29, 22_050, 1), 22_050, 2), (44_100, 2));
        // No ASC: keep what the entry says.
        assert_eq!(aac_output(&aac(0, 0, 0), 44_100, 2), (44_100, 2));
    }

    #[test]
    fn unknown_and_damaged_input_gives_none() {
        assert!(probe(&write("junk.mp4", b"not a media file at all")).is_none());
        assert!(probe(&write("cut.mkv", &[0x1A, 0x45, 0xDF, 0xA3, 0x9F])).is_none());
        assert!(wav(b"RIFF\0\0\0\0WAVEfmt \xFF\xFF\xFF\xFF").is_none());
        assert!(flac(b"fLaC\x00\0\0\x22").is_none());
        assert!(mpeg_audio(&[0xFF, 0xFB, 0x00, 0x00], 4).is_none());
        assert_eq!(extended_to_f64(&[0; 10]), None);
    }
}
