//! JPEG XL stills (ISO/IEC 18181) through `jxl-oxide`, a pure-Rust decoder. Bare codestreams and
//! the ISO BMFF-style container are both read. Native builds only: the browser build leaves the
//! decoder out to keep its wasm under the size cap, and reports JPEG XL as undecodable there.
//!
//! Orientation is applied by the decoder, so the header reports upright dimensions and
//! [`Orientation::Identity`](crate::Orientation). The first frame is decoded, with its alpha.

use super::RawHeader;
use crate::decode::guarded;
use crate::{Error, Format, Limits, Result, Rgba8Image};
use jxl_oxide::{AllocTracker, InitializeResult, JxlImage};
use std::borrow::Cow;

/// The container's signature box.
const CONTAINER_SIGNATURE: &[u8] = &[
    0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
];

/// Memory the header parse may use (ICC profile, preview frame header).
const HEADER_BUDGET: usize = 64 << 20;

fn malformed(why: impl Into<String>) -> Error {
    Error::Malformed(Format::Jxl, why.into())
}

/// Size, depth and alpha from the image header, without decoding a frame.
pub(crate) fn header(bytes: &[u8]) -> Result<RawHeader> {
    let src = strip_compressed_boxes(bytes)?;
    guarded(Format::Jxl, || {
        let mut uninit = JxlImage::builder()
            .alloc_tracker(AllocTracker::with_limit(HEADER_BUDGET))
            .build_uninit();
        uninit
            .feed_bytes(&src)
            .map_err(|e| malformed(e.to_string()))?;
        let image = match uninit.try_init().map_err(|e| malformed(e.to_string()))? {
            InitializeResult::Initialized(image) => image,
            InitializeResult::NeedMoreData(_) => return Err(malformed("header is cut short")),
        };
        let meta = &image.image_header().metadata;
        let bits = meta.bit_depth.bits_per_sample();
        Ok(RawHeader {
            width: image.width(),
            height: image.height(),
            bit_depth: Some(u8::try_from(bits.min(255)).unwrap_or(u8::MAX)),
            has_alpha: Some(meta.alpha().is_some()),
            animated: meta.animation.is_some(),
            orientation: None,
        })
    })
}

/// Decode the first frame to RGBA8, and return the ICC profile that describes its colours: the
/// embedded one when the file has it, else one generated from the colour encoding.
pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<(Rgba8Image, Option<Vec<u8>>)> {
    let src = strip_compressed_boxes(bytes)?;
    guarded(Format::Jxl, || {
        let head = header(&src)?;
        limits.check_size(head.width, head.height)?;
        let px = u64::from(head.width) * u64::from(head.height);
        // The RGBA output, and a few planes of f32 working storage inside the decoder.
        if px.saturating_mul(4) > limits.max_alloc {
            return Err(Error::TooLarge {
                width: head.width,
                height: head.height,
                max_pixels: limits.max_alloc / 4,
            });
        }
        let budget =
            usize::try_from(px.saturating_mul(64).min(limits.max_alloc)).unwrap_or(usize::MAX);
        let decode_err = |e: &dyn std::fmt::Display| Error::Decode(Format::Jxl, e.to_string());
        let image = JxlImage::builder()
            .alloc_tracker(AllocTracker::with_limit(budget))
            .read(&*src)
            .map_err(|e| decode_err(&e))?;
        // Spline rendering allocates outside the tracker, with no bound on the control points.
        if (0..image.num_loaded_frames())
            .filter_map(|i| image.frame(i))
            .any(|f| f.header().flags.splines())
        {
            return Err(Error::Decode(
                Format::Jxl,
                "splines are not supported".into(),
            ));
        }
        if image.pixel_format().has_black() {
            return Err(Error::Decode(Format::Jxl, "CMYK is not supported".into()));
        }
        let associated = image
            .image_header()
            .metadata
            .ec_info
            .iter()
            .find_map(|e| e.alpha_associated())
            .unwrap_or(false);
        let icc = image
            .original_icc()
            .map(<[u8]>::to_vec)
            .unwrap_or_else(|| image.rendered_icc());
        let frame = image.render_frame(0).map_err(|e| decode_err(&e))?;
        let mut stream = frame.stream();
        let (w, h, ch) = (stream.width(), stream.height(), stream.channels() as usize);
        if (w, h) != (head.width, head.height) {
            return Err(Error::Decode(
                Format::Jxl,
                "frame size differs from the header".into(),
            ));
        }
        if !(1..=4).contains(&ch) {
            return Err(Error::Decode(Format::Jxl, format!("{ch} colour channels")));
        }
        let n = px as usize * ch;
        let mut samples = vec![0u8; n];
        if stream.write_to_buffer(&mut samples) != n {
            return Err(Error::Decode(Format::Jxl, "pixel data is truncated".into()));
        }
        let mut out = Vec::with_capacity(px as usize * 4);
        for p in samples.chunks_exact(ch) {
            let (rgb, a) = match *p {
                [g] => ([g, g, g], 255),
                [g, a] => ([g, g, g], a),
                [r, g, b] => ([r, g, b], 255),
                [r, g, b, a] => ([r, g, b], a),
                _ => ([0; 3], 255),
            };
            let rgb = if associated {
                unpremultiply(rgb, a)
            } else {
                rgb
            };
            out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
        }
        Ok((Rgba8Image::new(w, h, out)?, Some(icc)))
    })
}

