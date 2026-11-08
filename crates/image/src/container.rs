//! Embedded metadata blocks: where each format keeps EXIF, XMP, ICC and IPTC, extracted as raw
//! bytes. Parsing them is the job of [`crate::exif`], [`crate::xmp`], [`crate::iptc`] and
//! [`crate::icc`].

use crate::formats;
use crate::{Format, sniff};
use std::borrow::Cow;
use std::io::Read;

/// Largest metadata block we inflate or reassemble (ICC profiles, compressed XMP).
pub(crate) const MAX_BLOCK: usize = 16 << 20;

/// The raw metadata blocks a file carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Embedded<'a> {
    /// EXIF as a TIFF structure (starting `II*\0` or `MM\0*`, any `Exif\0\0` prefix removed).
    pub exif: Option<Cow<'a, [u8]>>,
    /// The XMP packet (UTF-8 XML).
    pub xmp: Option<Cow<'a, [u8]>>,
    /// The ICC profile, reassembled and decompressed.
    pub icc: Option<Cow<'a, [u8]>>,
    /// IPTC-IIM records.
    pub iptc: Option<Cow<'a, [u8]>>,
}

impl Embedded<'_> {
    /// Whether no block was found.
    pub fn is_empty(&self) -> bool {
        self.exif.is_none() && self.xmp.is_none() && self.icc.is_none() && self.iptc.is_none()
    }

    /// Copy the blocks out of the input.
    pub fn into_owned(self) -> Embedded<'static> {
        let own = |c: Option<Cow<'_, [u8]>>| c.map(|c| Cow::Owned(c.into_owned()));
        Embedded {
            exif: own(self.exif),
            xmp: own(self.xmp),
            icc: own(self.icc),
            iptc: own(self.iptc),
        }
    }
}

/// Find the metadata blocks in a file. Unknown formats and formats without metadata give an
/// empty result; damaged blocks are skipped, never an error.
pub fn embedded(bytes: &[u8]) -> Embedded<'_> {
    match sniff(bytes) {
        Some(Format::Jpeg) => formats::jpeg::embedded(bytes),
        Some(Format::Png) => formats::png::embedded(bytes),
        Some(Format::WebP) => formats::webp::embedded(bytes),
        Some(Format::Tiff | Format::Raw) => formats::tiff_file::embedded(bytes),
        Some(Format::Psd) => formats::psd::embedded(bytes),
        Some(f @ (Format::Heif | Format::Avif)) => crate::heif::Heif::parse(bytes, f)
            .map(|h| h.embedded())
            .unwrap_or_default(),
        _ => Embedded::default(),
    }
}

/// Strip an `Exif\0\0` (or `Exif\0\xFF`) prefix and require a TIFF header behind it.
pub(crate) fn exif_payload(data: &[u8]) -> Option<&[u8]> {
    let d = if data.starts_with(b"Exif\0") {
        data.get(6..)?
    } else {
        data
    };
    (d.starts_with(b"II*\0") || d.starts_with(b"MM\0*")).then_some(d)
}

/// Photoshop image resource blocks (`8BIM` + id + Pascal name + size + data, each padded to an
/// even length), as used by PSD files and JPEG APP13. Returns `(id, data)` pairs.
pub(crate) fn photoshop_resources(mut d: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    while d.len() >= 12 && out.len() < 1024 {
        let sig = &d[0..4];
        if !matches!(sig, b"8BIM" | b"MeSa" | b"AgHg" | b"PHUT" | b"DCSR") {
            break;
        }
        let id = u16::from_be_bytes([d[4], d[5]]);
        let name_len = usize::from(d[6]);
        // Pascal string: length byte + chars, padded so the whole is even.
        let name_total = (1 + name_len + 1) & !1;
        let Some(size_at) = 6usize.checked_add(name_total) else {
            break;
        };
        let Some(size) = crate::bytes::be_u32(d, size_at) else {
            break;
        };
        let start = size_at + 4;
        let Some(end) = start.checked_add(size as usize) else {
            break;
        };
        let Some(data) = d.get(start..end) else { break };
        out.push((id, data));
        let next = end + (end & 1);
        d = d.get(next..).unwrap_or(&[]);
    }
    out
}

/// zlib-inflate at most [`MAX_BLOCK`] bytes; `None` when the stream is damaged or too large.
pub(crate) fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut dec = flate2::read::ZlibDecoder::new(data).take(MAX_BLOCK as u64 + 1);
    dec.read_to_end(&mut out).ok()?;
    (out.len() <= MAX_BLOCK).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_payload_strips_prefix_and_checks_header() {
        assert_eq!(exif_payload(b"Exif\0\0II*\0rest"), Some(&b"II*\0rest"[..]));
        assert_eq!(exif_payload(b"MM\0*x"), Some(&b"MM\0*x"[..]));
        assert_eq!(exif_payload(b"Exif\0\0garbage"), None);
    }

    #[test]
    fn photoshop_resources_walks_padded_blocks_and_stops_on_damage() {
        let mut d = Vec::new();
        // id 0x0404, empty name (00 00), size 3, data, pad
        d.extend_from_slice(b"8BIM\x04\x04\0\0\0\0\0\x03abc\0");
        // id 0x040F, name "x" (01 'x'), size 2
        d.extend_from_slice(b"8BIM\x04\x0F\x01x\0\0\0\x02hi");
        // damaged: size runs past the end
        d.extend_from_slice(b"8BIM\x04\x22\0\0\0\0\x10\0");
        let r = photoshop_resources(&d);
        assert_eq!(r, vec![(0x0404, &b"abc"[..]), (0x040F, &b"hi"[..])]);
    }

    #[test]
    fn inflate_refuses_garbage() {
        assert_eq!(inflate(b"not zlib"), None);
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut e, b"profile").unwrap();
        assert_eq!(
            inflate(&e.finish().unwrap()).as_deref(),
            Some(&b"profile"[..])
        );
    }

    #[test]
    fn unknown_input_has_no_blocks() {
        assert!(embedded(b"hello world").is_empty());
    }
}
