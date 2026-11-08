//! # mediacraft-preview
//!
//! Poster frames from video files with MediaCraft's own demuxers and decoders, no ffmpeg:
//! MP4/MOV through `craft-isobmff` and MKV/WebM through `craft-matroska`; H.264, HEVC and
//! ProRes through `craft-h264`, `craft-hevc` and `craft-prores`.
//!
//! A poster frame seeks to the sync sample before the requested time and decodes forward only
//! to that time, at most [`FrameOptions::max_decode_frames`] frames. It converts Y′CbCr to RGB
//! with the stream's own matrix and range, turns the frame upright by the track's display
//! matrix, and fits it within [`FrameOptions::max_edge`] through `craft-image`'s linear-light
//! resampler.
//!
//! ```no_run
//! let frame = mediacraft_preview::poster_frame(
//!     std::path::Path::new("clip.mov"),
//!     &mediacraft_preview::FrameOptions::at(2.0),
//! )?;
//! println!("{}×{} from {}", frame.image.width(), frame.image.height(), frame.codec);
//! # Ok::<(), mediacraft_preview::Error>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod waveform;
mod yuv;

pub use waveform::{Waveform, WaveformOptions, waveform};

use craft_image::{Orientation, ResizeOptions, Rgba8Image, resize_to_fit};
use std::fs::File;
use std::path::Path;
use yuv::Plane;

/// Why a preview could not be made.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading the file failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Not a container this crate reads.
    #[error("not an MP4, MOV, MKV or WebM file")]
    UnsupportedContainer,
    /// The container has no video track.
    #[error("no video track")]
    NoVideo,
    /// The container has no audio track.
    #[error("no audio track")]
    NoAudio,
    /// No decoder in this build for the codec.
    #[error("no decoder for {0}")]
    UnsupportedCodec(String),
    /// The demuxer rejected the file.
    #[error("demux failed: {0}")]
    Demux(String),
    /// The decoder rejected the data.
    #[error("decode failed: {0}")]
    Decode(String),
    /// Decoding produced no picture.
    #[error("no frame could be decoded")]
    NoFrame,
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Poster-frame request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameOptions {
    /// Presentation time to show, in seconds; clamped to the file's length.
    pub at_seconds: f64,
    /// Fit within `max_edge × max_edge`.
    pub max_edge: u32,
    /// Most frames decoded from the sync sample to the target. Past that, the sync frame is shown.
    pub max_decode_frames: usize,
    /// Resampling.
    pub resize: ResizeOptions,
}

impl FrameOptions {
    /// A 1280-pixel poster frame at `seconds`.
    pub fn at(seconds: f64) -> Self {
        Self {
            at_seconds: seconds,
            max_edge: 1280,
            max_decode_frames: 300,
            resize: ResizeOptions::default(),
        }
    }
}

/// A decoded poster frame.
#[derive(Clone, Debug)]
pub struct PosterFrame {
    /// Upright 8-bit RGBA, within `max_edge`.
    pub image: Rgba8Image,
    /// Presentation time of the frame shown, in seconds.
    pub seconds: f64,
    /// Codec name (`h264`, `hevc`, `prores`).
    pub codec: &'static str,
    /// Coded size before rotation and fitting.
    pub source_width: u32,
    /// See [`PosterFrame::source_width`].
    pub source_height: u32,
}

/// Decode the frame at `opts.at_seconds`.
pub fn poster_frame(path: &Path, opts: &FrameOptions) -> Result<PosterFrame> {
    let mut head = [0u8; 8];
    let n = std::io::Read::read(&mut File::open(path)?, &mut head)?;
    let head = head.get(..n).unwrap_or(&[]);
    let frame = if matches!(
        head.get(4..8),
        Some(b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip")
    ) {
        mp4(path, opts)?
    } else if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        mkv(path, opts)?
    } else {
        return Err(Error::UnsupportedContainer);
    };
    Ok(frame)
}

enum Decoder {
    H264(Box<craft_h264::Decoder>),
    Hevc(Box<craft_hevc::Decoder>),
    ProRes,
}

enum Picture {
    H264(craft_h264::Picture),
    Hevc(craft_hevc::Picture),
    ProRes(craft_prores::Frame, i64),
}

