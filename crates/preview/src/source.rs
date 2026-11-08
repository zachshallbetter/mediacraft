//! Containers as one shape: a decoder, each sample's presentation time and sync flag in
//! decode order, a sample reader, and how to crop and turn the picture.

use crate::codec::Decoder;
use crate::{Error, Result};
use craft_image::Rgba8Image;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

/// A crop rectangle in coded pixels: x, y, width, height.
pub(crate) type Rect = (u32, u32, u32, u32);

/// How the container says to crop.
pub(crate) enum Crop {
    None,
    /// MP4/MOV `clap`.
    CleanAperture(craft_isobmff::CleanAperture),
    /// Matroska `PixelCrop*`: top, bottom, left, right.
    Pixels((u32, u32, u32, u32)),
}

impl Crop {
    pub(crate) fn rect(&self, w: u32, h: u32) -> Option<Rect> {
        match self {
            Crop::None => None,
            Crop::CleanAperture(c) => clean_aperture(c, w, h),
            Crop::Pixels(p) => matroska_crop(*p, w, h),
        }
    }
}

type Reader = Box<dyn Fn(usize) -> Result<Vec<u8>>>;

pub(crate) struct Source {
    pub decoder: Decoder,
    /// Presentation time (ticks) and sync flag of each sample, in decode order.
    pub samples: Vec<(i64, bool)>,
    pub ticks_per_second: f64,
    pub read: Reader,
    pub crop: Crop,
    /// Clockwise quarter turns for display.
    pub quarter_turns: u8,
    /// Sample times are stored (decode) positions, not presentation times: MXF whose index has
    /// no temporal offsets. Presentation order then comes from the decoder's output order.
    pub decode_order_times: bool,
}

/// A file as a positioned byte source for the MPEG-TS and MXF demuxers.
struct FileSource {
    file: Mutex<File>,
    len: u64,
}

