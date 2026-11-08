//! Format detection from leading bytes.

use crate::tiff::Tiff;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Image container formats this crate recognises.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// JPEG (JFIF / EXIF).
    Jpeg,
    /// PNG, including APNG.
    Png,
    /// GIF 87a / 89a.
    Gif,
    /// WebP (lossy, lossless, extended).
    WebP,
    /// Ordinary TIFF, including BigTIFF.
    Tiff,
    /// Windows bitmap.
    Bmp,
    /// Photoshop PSD / PSB.
    Psd,
    /// HEIC / HEIF (ISO BMFF with HEVC).
    Heif,
    /// AVIF (ISO BMFF with AV1).
    Avif,
    /// JPEG XL, bare codestream or container.
    Jxl,
    /// Camera raw: TIFF-structured (DNG, CR2, NEF, ARW, …) or other containers (CR3, RAF, …).
    Raw,
}

impl Format {
    /// Canonical lowercase file extension.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Gif => "gif",
            Format::WebP => "webp",
            Format::Tiff => "tif",
            Format::Bmp => "bmp",
            Format::Psd => "psd",
            Format::Heif => "heic",
            Format::Avif => "avif",
            Format::Jxl => "jxl",
            Format::Raw => "raw",
        }
    }

    /// IANA media type.
    pub fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Png => "image/png",
            Format::Gif => "image/gif",
            Format::WebP => "image/webp",
            Format::Tiff => "image/tiff",
            Format::Bmp => "image/bmp",
            Format::Psd => "image/vnd.adobe.photoshop",
            Format::Heif => "image/heic",
            Format::Avif => "image/avif",
            Format::Jxl => "image/jxl",
            Format::Raw => "image/x-raw",
        }
    }

    /// Why this build cannot decode the format's pixels, `None` when it can. Headers and
    /// metadata of every format are still read.
    pub fn not_decodable(self) -> Option<&'static str> {
        match self {
            Format::Jxl => Some("JPEG XL decoding is not in this build"),
            _ => None,
        }
    }

    /// Whether [`crate::decode`] can decode this format.
    pub fn can_decode(self) -> bool {
        self.not_decodable().is_none()
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Format::Jpeg => "JPEG",
            Format::Png => "PNG",
            Format::Gif => "GIF",
            Format::WebP => "WebP",
            Format::Tiff => "TIFF",
            Format::Bmp => "BMP",
            Format::Psd => "PSD",
            Format::Heif => "HEIF",
            Format::Avif => "AVIF",
            Format::Jxl => "JPEG XL",
            Format::Raw => "camera raw",
        };
        f.write_str(name)
    }
}

/// Detect the format from a file's first bytes. A few KiB suffice; telling a camera raw from an
/// ordinary TIFF walks its IFDs, so give it more of the file when you have it.
pub fn sniff(b: &[u8]) -> Option<Format> {
    if b.len() < 4 {
        return None;
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some(Format::Gif);
    }
    if b.get(0..4) == Some(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        return Some(Format::WebP);
    }
    if b.starts_with(&[0xFF, 0x0A])
        || b.starts_with(&[
            0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
        ])
    {
        return Some(Format::Jxl);
    }
    if b.starts_with(b"8BPS") && matches!(b.get(4..6), Some([0, 1] | [0, 2])) {
        return Some(Format::Psd);
    }
    if b.starts_with(b"FUJIFILMCCD-RAW") || b.starts_with(b"\0MRM") || b.starts_with(b"FOVb") {
        return Some(Format::Raw);
    }
    if b.get(0..2) == Some(b"II") && b.get(6..14) == Some(b"HEAPCCDR") {
        return Some(Format::Raw);
    }
    if b.get(4..8) == Some(b"ftyp") {
        return sniff_bmff(b);
    }
    // Olympus ORF / Panasonic RW2: TIFF structure behind a private magic.
    if b.starts_with(b"IIRO")
        || b.starts_with(b"IIRS")
        || b.starts_with(b"MMOR")
        || b.starts_with(b"IIU\0")
    {
        return Some(Format::Raw);
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        return Some(if tiff_is_raw(b) {
            Format::Raw
        } else {
            Format::Tiff
        });
    }
    if b.starts_with(b"II+\0") || b.starts_with(b"MM\0+") {
        return Some(Format::Tiff);
    }
    if b.starts_with(b"BM") && b.len() >= 26 {
        let dib = crate::bytes::le_u32(b, 14)?;
        if matches!(dib, 12 | 16 | 40 | 52 | 56 | 64 | 108 | 124) {
            return Some(Format::Bmp);
        }
    }
    None
}

