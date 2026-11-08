//! PNG chunks (PNG 3rd edition §5, §11): IHDR, tRNS, acTL, iCCP, eXIf and the XMP iTXt.

use super::RawHeader;
use crate::bytes::be_u32;
use crate::container::{Embedded, exif_payload, inflate};
use crate::{Error, Format, Result};
use std::borrow::Cow;

const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

/// `(type, data)` for each chunk, in file order, stopping at IEND or the first damage.
pub(crate) fn chunks(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut p = 8usize;
    while let Some(len) = be_u32(b, p) {
        let Some(ty) = b.get(p + 4..p + 8) else { break };
        let start = p + 8;
        let Some(end) = start.checked_add(len as usize) else {
            break;
        };
        let Some(data) = b.get(start..end) else { break };
        let ty = [ty[0], ty[1], ty[2], ty[3]];
        out.push((ty, data));
        if &ty == b"IEND" || out.len() > 100_000 {
            break;
        }
        p = end + 4; // CRC
    }
    out
}

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let cs = chunks(b);
    let ihdr = cs
        .iter()
        .find(|(t, _)| t == b"IHDR")
        .map(|(_, d)| *d)
        .ok_or_else(|| Error::Malformed(Format::Png, "no IHDR chunk".into()))?;
    let (Some(width), Some(height), Some(&depth), Some(&color)) =
        (be_u32(ihdr, 0), be_u32(ihdr, 4), ihdr.get(8), ihdr.get(9))
    else {
        return Err(Error::Malformed(Format::Png, "IHDR is cut short".into()));
    };
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(Error::Malformed(
            Format::Png,
            format!("image size {width}×{height}"),
        ));
    }
    let trns = cs.iter().any(|(t, _)| t == b"tRNS");
    Ok(RawHeader {
        width,
        height,
        bit_depth: Some(depth),
        has_alpha: Some(matches!(color, 4 | 6) || trns),
        animated: cs.iter().any(|(t, _)| t == b"acTL"),
        orientation: None,
    })
}

pub(crate) fn embedded(b: &[u8]) -> Embedded<'_> {
    let mut e = Embedded::default();
    for (ty, d) in chunks(b) {
        match &ty {
            b"eXIf" if e.exif.is_none() => e.exif = exif_payload(d).map(Cow::Borrowed),
            b"iCCP" if e.icc.is_none() => {
                // name (1–79 bytes) NUL, compression method 0, zlib data
                if let Some(nul) = d.iter().position(|&c| c == 0)
                    && d.get(nul + 1) == Some(&0)
                {
                    e.icc = d.get(nul + 2..).and_then(inflate).map(Cow::Owned);
                }
            }
            b"iTXt" if e.xmp.is_none() => e.xmp = itxt_xmp(d),
            _ => {}
        }
    }
    e
}

/// The text of an `XML:com.adobe.xmp` iTXt chunk:
/// keyword NUL, compression flag, method, language NUL, translated keyword NUL, text.
fn itxt_xmp(d: &[u8]) -> Option<Cow<'_, [u8]>> {
    let rest = d.strip_prefix(XMP_KEYWORD)?.strip_prefix(&[0])?;
    let (&flag, rest) = rest.split_first()?;
    let (_method, rest) = rest.split_first()?;
    let lang_end = rest.iter().position(|&c| c == 0)?;
    let rest = rest.get(lang_end + 1..)?;
    let tk_end = rest.iter().position(|&c| c == 0)?;
    let text = rest.get(tk_end + 1..)?;
    if flag == 1 {
        inflate(text).map(Cow::Owned)
    } else {
        Some(Cow::Borrowed(text))
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Append a chunk with a correct CRC.
    pub fn chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(ty);
        out.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(ty);
        crc.update(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::chunk;
    use super::*;

    fn png_with(extra: &[(&[u8; 4], Vec<u8>)], color: u8) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = 300u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&200u32.to_be_bytes());
        ihdr.extend_from_slice(&[16, color, 0, 0, 0]);
        chunk(&mut b, b"IHDR", &ihdr);
        for (t, d) in extra {
            chunk(&mut b, t, d);
        }
        chunk(&mut b, b"IEND", &[]);
        b
    }

    #[test]
    fn header_reads_ihdr_alpha_and_animation() {
        let h = header(&png_with(&[], 2)).unwrap();
        assert_eq!(
            (h.width, h.height, h.bit_depth, h.has_alpha, h.animated),
            (300, 200, Some(16), Some(false), false)
        );
        let h = header(&png_with(
            &[(b"acTL", vec![0; 8]), (b"tRNS", vec![0; 6])],
            2,
        ))
        .unwrap();
        assert_eq!((h.has_alpha, h.animated), (Some(true), true));
        assert!(header(&b"\x89PNG\r\n\x1a\n"[..]).is_err());
    }

    #[test]
    fn finds_compressed_icc_exif_and_both_xmp_forms() {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut z, b"icc bytes").unwrap();
        let mut iccp = b"sRGB\0\0".to_vec();
        iccp.extend_from_slice(&z.finish().unwrap());
        let mut xmp = XMP_KEYWORD.to_vec();
        xmp.extend_from_slice(b"\0\0\0\0\0<x:xmpmeta/>");
        let file = png_with(
            &[
                (b"iCCP", iccp),
                (b"eXIf", b"MM\0*\0\0\0\x08".to_vec()),
                (b"iTXt", xmp),
            ],
            6,
        );
        let e = embedded(&file);
        assert_eq!(e.icc.as_deref(), Some(&b"icc bytes"[..]));
        assert_eq!(e.exif.as_deref(), Some(&b"MM\0*\0\0\0\x08"[..]));
        assert_eq!(e.xmp.as_deref(), Some(&b"<x:xmpmeta/>"[..]));

        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut z, b"<packed/>").unwrap();
        let mut xmp = XMP_KEYWORD.to_vec();
        xmp.extend_from_slice(b"\0\x01\0en\0\0");
        xmp.extend_from_slice(&z.finish().unwrap());
        assert_eq!(
            embedded(&png_with(&[(b"iTXt", xmp)], 6)).xmp.as_deref(),
            Some(&b"<packed/>"[..])
        );
    }
}