impl Decoder {
    fn name(&self) -> &'static str {
        match self {
            Decoder::H264(_) => "h264",
            Decoder::Hevc(_) => "hevc",
            Decoder::ProRes => "prores",
        }
    }

    fn decode(&mut self, data: &[u8], pts: i64) -> Result<Vec<Picture>> {
        let err = |e: &dyn std::fmt::Display| Error::Decode(e.to_string());
        Ok(match self {
            Decoder::H264(d) => d
                .decode(data, pts)
                .map_err(|e| err(&e))?
                .into_iter()
                .map(Picture::H264)
                .collect(),
            Decoder::Hevc(d) => d
                .decode(data, pts)
                .map_err(|e| err(&e))?
                .into_iter()
                .map(Picture::Hevc)
                .collect(),
            Decoder::ProRes => vec![Picture::ProRes(
                craft_prores::decode_frame(data).map_err(|e| err(&e))?,
                pts,
            )],
        })
    }

    fn flush(&mut self) -> Vec<Picture> {
        match self {
            Decoder::H264(d) => d.flush().into_iter().map(Picture::H264).collect(),
            Decoder::Hevc(d) => d.flush().into_iter().map(Picture::Hevc).collect(),
            Decoder::ProRes => Vec::new(),
        }
    }

    /// Intra-only codecs decode the target sample alone.
    fn intra_only(&self) -> bool {
        matches!(self, Decoder::ProRes)
    }
}

impl Picture {
    fn pts(&self) -> i64 {
        match self {
            Picture::H264(p) => p.pts,
            Picture::Hevc(p) => p.pts,
            Picture::ProRes(_, pts) => *pts,
        }
    }

    fn size(&self) -> (u32, u32) {
        match self {
            Picture::H264(p) => (p.width, p.height),
            Picture::Hevc(p) => (p.width, p.height),
            Picture::ProRes(f, _) => (f.width, f.height),
        }
    }

    fn to_rgba(&self) -> Option<Rgba8Image> {
        match self {
            Picture::H264(p) => {
                let c = |d| Plane {
                    data: d,
                    stride: p.uv_stride,
                    width: p.chroma_width,
                    height: p.chroma_height,
                };
                let y = Plane {
                    data: &p.y[..],
                    stride: p.y_stride,
                    width: p.width,
                    height: p.height,
                };
                yuv::to_rgba(
                    y,
                    c(&p.u[..]),
                    c(&p.v[..]),
                    None,
                    8,
                    p.color.matrix,
                    p.color.full_range,
                )
            }
            Picture::Hevc(p) => {
                let (m, fr, bits) = (p.color.matrix, p.color.full_range, p.bit_depth);
                match (p.y.as_u8(), p.u.as_u8(), p.v.as_u8()) {
                    (Some(y), Some(u), Some(v)) => yuv::to_rgba(
                        Plane {
                            data: y,
                            stride: p.y_stride,
                            width: p.width,
                            height: p.height,
                        },
                        Plane {
                            data: u,
                            stride: p.uv_stride,
                            width: p.chroma_width,
                            height: p.chroma_height,
                        },
                        Plane {
                            data: v,
                            stride: p.uv_stride,
                            width: p.chroma_width,
                            height: p.chroma_height,
                        },
                        None,
                        bits,
                        m,
                        fr,
                    ),
                    _ => yuv::to_rgba(
                        Plane {
                            data: p.y.as_u16()?,
                            stride: p.y_stride,
                            width: p.width,
                            height: p.height,
                        },
                        Plane {
                            data: p.u.as_u16()?,
                            stride: p.uv_stride,
                            width: p.chroma_width,
                            height: p.chroma_height,
                        },
                        Plane {
                            data: p.v.as_u16()?,
                            stride: p.uv_stride,
                            width: p.chroma_width,
                            height: p.chroma_height,
                        },
                        None,
                        bits,
                        m,
                        fr,
                    ),
                }
            }
            Picture::ProRes(f, _) => {
                let cw = f.chroma.chroma_width(f.width);
                let full = Plane {
                    data: &f.y[..],
                    stride: f.width as usize,
                    width: f.width,
                    height: f.height,
                };
                let c = |d| Plane {
                    data: d,
                    stride: cw as usize,
                    width: cw,
                    height: f.height,
                };
                let alpha = f.alpha.as_deref().map(|a| Plane {
                    data: a,
                    stride: f.width as usize,
                    width: f.width,
                    height: f.height,
                });
                // ProRes is video range.
                yuv::to_rgba(
                    full,
                    c(&f.cb[..]),
                    c(&f.cr[..]),
                    alpha,
                    u32::from(f.bit_depth),
                    f.color.matrix,
                    false,
                )
            }
        }
    }
}