impl FileSource {
    fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
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

fn demux(e: impl std::fmt::Display) -> Error {
    Error::Demux(e.to_string())
}

fn unsupported(name: impl Into<String>) -> Error {
    Error::UnsupportedCodec(name.into())
}

/// Open the first video track of any container this crate reads.
pub(crate) fn open(path: &Path) -> Result<Source> {
    let mut head = vec![0u8; 4096];
    let n = std::io::Read::read(&mut File::open(path)?, &mut head)?;
    head.truncate(n);
    if matches!(
        head.get(4..8),
        Some(b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip")
    ) {
        mp4(path)
    } else if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        mkv(path)
    } else if craft_mxf::sniff(&head) {
        mxf(path)
    } else if craft_mpegts::sniff(&head).is_some() {
        ts(path)
    } else {
        Err(Error::UnsupportedContainer)
    }
}

/// QuickTime four-character codes that carry MPEG-2 video (XDCAM, HDV, IMX).
fn mpeg2_fourcc(f: &[u8; 4]) -> bool {
    f.starts_with(b"xd")
        || f.starts_with(b"hdv")
        || f.starts_with(b"mx3")
        || f.starts_with(b"mx4")
        || f.starts_with(b"mx5")
        || f == b"mpg2"
        || f == b"m2v1"
}

fn mp4(path: &Path) -> Result<Source> {
    use craft_isobmff::{CodecConfig, TrackKind};
    let file = File::open(path)?;
    let m = craft_isobmff::open(&file).map_err(demux)?;
    let ti = m.track_of_kind(TrackKind::Video).ok_or(Error::NoVideo)?;
    let t = m.tracks.get(ti).ok_or(Error::NoVideo)?;
    let decoder = match t.codec() {
        Some(CodecConfig::Avc(c)) => Decoder::h264(Some(&c.to_bytes()))?,
        Some(CodecConfig::Hevc(c)) => Decoder::hevc(Some(&c.to_bytes()))?,
        Some(CodecConfig::ProRes { .. }) => Decoder::ProRes,
        Some(CodecConfig::Dnx { .. }) => Decoder::Dnx,
        Some(CodecConfig::Av1(c)) => Decoder::av1(&c.config_obus),
        Some(CodecConfig::Vp9(_)) => Decoder::vp9(),
        Some(CodecConfig::Jpeg { .. }) => Decoder::Mjpeg,
        Some(CodecConfig::Unknown { fourcc, .. }) if mpeg2_fourcc(&fourcc.0) => Decoder::mpeg2(),
        Some(other) => return Err(unsupported(other.name())),
        None => return Err(unsupported("unknown")),
    };
    let samples = t
        .samples
        .iter()
        .map(|s| (s.pts + t.edit_offset, s.is_sync))
        .collect();
    let crop = t
        .video()
        .and_then(|v| v.clean_aperture)
        .map_or(Crop::None, Crop::CleanAperture);
    let quarter_turns = t.display_rotation().unwrap_or(0);
    let ticks_per_second = f64::from(t.timescale.max(1));
    Ok(Source {
        decoder,
        samples,
        ticks_per_second,
        crop,
        quarter_turns,
        decode_order_times: false,
        read: Box::new(move |i| m.read_sample(&file, ti, i).map_err(demux)),
    })
}

fn mkv(path: &Path) -> Result<Source> {
    use craft_matroska::{Codec, TrackKind};
    let file = File::open(path)?;
    let m = craft_matroska::open(&file).map_err(demux)?;
    let ti = m
        .tracks
        .iter()
        .position(|t| t.kind == TrackKind::Video)
        .ok_or(Error::NoVideo)?;
    let t = m.tracks.get(ti).ok_or(Error::NoVideo)?;
    let decoder = match &t.codec {
        Codec::Avc { avcc } => Decoder::h264(Some(avcc))?,
        Codec::Hevc { hvcc } => Decoder::hevc(Some(hvcc))?,
        Codec::Vp8 => Decoder::vp8(),
        Codec::Vp9 { .. } => Decoder::vp9(),
        // av1C: a 4-byte header, then configuration OBUs.
        Codec::Av1 { av1c } => Decoder::av1(av1c.get(4..).unwrap_or(&[])),
        Codec::ProRes { .. } => Decoder::ProRes,
        Codec::Mjpeg => Decoder::Mjpeg,
        other => return Err(unsupported(other.name())),
    };
    let samples = t.samples.iter().map(|s| (s.pts, s.keyframe)).collect();
    let (num, den) = (t.timebase.0.max(1) as f64, t.timebase.1.max(1) as f64);
    let crop = t
        .video
        .as_ref()
        .map_or(Crop::None, |v| Crop::Pixels(v.crop));
    let quarter_turns = t
        .video
        .as_ref()
        .and_then(|v| v.display_rotation())
        .unwrap_or(0);
    Ok(Source {
        decoder,
        samples,
        ticks_per_second: den / num,
        crop,
        quarter_turns,
        decode_order_times: false,
        read: Box::new(move |i| m.read_sample(&file, ti, i).map_err(demux)),
    })
}

/// MPEG transport and program streams (`.ts`, `.m2ts`, `.mts`, `.mpg`, `.vob`).
fn ts(path: &Path) -> Result<Source> {
    use craft_mpegts::{Codec, Kind};
    let file = FileSource::open(path)?;
    let m = craft_mpegts::open(&file).map_err(demux)?;
    let si = m.find(Kind::Video, |_| true).ok_or(Error::NoVideo)?;
    let s = m.streams.get(si).ok_or(Error::NoVideo)?;
    let decoder = match &s.codec {
        Codec::H264 => Decoder::h264(None)?,
        Codec::Hevc => Decoder::hevc(None)?,
        Codec::Mpeg1Video | Codec::Mpeg2Video => Decoder::mpeg2(),
        other => return Err(unsupported(other.name())),
    };
    // 90 kHz timestamps, made relative to the first; units without one keep the previous time.
    let mut last = 0i64;
    let raw: Vec<(i64, bool)> = s
        .units
        .iter()
        .map(|u| {
            last = u.pts.or(u.dts).unwrap_or(last);
            (last, u.key)
        })
        .collect();
    let first = raw.iter().map(|(p, _)| *p).min().unwrap_or(0);
    let samples = raw.into_iter().map(|(p, k)| (p - first, k)).collect();
    Ok(Source {
        decoder,
        samples,
        ticks_per_second: 90_000.0,
        crop: Crop::None,
        quarter_turns: 0,
        decode_order_times: false,
        read: Box::new(move |i| m.read_unit(&file, si, i).map_err(demux)),
    })
}

fn mxf(path: &Path) -> Result<Source> {
    use craft_mxf::{Codec, TrackKind};
    let file = FileSource::open(path)?;
    let m = craft_mxf::open(&file).map_err(demux)?;
    let ti = m
        .tracks
        .iter()
        .position(|t| t.kind == TrackKind::Picture)
        .ok_or(Error::NoVideo)?;
    let t = m.tracks.get(ti).ok_or(Error::NoVideo)?;
    let decoder = match &t.codec {
        Codec::Avc { .. } => Decoder::h264(None)?,
        Codec::Vc3 => Decoder::Dnx,
        Codec::ProRes { .. } => Decoder::ProRes,
        Codec::Mpeg2 => Decoder::mpeg2(),
        other => return Err(unsupported(other.name())),
    };
    let intra = decoder.intra_only();
    // Time zero is the first presented edit unit (origin + start position), not the first stored
    // one: encoders with B-frames set an origin equal to their reordering delay.
    let first = t.first_edit_unit();
    let samples = t.samples.iter().map(|s| (s.pts - first, s.key)).collect();
    let rate = f64::from(t.edit_rate.num) / f64::from(t.edit_rate.den.max(1));
    Ok(Source {
        decoder,
        samples,
        ticks_per_second: if rate > 0.0 { rate } else { 25.0 },
        crop: Crop::None,
        quarter_turns: 0,
        decode_order_times: !t.temporal_offsets && !intra,
        read: Box::new(move |i| m.read_sample(&file, ti, i).map_err(demux)),
    })
}

/// The `clap` clean aperture of an MP4/MOV track (ISO/IEC 14496-12 §12.1.4): a rectangle of
/// the given size centred on the picture centre plus the offsets. iPhone Live Photo clips use
/// it to hide stabilisation margins; players crop to it.
pub(crate) fn clean_aperture(c: &craft_isobmff::CleanAperture, w: u32, h: u32) -> Option<Rect> {
    let rat = |n: f64, d: u32| (d > 0).then(|| n / f64::from(d));
    let cw = rat(f64::from(c.width.0), c.width.1)?.round();
    let ch = rat(f64::from(c.height.0), c.height.1)?.round();
    let ho = rat(f64::from(c.horiz_offset.0), c.horiz_offset.1)?;
    let vo = rat(f64::from(c.vert_offset.0), c.vert_offset.1)?;
    if cw < 1.0 || ch < 1.0 || cw > f64::from(w) || ch > f64::from(h) {
        return None;
    }
    let x = ((f64::from(w) - cw) / 2.0 + ho)
        .round()
        .clamp(0.0, f64::from(w) - cw);
    let y = ((f64::from(h) - ch) / 2.0 + vo)
        .round()
        .clamp(0.0, f64::from(h) - ch);
    let r = (x as u32, y as u32, cw as u32, ch as u32);
    (r != (0, 0, w, h)).then_some(r)
}

/// Matroska `PixelCrop*` (top, bottom, left, right) as a rectangle.
pub(crate) fn matroska_crop(
    (top, bottom, left, right): (u32, u32, u32, u32),
    w: u32,
    h: u32,
) -> Option<Rect> {
    let cw = w.checked_sub(left)?.checked_sub(right)?;
    let ch = h.checked_sub(top)?.checked_sub(bottom)?;
    (cw > 0 && ch > 0 && (cw, ch) != (w, h)).then_some((left, top, cw, ch))
}

pub(crate) fn crop_to(img: &Rgba8Image, (x, y, w, h): Rect) -> Option<Rgba8Image> {
    if x.checked_add(w)? > img.width() || y.checked_add(h)? > img.height() || w == 0 || h == 0 {
        return None;
    }
    let stride = img.width() as usize * 4;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for row in y..y + h {
        let start = row as usize * stride + x as usize * 4;
        out.extend_from_slice(img.data().get(start..start + w as usize * 4)?);
    }
    Rgba8Image::new(w, h, out).ok()
}