/// ISO BMFF brands (ISO/IEC 14496-12 §4.3): the major brand and the compatible list.
fn sniff_bmff(b: &[u8]) -> Option<Format> {
    let size = crate::bytes::be_u32(b, 0)? as usize;
    let end = size.clamp(12, b.len().min(4096));
    let mut brands: Vec<&[u8]> = vec![b.get(8..12)?];
    let mut p = 16;
    while p + 4 <= end {
        brands.push(b.get(p..p + 4)?);
        p += 4;
    }
    let has = |x: &[u8]| brands.contains(&x);
    if has(b"crx ") {
        return Some(Format::Raw);
    }
    if has(b"jxl ") {
        return Some(Format::Jxl);
    }
    if has(b"avif") || has(b"avis") {
        return Some(Format::Avif);
    }
    let heif: [&[u8]; 10] = [
        b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"msf1",
    ];
    if heif.iter().any(|x| has(x)) {
        return Some(Format::Heif);
    }
    None
}

/// Whether a TIFF-structured file is a camera raw: Canon's `CR` marker, a DNGVersion tag, or a
/// raw-only photometric interpretation or compression in IFD0 or a SubIFD.
fn tiff_is_raw(b: &[u8]) -> bool {
    if b.get(8..10) == Some(b"CR") {
        return true;
    }
    let Some(t) = Tiff::new(b) else { return false };
    let Some(first) = t.first_ifd() else {
        return false;
    };
    let mut stack = vec![first];
    let mut seen = 0;
    while let Some(pos) = stack.pop() {
        seen += 1;
        if seen > 16 {
            break;
        }
        let Some(ifd) = t.ifd(pos) else { continue };
        for e in &ifd.entries {
            match e.tag {
                0xC612 => return true, // DNGVersion
                // PhotometricInterpretation: CFA, LinearRaw
                0x0106 if matches!(t.uint(e), Some(32803 | 34892)) => return true,
                // Compression codes only raw writers use (Nikon, Sony, Pentax, Kodak, …)
                0x0103
                    if matches!(
                        t.uint(e),
                        Some(32767 | 32769 | 32770 | 34316 | 34713 | 65000 | 65535)
                    ) =>
                {
                    return true;
                }
                // SubIFDs
                0x014A => {
                    for i in 0..(e.count as usize).min(8) {
                        if let Some(p) = t.uint_at(e, i) {
                            stack.push(p as usize);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiff::testutil::{Val, build};

    #[test]
    fn magic_numbers() {
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0\0\x10JFIF"), Some(Format::Jpeg));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some(Format::Png));
        assert_eq!(sniff(b"GIF89a......"), Some(Format::Gif));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::WebP));
        assert_eq!(
            sniff(b"\0\0\0\x1cftypavif\0\0\0\0avifmif1miaf"),
            Some(Format::Avif)
        );
        assert_eq!(
            sniff(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Some(Format::Heif)
        );
        assert_eq!(
            sniff(b"\0\0\0\x18ftypcrx \0\0\0\x01crx isom"),
            Some(Format::Raw)
        );
        assert_eq!(sniff(&[0xFF, 0x0A, 0xFA, 0x7F]), Some(Format::Jxl));
        assert_eq!(sniff(b"8BPS\0\x01\0\0\0\0\0\0"), Some(Format::Psd));
        assert_eq!(sniff(b"8BPS\0\x03\0\0\0\0\0\0"), None);
        assert_eq!(sniff(b"FUJIFILMCCD-RAW 0201"), Some(Format::Raw));
        assert_eq!(sniff(b"II*\0\x10\0\0\0CR\x02\0"), Some(Format::Raw));
        assert_eq!(sniff(b"IIRO\x08\0\0\0"), Some(Format::Raw));
        assert_eq!(sniff(b"MM\0+\0\x08\0\0"), Some(Format::Tiff));
        assert_eq!(sniff(b"nope"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn dng_is_raw_and_plain_tiff_is_not() {
        let plain = build(
            vec![(0x0100, Val::Short(4)), (0x0101, Val::Short(4))],
            vec![],
        );
        assert_eq!(sniff(&plain), Some(Format::Tiff));
        let dng = build(
            vec![
                (0x0100, Val::Short(4)),
                (0xC612, Val::Bytes(vec![1, 4, 0, 0])),
            ],
            vec![],
        );
        assert_eq!(sniff(&dng), Some(Format::Raw));
    }

    #[test]
    fn decodability_is_reported() {
        assert!(Format::Jpeg.can_decode());
        assert!(Format::Psd.can_decode());
        assert!(Format::Heif.can_decode() && Format::Avif.can_decode());
        assert!(!Format::Jxl.can_decode());
        assert_eq!(Format::WebP.mime(), "image/webp");
        assert_eq!(Format::Jxl.to_string(), "JPEG XL");
    }
}
