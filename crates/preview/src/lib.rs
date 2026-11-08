//! # mediacraft-preview
//!
//! Poster frames and waveforms from MediaCraft's own demuxers and decoders, no ffmpeg.
//!
//! Containers: MP4/MOV (`craft-isobmff`), MKV/WebM (`craft-matroska`), MPEG transport and
//! program streams (`craft-mpegts`: `.ts`, `.m2ts`, `.mts`, `.mpg`, `.vob`) and MXF
//! (`craft-mxf`). Video codecs: H.264, HEVC, VP8, VP9, AV1, MPEG-1/2, ProRes, DNxHD/HR and
//! Motion JPEG.
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

mod codec;
mod source;
mod waveform;

pub use waveform::{Waveform, WaveformOptions, waveform};

use codec::{Decoder, Picture};
use craft_image::{Orientation, ResizeOptions, Rgba8Image, resize_to_fit};
#[cfg(test)]
use source::{clean_aperture, crop_to, matroska_crop};
use std::path::Path;

/// Why a preview could not be made.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading the file failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Not a container this crate reads.
    #[error("not an MP4, MOV, MKV, WebM, MPEG-TS/PS or MXF file")]
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
    /// Codec name, as ffprobe names it (`h264`, `hevc`, `vp9`, `av1`, `prores`, …).
    pub codec: &'static str,
    /// Coded size before rotation and fitting.
    pub source_width: u32,
    /// See [`PosterFrame::source_width`].
    pub source_height: u32,
}

/// Decode the frame at `opts.at_seconds`.
pub fn poster_frame(path: &Path, opts: &FrameOptions) -> Result<PosterFrame> {
    let mut src = source::open(path)?;
    let last = src.samples.len().checked_sub(1).ok_or(Error::NoFrame)?;
    let want = (opts.at_seconds.max(0.0) * src.ticks_per_second) as i64;
    // The sample presented latest at or before the target (B-frames: not the last in decode
    // order), and the sync sample at or before it in decode order.
    let mut target = src
        .samples
        .iter()
        .enumerate()
        .filter(|(_, (pts, _))| *pts <= want)
        .max_by_key(|(_, (pts, _))| *pts)
        .map(|(i, _)| i)
        .or_else(|| {
            src.samples
                .iter()
                .enumerate()
                .min_by_key(|(_, (pts, _))| *pts)
                .map(|(i, _)| i)
        })
        .unwrap_or(0)
        .min(last);
    let start = src
        .samples
        .get(..=target)
        .and_then(|s| s.iter().rposition(|(_, key)| *key))
        .unwrap_or(0);
    if target - start > opts.max_decode_frames {
        target = start;
    }
    let read = &src.read;
    let (pic, seconds) = if src.decode_order_times {
        // Pictures come out in presentation order from the sync sample on: the k-th is shown
        // at the sync sample's position plus k. Decode until that one appears.
        let start_pts = src.samples.get(start).map_or(0, |s| s.0);
        let needed = usize::try_from(want.saturating_sub(start_pts).max(0))
            .unwrap_or(0)
            .min(opts.max_decode_frames);
        let pic = nth_output(&mut src.decoder, start, src.samples.len(), needed, |i| {
            read(i)
        })?;
        let shown = start_pts + i64::try_from(needed).unwrap_or(0);
        (pic, shown as f64 / src.ticks_per_second)
    } else {
        let target_pts = src.samples.get(target).map_or(0, |s| s.0);
        let samples = std::mem::take(&mut src.samples);
        let pic = decode_range(&mut src.decoder, start, target, target_pts, |i| {
            Ok((read(i)?, samples.get(i).map_or(0, |s| s.0)))
        })?;
        let seconds = pic.pts() as f64 / src.ticks_per_second;
        (pic, seconds)
    };
    let (pw, ph) = pic.size();
    let crop = src.crop.rect(pw, ph);
    finish(
        &pic,
        crop,
        src.quarter_turns,
        opts,
        seconds.max(0.0),
        src.decoder.name(),
    )
}

/// Most pictures H.264 and HEVC may hold back for reordering.
const REORDER_DEPTH: usize = 16;

/// Decode from `start` and return the `n`-th picture in presentation order, or the last one if
/// the samples run out first. Pictures pass through a reorder buffer keyed by picture order
/// count where the codec has one, so early or out-of-order output from the decoder cannot
/// shift the count; other codecs keep the decoder's output order.
fn nth_output(
    dec: &mut Decoder,
    start: usize,
    len: usize,
    n: usize,
    read: impl Fn(usize) -> Result<Vec<u8>>,
) -> Result<Picture> {
    let mut buffer: Vec<Picture> = Vec::new();
    let mut shown = 0usize;
    let mut last = None;
    // Release the lowest-ordered picture; true when it is the one wanted.
    let mut release = |buffer: &mut Vec<Picture>, last: &mut Option<Picture>| -> bool {
        let at = buffer
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.order().unwrap_or(i64::MAX))
            .map_or(0, |(i, _)| i);
        if buffer.is_empty() {
            return false;
        }
        let p = buffer.remove(at);
        let hit = shown == n;
        shown += 1;
        *last = Some(p);
        hit
    };
    let end = len.min(start.saturating_add(n).saturating_add(REORDER_DEPTH * 2));
    for i in start..end {
        for p in dec.decode(&read(i)?, i64::try_from(i).unwrap_or(0))? {
            let ordered = p.order().is_some();
            buffer.push(p);
            while buffer.len() > if ordered { REORDER_DEPTH } else { 0 } {
                if release(&mut buffer, &mut last) {
                    return last.ok_or(Error::NoFrame);
                }
            }
        }
    }
    buffer.extend(dec.flush());
    while !buffer.is_empty() {
        if release(&mut buffer, &mut last) {
            break;
        }
    }
    last.ok_or(Error::NoFrame)
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
        if best
            .as_ref()
            .is_none_or(|b| (p.pts() - target_pts).abs() < (b.pts() - target_pts).abs())
        {
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

/// Crop, fit, then turn upright by `quarter_turns` clockwise.
fn finish(
    pic: &Picture,
    crop: Option<source::Rect>,
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
        Some(r) => source::crop_to(&rgba, r).unwrap_or(rgba),
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

#[cfg(test)]
mod tests;
