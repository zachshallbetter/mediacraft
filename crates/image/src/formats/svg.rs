//! SVG: sniffing and header from the root element's attributes (always built), and rasterising
//! through `usvg` and `resvg` (the `svg` feature).
//!
//! Every document is sanitised first (see [`crate::sanitize_svg`]), the renderer never reads a
//! file or URL (only `data:` images in PNG, JPEG, GIF and WebP), and it has no fonts: the `text`
//! feature of `usvg` is not built, so `<text>` is skipped and system fonts are never touched.

use super::RawHeader;
use crate::{Error, Format, Result};
use std::borrow::Cow;
use std::io::Read;

/// What an SVG with no usable size at all measures, as in `usvg`.
const DEFAULT_SIZE: f64 = 100.0;

/// The prefix of the document as text bytes: gzip is inflated (as far as `limit` bytes of output
/// and as far as the input goes), anything else is returned as is.
fn text_prefix(b: &[u8], limit: u64) -> Cow<'_, [u8]> {
    if !b.starts_with(&[0x1F, 0x8B]) {
        return Cow::Borrowed(b);
    }
    let mut out = Vec::new();
    // A cut-off stream errors after yielding what it had; keep that.
    let _ = flate2::read::GzDecoder::new(b)
        .take(limit)
        .read_to_end(&mut out);
    Cow::Owned(out)
}

/// Offset of the `<` of the root `<svg` start tag, after an optional BOM, whitespace, XML
/// declaration, comments and DOCTYPE.
fn root_offset(b: &[u8]) -> Option<usize> {
    let mut i = if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    loop {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let rest = b.get(i..)?;
        if rest.starts_with(b"<?") {
            i += find(rest, b"?>")? + 2;
        } else if rest.starts_with(b"<!--") {
            i += find(rest.get(4..)?, b"-->")? + 4 + 3;
        } else if rest
            .get(..9)
            .is_some_and(|d| d.eq_ignore_ascii_case(b"<!DOCTYPE"))
        {
            i += doctype_len(rest)?;
        } else if rest.starts_with(b"<svg")
            && matches!(rest.get(4), Some(c) if c.is_ascii_whitespace() || *c == b'>' || *c == b'/')
        {
            return Some(i);
        } else {
            return None;
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Length of a `<!DOCTYPE ...>` declaration, through any `[ internal subset ]` and quoted
/// strings.
fn doctype_len(b: &[u8]) -> Option<usize> {
    let mut quote = None;
    let mut subset = false;
    for (i, &c) in b.iter().enumerate().skip(9) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(c),
            (None, b'[') => subset = true,
            (None, b']') => subset = false,
            (None, b'>') if !subset => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Whether `b` is an SVG document (or gzip of one): the root element is `<svg`.
pub(crate) fn sniff(b: &[u8]) -> bool {
    let text = text_prefix(b, 8 << 10);
    root_offset(&text).is_some()
}

/// The attributes of the root start tag that were fully read, as `(name, value)`.
fn root_attributes(tag: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 4;
    loop {
        while tag.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        match tag.get(i) {
            None | Some(b'>') | Some(b'/') => return out,
            _ => {}
        }
        let start = i;
        while tag
            .get(i)
            .is_some_and(|c| !c.is_ascii_whitespace() && !matches!(c, b'=' | b'>' | b'/'))
        {
            i += 1;
        }
        let name = String::from_utf8_lossy(&tag[start..i]).into_owned();
        while tag.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if tag.get(i) != Some(&b'=') {
            if i == start {
                return out;
            }
            continue;
        }
        i += 1;
        while tag.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let Some(&q) = tag.get(i).filter(|c| matches!(c, b'"' | b'\'')) else {
            return out;
        };
        i += 1;
        let vstart = i;
        while tag.get(i).is_some_and(|c| *c != q) {
            i += 1;
        }
        if tag.get(i).is_none() {
            return out; // cut off inside the value
        }
        out.push((name, String::from_utf8_lossy(&tag[vstart..i]).into_owned()));
        i += 1;
    }
}

/// A CSS length in px. `None` for percentages, unknown units and anything not positive.
fn length(s: &str) -> Option<f64> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(s.len());
    // `1em` starts with the digit and `e`; take the longest prefix that parses.
    let (num, unit) = (0..=end)
        .rev()
        .find_map(|k| s.get(..k)?.parse::<f64>().ok().map(|n| (n, &s[k..])))?;
    let k = match unit.trim() {
        "" | "px" => 1.0,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "in" => 96.0,
        "em" => 12.0,
        "ex" => 6.0,
        _ => return None,
    };
    let v = num * k;
    (v.is_finite() && v > 0.0).then_some(v)
}

fn view_box(s: &str) -> Option<(f64, f64)> {
    let mut it = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f64>().ok());
    let (_, _, w, h) = (it.next()??, it.next()??, it.next()??, it.next()??);
    (it.next().is_none() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h))
}

fn to_px(v: f64) -> u32 {
    // Float to integer casts saturate.
    (v.ceil() as u32).max(1)
}

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let text = text_prefix(b, 64 << 10);
    let start =
        root_offset(&text).ok_or_else(|| Error::Malformed(Format::Svg, "no <svg> root".into()))?;
    let attrs = root_attributes(&text[start..]);
    let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let (w, h) = (
        get("width").and_then(length),
        get("height").and_then(length),
    );
    let vb = get("viewBox").and_then(view_box);
    // The same rules `usvg` applies; a percentage or missing size falls back to the viewBox.
    let (w, h) = match (w, h, vb) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some((vw, vh))) => (w, w * vh / vw),
        (None, Some(h), Some((vw, vh))) => (h * vw / vh, h),
        (None, None, Some(vb)) => vb,
        (w, h, None) => (w.unwrap_or(DEFAULT_SIZE), h.unwrap_or(DEFAULT_SIZE)),
    };
    Ok(RawHeader {
        width: to_px(w),
        height: to_px(h),
        bit_depth: None,
        has_alpha: Some(true),
        animated: false,
        orientation: None,
    })
}