/// Decode samples `start..=target` (decode order) and return the picture whose presentation
/// time is nearest `target_pts`.
fn decode_range(
    dec: &mut Decoder,
    start: usize,
    target: usize,
    target_pts: i64,
    mut read: impl FnMut(usize) -> Result<(Vec<u8>, i64)>,
) -> Result<Picture> {
    let mut best: Option<Picture> = None;
    let keep = |p: Picture, best: &mut Option<Picture>| {
        let better = best
            .as_ref()
            .is_none_or(|b| (p.pts() - target_pts).abs() < (b.pts() - target_pts).abs());
        if better {
            *best = Some(p);
        }
    };
    let first = if dec.intra_only() { target } else { start };
    for i in first..=target {
        let (data, pts) = read(i)?;
        for p in dec.decode(&data, pts)? {
            keep(p, &mut best);
        }
    }
    for p in dec.flush() {
        keep(p, &mut best);
    }
    best.ok_or(Error::NoFrame)
}

/// Fit, then turn upright by `quarter_turns` clockwise.
fn finish(
    pic: &Picture,
    crop: Option<Rect>,
    quarter_turns: u8,
    opts: &FrameOptions,
    seconds: f64,
    codec: &'static str,
) -> Result<PosterFrame> {
    let rgba = pic.to_rgba().ok_or(Error::Decode(
        "decoded picture has inconsistent planes".into(),
    ))?;
    let (source_width, source_height) = pic.size();
    let rgba = match crop {
        Some(r) => crop_to(&rgba, r).unwrap_or(rgba),
        None => rgba,
    };
    let small = resize_to_fit(
        &rgba,
        opts.max_edge.max(1),
        opts.max_edge.max(1),
        &opts.resize,
    )
    .map_err(|e| Error::Decode(e.to_string()))?;
    let turn = match quarter_turns {
        1 => Orientation::Rotate90,
        2 => Orientation::Rotate180,
        3 => Orientation::Rotate270,
        _ => Orientation::Identity,
    };
    let image = turn
        .apply(&small)
        .map_err(|e| Error::Decode(e.to_string()))?;
    Ok(PosterFrame {
        image,
        seconds,
        codec,
        source_width,
        source_height,
    })
}

/// A crop rectangle in coded pixels: x, y, width, height.
type Rect = (u32, u32, u32, u32);