fn unpremultiply(rgb: [u8; 3], a: u8) -> [u8; 3] {
    if a == 255 {
        return rgb;
    }
    if a == 0 {
        return [0; 3];
    }
    rgb.map(|c| {
        u8::try_from((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).unwrap_or(u8::MAX)
    })
}

/// The input without Brotli-compressed (`brob`) boxes. `jxl-oxide` inflates those into memory
/// its allocation tracker does not see, and we read no metadata from them.
fn strip_compressed_boxes(src: &[u8]) -> Result<Cow<'_, [u8]>> {
    if !src.starts_with(CONTAINER_SIGNATURE) {
        return Ok(Cow::Borrowed(src));
    }
    let mut out = Vec::new();
    let mut stripped = false;
    let mut at = 0usize;
    while at < src.len() {
        let head = src
            .get(at..at.saturating_add(8))
            .ok_or_else(|| malformed("box header is cut short"))?;
        let ty = &head[4..];
        let size = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
            0 => (src.len() - at) as u64,
            1 => src
                .get(at + 8..at.saturating_add(16))
                .and_then(|b| <[u8; 8]>::try_from(b).ok())
                .map(u64::from_be_bytes)
                .ok_or_else(|| malformed("box header is cut short"))?,
            n => u64::from(n),
        };
        // A box may run past a truncated file; the decoder reports what is missing.
        let end = usize::try_from(size)
            .ok()
            .filter(|&s| s >= 8)
            .and_then(|s| at.checked_add(s))
            .ok_or_else(|| malformed("box size is invalid"))?
            .min(src.len());
        if ty == b"brob" {
            stripped = true;
        } else {
            out.extend_from_slice(src.get(at..end).unwrap_or_default());
        }
        at = end;
    }
    Ok(if stripped {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(src)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpremultiply_restores_colour() {
        assert_eq!(unpremultiply([64, 0, 0], 128), [128, 0, 0]);
        assert_eq!(unpremultiply([9, 9, 9], 0), [0, 0, 0]);
        assert_eq!(unpremultiply([9, 9, 9], 255), [9, 9, 9]);
    }

    #[test]
    fn brob_boxes_are_removed() {
        let mut b = CONTAINER_SIGNATURE.to_vec();
        b.extend_from_slice(&[0, 0, 0, 12, b'b', b'r', b'o', b'b', 1, 2, 3, 4]);
        b.extend_from_slice(&[0, 0, 0, 10, b'j', b'x', b'l', b'c', 0xFF, 0x0A]);
        let out = strip_compressed_boxes(&b).unwrap();
        assert_eq!(out.len(), 12 + 10);
        assert!(!out.windows(4).any(|w| w == b"brob"));
        assert!(strip_compressed_boxes(&b[..14]).is_err());
    }
}