#[cfg(feature = "svg")]
pub(crate) use render::decode;

/// Without the `svg` feature a document is sniffed and measured but not drawn.
#[cfg(not(feature = "svg"))]
pub(crate) fn decode(
    _: &[u8],
    _: &crate::Limits,
    _: Option<(u32, u32)>,
) -> Result<crate::Rgba8Image> {
    Err(Format::Svg.unsupported())
}

#[cfg(feature = "svg")]
mod render {
    use crate::{Error, Format, Limits, Result, Rgba8Image, sanitize_svg};
    use resvg::tiny_skia;

    fn fail(why: impl ToString) -> Error {
        Error::Decode(Format::Svg, why.to_string())
    }

    /// Resolve images from `data:` URIs only, and only raster kinds. Every other `href` (a path,
    /// a URL) resolves to nothing, so the renderer never opens a file or a socket.
    fn options() -> usvg::Options<'static> {
        let default_data = usvg::ImageHrefResolver::default_data_resolver();
        usvg::Options {
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(move |mime, data, opts| {
                    match default_data(mime, data, opts) {
                        Some(usvg::ImageKind::SVG(_)) => None,
                        other => other,
                    }
                }),
                resolve_string: Box::new(|_, _| None),
            },
            ..usvg::Options::default()
        }
    }

    /// Rasterise. With `fit`, the largest size within it that keeps the aspect ratio (a vector
    /// scales up as well as down); otherwise the intrinsic size. Either way the pixel count and
    /// buffer are checked against `limits` before anything is allocated.
    pub(crate) fn decode(
        bytes: &[u8],
        limits: &Limits,
        fit: Option<(u32, u32)>,
    ) -> Result<Rgba8Image> {
        let clean = sanitize_svg(bytes)?;
        let tree = usvg::Tree::from_data(&clean, &options()).map_err(fail)?;
        let (tw, th) = (
            f64::from(tree.size().width()),
            f64::from(tree.size().height()),
        );
        if !(tw.is_finite() && th.is_finite() && tw > 0.0 && th > 0.0) {
            return Err(fail("the document has no size"));
        }
        let (w, h) = match fit {
            Some((fw, fh)) if fw > 0 && fh > 0 => {
                let s = (f64::from(fw) / tw).min(f64::from(fh) / th);
                (
                    ((tw * s).round() as u32).max(1),
                    ((th * s).round() as u32).max(1),
                )
            }
            _ => ((tw.ceil() as u32).max(1), (th.ceil() as u32).max(1)),
        };
        limits.check_size(w, h)?;
        if u64::from(w) * u64::from(h) * 4 > limits.max_alloc {
            return Err(Error::TooLarge {
                width: w,
                height: h,
                max_pixels: limits.max_alloc / 4,
            });
        }
        let mut pixmap =
            tiny_skia::Pixmap::new(w, h).ok_or_else(|| fail("cannot allocate the canvas"))?;
        let scale = tiny_skia::Transform::from_scale(
            (f64::from(w) / tw) as f32,
            (f64::from(h) / th) as f32,
        );
        resvg::render(&tree, scale, &mut pixmap.as_mut());
        let mut data = Vec::with_capacity(pixmap.pixels().len() * 4);
        for p in pixmap.pixels() {
            let c = p.demultiply();
            data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        Rgba8Image::new(w, h, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_through_prolog() {
        assert!(sniff(b"<svg xmlns='http://www.w3.org/2000/svg'/>"));
        assert!(sniff(
            b"\xEF\xBB\xBF \n<?xml version='1.0'?>\n<!-- hi -->\n<svg>"
        ));
        assert!(sniff(
            b"<?xml version=\"1.0\"?><!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [ <!ENTITY a \"b>\"> ]><svg>"
        ));
        assert!(!sniff(b"<html><svg></svg></html>"));
        assert!(!sniff(b"<svgx/>"));
        assert!(!sniff(b"<?xml version='1.0'?><feed/>"));
    }

    #[test]
    fn header_sizes() {
        let h = |s: &str| {
            let r = header(s.as_bytes()).unwrap();
            (r.width, r.height)
        };
        assert_eq!(h("<svg width='10' height='20'/>"), (10, 20));
        assert_eq!(
            h("<svg width='1in' height='72pt' viewBox='0 0 1 1'/>"),
            (96, 96)
        );
        assert_eq!(
            h("<svg width='100%' height='100%' viewBox='0 0 30 40'/>"),
            (30, 40)
        );
        assert_eq!(h("<svg width='60' viewBox='0 0 30 40'/>"), (60, 80));
        assert_eq!(h("<svg/>"), (100, 100));
    }
}
