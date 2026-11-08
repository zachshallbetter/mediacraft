//! Display-ready thumbnails: sRGB, upright, fitted within a square.

use crate::container::embedded;
use crate::exif::parse_exif;
use crate::icc::ColorConversion;
use crate::resize::{ResizeOptions, resize_to_fit};
use crate::{
    DecodeOptions, Format, Limits, Orientation, RawMode, Result, Rgba8Image, decode, sniff,
};
use serde::{Deserialize, Serialize};

/// Thumbnail request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbnailOptions {
    /// Fit within `max_edge × max_edge`.
    pub max_edge: u32,
    /// Accept the JPEG preview embedded in EXIF when its long edge is at least this. Defaults to
    /// `max_edge`, so it is only used when it needs no enlarging; lower it (e.g. 160) for an
    /// instant placeholder.
    pub min_embedded_edge: u32,
    /// Resource ceilings for the decode.
    pub limits: Limits,
    /// Resampling.
    pub resize: ResizeOptions,
}

impl ThumbnailOptions {
    /// Defaults for a `max_edge` thumbnail.
    pub fn new(max_edge: u32) -> Self {
        Self {
            max_edge,
            min_embedded_edge: max_edge,
            limits: Limits::default(),
            resize: ResizeOptions::default(),
        }
    }
}

/// Where the pixels came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThumbnailSource {
    /// The JPEG preview stored in EXIF IFD1.
    Embedded,
    /// A DCT-scaled JPEG decode.
    ScaledDecode,
    /// A full-resolution decode, then resampled.
    FullDecode,
}

/// A thumbnail.
#[derive(Clone, Debug)]
pub struct Thumbnail {
    /// 8-bit sRGB, upright, within `max_edge × max_edge`.
    pub image: Rgba8Image,
    /// Where the pixels came from.
    pub source: ThumbnailSource,
    /// Stored size of the original.
    pub source_width: u32,
    /// See [`Thumbnail::source_width`].
    pub source_height: u32,
    /// The orientation that was applied.
    pub orientation: Orientation,
    /// What colour conversion was applied.
    pub color: ColorConversion,
}

/// Make a thumbnail.
pub fn thumbnail(bytes: &[u8], opts: &ThumbnailOptions) -> Result<Thumbnail> {
    let max_edge = opts.max_edge.max(1);
    if sniff(bytes) == Some(Format::Jpeg)
        && let Some(t) = embedded_preview(bytes, max_edge, opts)
    {
        return Ok(t);
    }
    let d = decode(
        bytes,
        &DecodeOptions {
            limits: opts.limits,
            fit: Some((max_edge, max_edge)),
            raw: RawMode::Preview,
        },
    )?;
    let source = if (d.image.width(), d.image.height()) == (d.source_width, d.source_height) {
        ThumbnailSource::FullDecode
    } else {
        ThumbnailSource::ScaledDecode
    };
    let mut small = resize_to_fit(&d.image, max_edge, max_edge, &opts.resize)?;
    let color = d.convert_to_srgb(&mut small);
    Ok(Thumbnail {
        image: d.orientation.apply(&small)?,
        source,
        source_width: d.source_width,
        source_height: d.source_height,
        orientation: d.orientation,
        color,
    })
}

/// The EXIF IFD1 JPEG, when it is big enough. Shares the main image's orientation and profile.
fn embedded_preview(bytes: &[u8], max_edge: u32, opts: &ThumbnailOptions) -> Option<Thumbnail> {
    let blocks = embedded(bytes);
    let exif_blob = blocks.exif.as_deref()?;
    let exif = parse_exif(exif_blob)?;
    let (off, len) = exif.thumbnail?;
    let jpeg = exif_blob.get(off..off.checked_add(len)?)?;
    let main = crate::read_header(bytes).ok()?;
    let mut d = decode(
        jpeg,
        &DecodeOptions {
            limits: opts.limits,
            fit: None,
            raw: RawMode::Preview,
        },
    )
    .ok()?;
    if d.image.width().max(d.image.height()) < opts.min_embedded_edge.min(max_edge) {
        return None;
    }
    d.icc = blocks.icc.map(|c| c.into_owned());
    d.adobe_rgb_hint = d.icc.is_none() && exif.adobe_rgb_hint();
    let mut small = resize_to_fit(&d.image, max_edge, max_edge, &opts.resize).ok()?;
    let color = d.convert_to_srgb(&mut small);
    Some(Thumbnail {
        image: main.orientation.apply(&small).ok()?,
        source: ThumbnailSource::Embedded,
        source_width: main.width,
        source_height: main.height,
        orientation: main.orientation,
        color,
    })
}