/// The `clap` clean aperture of an MP4/MOV track (ISO/IEC 14496-12 §12.1.4): a rectangle of
/// the given size centred on the picture centre plus the offsets. iPhone Live Photo clips use
/// it to hide stabilisation margins; players crop to it.
fn clean_aperture(c: &craft_isobmff::CleanAperture, w: u32, h: u32) -> Option<Rect> {
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
fn matroska_crop((top, bottom, left, right): (u32, u32, u32, u32), w: u32, h: u32) -> Option<Rect> {
    let cw = w.checked_sub(left)?.checked_sub(right)?;
    let ch = h.checked_sub(top)?.checked_sub(bottom)?;
    (cw > 0 && ch > 0 && (cw, ch) != (w, h)).then_some((left, top, cw, ch))
}

fn crop_to(img: &Rgba8Image, (x, y, w, h): Rect) -> Option<Rgba8Image> {
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

fn mp4(path: &Path, opts: &FrameOptions) -> Result<PosterFrame> {
    use craft_isobmff::{CodecConfig, TrackKind};
    let file = File::open(path)?;
    let m = craft_isobmff::open(&file).map_err(|e| Error::Demux(e.to_string()))?;
    let ti = m.track_of_kind(TrackKind::Video).ok_or(Error::NoVideo)?;
    let t = m.tracks.get(ti).ok_or(Error::NoVideo)?;
    let mut dec = match t.codec() {
        Some(CodecConfig::Avc(c)) => Decoder::H264(Box::new(
            craft_h264::Decoder::from_avcc(&c.to_bytes())
                .map_err(|e| Error::Decode(e.to_string()))?,
        )),
        Some(CodecConfig::Hevc(c)) => Decoder::Hevc(Box::new(
            craft_hevc::Decoder::from_hvcc(&c.to_bytes())
                .map_err(|e| Error::Decode(e.to_string()))?,
        )),
        Some(CodecConfig::ProRes { .. }) => Decoder::ProRes,
        Some(other) => return Err(Error::UnsupportedCodec(other.name().into())),
        None => return Err(Error::UnsupportedCodec("unknown".into())),
    };
    let last = t.samples.len().checked_sub(1).ok_or(Error::NoFrame)?;
    let scale = f64::from(t.timescale.max(1));
    let want = (opts.at_seconds.max(0.0) * scale) as i64;
    let mut target = t.sample_at_presentation_time(want).unwrap_or(0).min(last);
    let start = t.sync_sample_before(target);
    if target - start > opts.max_decode_frames {
        target = start;
    }
    let target_pts = t.samples.get(target).map_or(0, |s| s.pts);
    let codec = dec.name();
    let pic = decode_range(&mut dec, start, target, target_pts, |i| {
        let data = m
            .read_sample(&file, ti, i)
            .map_err(|e| Error::Demux(e.to_string()))?;
        Ok((data, t.samples.get(i).map_or(0, |s| s.pts)))
    })?;
    let seconds = (pic.pts() + t.edit_offset) as f64 / scale;
    let (pw, ph) = pic.size();
    let crop = t
        .video()
        .and_then(|v| v.clean_aperture.as_ref())
        .and_then(|c| clean_aperture(c, pw, ph));
    finish(
        &pic,
        crop,
        t.display_rotation().unwrap_or(0),
        opts,
        seconds.max(0.0),
        codec,
    )
}

fn mkv(path: &Path, opts: &FrameOptions) -> Result<PosterFrame> {
    use craft_matroska::{Codec, TrackKind};
    let file = File::open(path)?;
    let m = craft_matroska::open(&file).map_err(|e| Error::Demux(e.to_string()))?;
    let ti = m
        .tracks
        .iter()
        .position(|t| t.kind == TrackKind::Video)
        .ok_or(Error::NoVideo)?;
    let t = m.tracks.get(ti).ok_or(Error::NoVideo)?;
    let mut dec = match &t.codec {
        Codec::Avc { avcc } => Decoder::H264(Box::new(
            craft_h264::Decoder::from_avcc(avcc).map_err(|e| Error::Decode(e.to_string()))?,
        )),
        Codec::Hevc { hvcc } => Decoder::Hevc(Box::new(
            craft_hevc::Decoder::from_hvcc(hvcc).map_err(|e| Error::Decode(e.to_string()))?,
        )),
        other => return Err(Error::UnsupportedCodec(other.name())),
    };
    let (num, den) = (t.timebase.0.max(1) as f64, t.timebase.1.max(1) as f64);
    let want = (opts.at_seconds.max(0.0) * den / num) as i64;
    let last = t.samples.len().checked_sub(1).ok_or(Error::NoFrame)?;
    // Samples are in file (decode) order: the last one presented at or before the target.
    let mut target = t
        .samples
        .iter()
        .rposition(|s| s.pts <= want)
        .unwrap_or(0)
        .min(last);
    let start = t
        .samples
        .get(..=target)
        .and_then(|s| s.iter().rposition(|s| s.keyframe))
        .unwrap_or(0);
    if target - start > opts.max_decode_frames {
        target = start;
    }
    let target_pts = t.samples.get(target).map_or(0, |s| s.pts);
    let codec = dec.name();
    let pic = decode_range(&mut dec, start, target, target_pts, |i| {
        let data = m
            .read_sample(&file, ti, i)
            .map_err(|e| Error::Demux(e.to_string()))?;
        Ok((data, t.samples.get(i).map_or(0, |s| s.pts)))
    })?;
    let seconds = pic.pts() as f64 * num / den;
    let turns = t
        .video
        .as_ref()
        .and_then(|v| v.display_rotation())
        .unwrap_or(0);
    let (pw, ph) = pic.size();
    let crop = t.video.as_ref().and_then(|v| matroska_crop(v.crop, pw, ph));
    finish(&pic, crop, turns, opts, seconds.max(0.0), codec)
}

#[cfg(test)]
mod tests;
