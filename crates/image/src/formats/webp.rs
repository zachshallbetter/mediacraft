//! WebP RIFF chunks (RFC 9649): VP8, VP8L and VP8X headers, ICCP, EXIF and XMP.

use super::RawHeader;
use crate::bytes::{le_u16, le_u24, le_u32};
use crate::container::{Embedded, exif_payload};
use crate::{Error, Format, Result};
use std::borrow::Cow;

/// `(fourcc, data)` for each chunk after the RIFF/WEBP header.
pub(crate) fn chunks(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut p = 12usize;
    while let (Some(id), Some(size)) = (b.get(p..p + 4), le_u32(b, p + 4)) {
        let start = p + 8;
        let Some(end) = start.checked_add(size as usize) else {
            break;
        };
        let Some(data) = b.get(start..end) else { break };
        out.push(([id[0], id[1], id[2], id[3]], data));
        if out.len() > 100_000 {
            break;
        }
        p = end + (end & 1);
    }
    out
}

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let cs = chunks(b);
    let bad = |why: &str| Error::Malformed(Format::WebP, why.into());
    let first = cs.first().ok_or_else(|| bad("no chunks"))?;
    let (width, height, has_alpha, animated) = match &first.0 {
        b"VP8X" => {
            let d = first.1;
            let flags = *d.first().ok_or_else(|| bad("VP8X is cut short"))?;
            let w = le_u24(d, 4).ok_or_else(|| bad("VP8X is cut short"))? + 1;
            let h = le_u24(d, 7).ok_or_else(|| bad("VP8X is cut short"))? + 1;
            (w, h, flags & 0x10 != 0, flags & 0x02 != 0)
        }
        b"VP8L" => {
            let d = first.1;
            if d.first() != Some(&0x2F) {
                return Err(bad("VP8L signature missing"));
            }
            let bits = le_u32(d, 1).ok_or_else(|| bad("VP8L is cut short"))?;
            (
                (bits & 0x3FFF) + 1,
                ((bits >> 14) & 0x3FFF) + 1,
                (bits >> 28) & 1 == 1,
                false,
            )
        }
        b"VP8 " => {
            let d = first.1;
            if d.get(3..6) != Some(&[0x9D, 0x01, 0x2A]) {
                return Err(bad("VP8 start code missing"));
            }
            let w = le_u16(d, 6).ok_or_else(|| bad("VP8 is cut short"))? & 0x3FFF;
            let h = le_u16(d, 8).ok_or_else(|| bad("VP8 is cut short"))? & 0x3FFF;
            (u32::from(w), u32::from(h), false, false)
        }
        _ => return Err(bad("first chunk is not VP8, VP8L or VP8X")),
    };
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    Ok(RawHeader {
        width,
        height,
        bit_depth: Some(8),
        has_alpha: Some(has_alpha),
        animated,
        orientation: None,
    })
}

pub(crate) fn embedded(b: &[u8]) -> Embedded<'_> {
    let mut e = Embedded::default();
    for (id, d) in chunks(b) {
        match &id {
            b"ICCP" if e.icc.is_none() => e.icc = Some(Cow::Borrowed(d)),
            b"EXIF" if e.exif.is_none() => e.exif = exif_payload(d).map(Cow::Borrowed),
            b"XMP " if e.xmp.is_none() => e.xmp = Some(Cow::Borrowed(d)),
            _ => {}
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riff(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = b"WEBP".to_vec();
        for (id, d) in chunks {
            body.extend_from_slice(*id);
            body.extend_from_slice(&(d.len() as u32).to_le_bytes());
            body.extend_from_slice(d);
            if d.len() % 2 == 1 {
                body.push(0);
            }
        }
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&(body.len() as u32).to_le_bytes());
        b.extend_from_slice(&body);
        b
    }

    #[test]
    fn vp8x_header_flags_and_metadata() {
        let mut x = vec![0x10 | 0x02 | 0x20, 0, 0, 0];
        x.extend_from_slice(&[0xFF, 0x03, 0x00]); // 1024 - 1
        x.extend_from_slice(&[0xFF, 0x02, 0x00]); // 768 - 1
        let b = riff(&[
            (b"VP8X", x),
            (b"ICCP", b"prof".to_vec()),
            (b"EXIF", b"Exif\0\0II*\0".to_vec()),
            (b"XMP ", b"<x/>".to_vec()),
        ]);
        let h = header(&b).unwrap();
        assert_eq!(
            (h.width, h.height, h.has_alpha, h.animated),
            (1024, 768, Some(true), true)
        );
        let e = embedded(&b);
        assert_eq!(e.icc.as_deref(), Some(&b"prof"[..]));
        assert_eq!(e.exif.as_deref(), Some(&b"II*\0"[..]));
        assert_eq!(e.xmp.as_deref(), Some(&b"<x/>"[..]));
    }

    #[test]
    fn simple_lossy_and_lossless_headers() {
        let mut vp8 = vec![0, 0, 0, 0x9D, 0x01, 0x2A];
        vp8.extend_from_slice(&640u16.to_le_bytes());
        vp8.extend_from_slice(&480u16.to_le_bytes());
        let h = header(&riff(&[(b"VP8 ", vp8)])).unwrap();
        assert_eq!((h.width, h.height), (640, 480));

        let bits: u32 = 99 | (49 << 14) | (1 << 28);
        let mut vp8l = vec![0x2F];
        vp8l.extend_from_slice(&bits.to_le_bytes());
        let h = header(&riff(&[(b"VP8L", vp8l)])).unwrap();
        assert_eq!((h.width, h.height, h.has_alpha), (100, 50, Some(true)));

        assert!(header(&riff(&[(b"JUNK", vec![0; 4])])).is_err());
    }
}
