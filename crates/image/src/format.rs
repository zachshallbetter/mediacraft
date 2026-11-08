//! Format detection from leading bytes.

use crate::Error;
use crate::formats;
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
    /// OpenEXR (floating-point, scene-linear).
    Exr,
    /// Radiance HDR (RGBE, `.hdr` / `.pic`).
    Hdr,
    /// DirectDraw Surface (block-compressed textures).
    Dds,
    /// Windows icon or cursor (ICO and CUR).
    Ico,
    /// Netpbm: PBM, PGM, PPM and PAM.
    Pnm,
    /// Quite OK Image.
    Qoi,
    /// Truevision TGA.
    Tga,
    /// DPX (SMPTE 268M film scans).
    Dpx,
    /// ZSoft PCX.
    Pcx,
    /// FITS (astronomical images).
    Fits,
    /// DICOM (medical images, uncompressed transfer syntaxes).
    Dicom,
    /// SVG (and gzip-compressed `.svgz`).
    Svg,
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
            Format::Exr => "exr",
            Format::Hdr => "hdr",
            Format::Dds => "dds",
            Format::Ico => "ico",
            Format::Pnm => "pnm",
            Format::Qoi => "qoi",
            Format::Tga => "tga",
            Format::Dpx => "dpx",
            Format::Pcx => "pcx",
            Format::Fits => "fits",
            Format::Dicom => "dcm",
            Format::Svg => "svg",
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
            Format::Exr => "image/x-exr",
            Format::Hdr => "image/vnd.radiance",
            Format::Dds => "image/vnd.ms-dds",
            Format::Ico => "image/vnd.microsoft.icon",
            Format::Pnm => "image/x-portable-anymap",
            Format::Qoi => "image/qoi",
            Format::Tga => "image/x-tga",
            Format::Dpx => "image/x-dpx",
            Format::Pcx => "image/x-pcx",
            Format::Fits => "image/fits",
            Format::Dicom => "application/dicom",
            Format::Svg => "image/svg+xml",
            Format::Raw => "image/x-raw",
        }
    }

    /// Why this build cannot decode the format's pixels, `None` when it can. Headers and
    /// metadata are read wherever they need no optional dependency, so a catalogue can still
    /// identify a file it cannot decode.
    pub fn not_decodable(self) -> Option<&'static str> {
        // The decoder is left out of the browser build to keep its wasm under the size cap.
        if self == Format::Jxl && cfg!(target_arch = "wasm32") {
            return Some("JPEG XL decoding is not in the browser build");
        }
        // Whether the cargo feature that provides the decoder is on.
        let (built, why) = match self {
            Format::Jpeg => (cfg!(feature = "jpeg"), "the `jpeg` feature is off"),
            Format::Png => (cfg!(feature = "png"), "the `png` feature is off"),
            Format::Gif => (cfg!(feature = "gif"), "the `gif` feature is off"),
            Format::WebP => (cfg!(feature = "webp"), "the `webp` feature is off"),
            Format::Tiff => (cfg!(feature = "tiff"), "the `tiff` feature is off"),
            Format::Bmp => (cfg!(feature = "bmp"), "the `bmp` feature is off"),
            Format::Psd => (cfg!(feature = "psd"), "the `psd` feature is off"),
            Format::Heif => (cfg!(feature = "heic"), "the `heic` feature is off"),
            Format::Avif => (cfg!(feature = "avif"), "the `avif` feature is off"),
            Format::Jxl => (cfg!(feature = "jxl"), "the `jxl` feature is off"),
            Format::Exr => (cfg!(feature = "exr"), "the `exr` feature is off"),
            Format::Hdr => (cfg!(feature = "hdr"), "the `hdr` feature is off"),
            Format::Dds => (cfg!(feature = "dds"), "the `dds` feature is off"),
            Format::Ico => (cfg!(feature = "ico"), "the `ico` feature is off"),
            Format::Pnm => (cfg!(feature = "pnm"), "the `pnm` feature is off"),
            Format::Qoi => (cfg!(feature = "qoi"), "the `qoi` feature is off"),
            Format::Tga => (cfg!(feature = "tga"), "the `tga` feature is off"),
            Format::Dpx => (cfg!(feature = "dpx"), "the `dpx` feature is off"),
            Format::Pcx => (cfg!(feature = "pcx"), "the `pcx` feature is off"),
            Format::Fits => (cfg!(feature = "fits"), "the `fits` feature is off"),
            Format::Dicom => (cfg!(feature = "dicom"), "the `dicom` feature is off"),
            Format::Svg => (cfg!(feature = "svg"), "the `svg` feature is off"),
            Format::Raw => (cfg!(feature = "raw"), "the `raw` feature is off"),
        };
        (!built).then_some(why)
    }

    /// The error for pixels this build cannot decode. Only the shims for features that are off
    /// call it, so a build with every feature on never does.
    #[allow(dead_code)]
    pub(crate) fn unsupported(self) -> Error {
        Error::Unsupported(
            self,
            self.not_decodable()
                .unwrap_or("no decoder is built for this format"),
        )
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
            Format::Exr => "OpenEXR",
            Format::Hdr => "Radiance HDR",
            Format::Dds => "DDS",
            Format::Ico => "ICO",
            Format::Pnm => "PNM",
            Format::Qoi => "QOI",
            Format::Tga => "TGA",
            Format::Dpx => "DPX",
            Format::Pcx => "PCX",
            Format::Fits => "FITS",
            Format::Dicom => "DICOM",
            Format::Svg => "SVG",
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
    // DICOM keeps its marker after a 128-byte preamble that may hold anything (even a TIFF
    // header), so look for it before the TIFF checks.
    if formats::dicom::sniff(b) {
        return Some(Format::Dicom);
    }
    // FITS is the one text-led format here; its `SIMPLE  =` card is unambiguous.
    if formats::fits::sniff(b) {
        return Some(Format::Fits);
    }
    if formats::dpx::sniff(b) {
        return Some(Format::Dpx);
    }
    if b.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Some(Format::Exr);
    }
    if b.starts_with(b"#?RADIANCE") || b.starts_with(b"#?RGBE") {
        return Some(Format::Hdr);
    }
    if b.starts_with(b"DDS ") && crate::bytes::le_u32(b, 4) == Some(124) {
        return Some(Format::Dds);
    }
    if b.starts_with(b"qoif") {
        return Some(Format::Qoi);
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
    // Magic-less formats last, so nothing with a real signature can be mistaken for them.
    if tga_has_footer(b) {
        return Some(Format::Tga);
    }
    if is_ico(b) {
        return Some(Format::Ico);
    }
    if is_pnm(b) {
        return Some(Format::Pnm);
    }
    if formats::pcx::sniff(b) {
        return Some(Format::Pcx);
    }
    if tga_header_plausible(b) {
        return Some(Format::Tga);
    }
    if formats::svg::sniff(b) {
        return Some(Format::Svg);
    }
    None
}

/// A TGA 2.0 footer: the last 18 bytes are `TRUEVISION-XFILE.` and a NUL. Only visible when the
/// slice reaches the end of the file.
fn tga_has_footer(b: &[u8]) -> bool {
    b.len() >= 18 + 26 && b.ends_with(b"TRUEVISION-XFILE.\0")
}

/// ICO (`00 00 01 00`) or CUR (`00 00 02 00`): a non-empty directory whose first entry is
/// coherent. A truecolour TGA starts with the same four bytes as a CUR, but its colour-map
/// length (the directory count here) is zero, which a real icon directory cannot be.
fn is_ico(b: &[u8]) -> bool {
    if !(b.starts_with(&[0, 0, 1, 0]) || b.starts_with(&[0, 0, 2, 0])) {
        return false;
    }
    let Some(count) = crate::bytes::le_u16(b, 4).filter(|&c| c > 0) else {
        return false;
    };
    // Entry 0: width, height, colours, reserved (0), planes, bits, size, offset.
    let (Some(&reserved), Some(size), Some(offset)) = (
        b.get(9),
        crate::bytes::le_u32(b, 14),
        crate::bytes::le_u32(b, 18),
    ) else {
        return false;
    };
    reserved == 0 && size > 0 && offset as usize >= 6 + 16 * usize::from(count)
}

/// Netpbm: `P1`..`P6` then whitespace, or `P7\n` (PAM).
fn is_pnm(b: &[u8]) -> bool {
    matches!(b.first(), Some(b'P'))
        && matches!(b.get(1), Some(b'1'..=b'7'))
        && matches!(b.get(2), Some(b' ' | b'\t' | b'\r' | b'\n'))
}

/// TGA has no magic number. Without a footer, accept only a header whose fields are all
/// consistent with the specification: colour-map type 0 or 1, an image type in {1, 2, 3, 9, 10,
/// 11} that agrees with the colour-map type, a pixel depth the type allows, a colour-map entry
/// size when there is a map, non-zero width and height, and zero interleave bits.
fn tga_header_plausible(b: &[u8]) -> bool {
    let (Some(&cmap_type), Some(&image_type), Some(&bpp), Some(&desc)) =
        (b.get(1), b.get(2), b.get(16), b.get(17))
    else {
        return false;
    };
    let (Some(w), Some(h)) = (crate::bytes::le_u16(b, 12), crate::bytes::le_u16(b, 14)) else {
        return false;
    };
    if cmap_type > 1 || w == 0 || h == 0 || desc >> 6 != 0 {
        return false;
    }
    let depth_ok = match image_type {
        1 | 9 => cmap_type == 1 && bpp == 8,
        2 | 10 => matches!(bpp, 15 | 16 | 24 | 32),
        3 | 11 => matches!(bpp, 8 | 16),
        _ => false,
    };
    if !depth_ok {
        return false;
    }
    if cmap_type == 1 {
        let (Some(len), Some(&entry)) = (crate::bytes::le_u16(b, 5), b.get(7)) else {
            return false;
        };
        return len > 0 && matches!(entry, 15 | 16 | 24 | 32);
    }
    true
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
        assert_eq!(sniff(b"v/1\x01\x02\0\0\0"), Some(Format::Exr));
        assert_eq!(sniff(b"#?RADIANCE\nFORMAT="), Some(Format::Hdr));
        assert_eq!(sniff(b"#?RGBE\n"), Some(Format::Hdr));
        assert_eq!(sniff(b"qoif\0\0\0\x01"), Some(Format::Qoi));
        assert_eq!(sniff(b"P6\n2 2\n255\n"), Some(Format::Pnm));
        assert_eq!(sniff(b"P7\nWIDTH 2\n"), Some(Format::Pnm));
        assert_eq!(sniff(b"P8\n2 2\n"), None);
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
    fn decodability_follows_the_features() {
        assert_eq!(Format::Jpeg.can_decode(), cfg!(feature = "jpeg"));
        assert_eq!(Format::Png.can_decode(), cfg!(feature = "png"));
        assert_eq!(Format::Psd.can_decode(), cfg!(feature = "psd"));
        assert_eq!(Format::Heif.can_decode(), cfg!(feature = "heic"));
        assert_eq!(Format::Avif.can_decode(), cfg!(feature = "avif"));
        assert_eq!(
            Format::Jxl.can_decode(),
            cfg!(feature = "jxl") && !cfg!(target_arch = "wasm32")
        );
        assert_eq!(Format::Exr.can_decode(), cfg!(feature = "exr"));
        assert_eq!(Format::Tga.can_decode(), cfg!(feature = "tga"));
        assert_eq!(Format::Dicom.can_decode(), cfg!(feature = "dicom"));
        assert_eq!(Format::Raw.can_decode(), cfg!(feature = "raw"));
        assert_eq!(Format::WebP.mime(), "image/webp");
        assert_eq!(Format::Jxl.to_string(), "JPEG XL");
    }

    #[test]
    fn dds_needs_its_header_size() {
        let mut b = b"DDS ".to_vec();
        b.extend_from_slice(&124u32.to_le_bytes());
        assert_eq!(sniff(&b), Some(Format::Dds));
        assert_eq!(sniff(b"DDS \x7B\0\0\0"), None);
    }

    /// A TGA header: type, colour-map type, width, height, depth.
    fn tga_head(cmap: u8, ty: u8, w: u16, h: u16, bpp: u8) -> Vec<u8> {
        let mut b = vec![0, cmap, ty, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b.extend_from_slice(&[bpp, 0]);
        b
    }

    #[test]
    fn tga_is_found_by_footer_or_a_strict_header() {
        // A truecolour TGA begins `00 00 02 00`, like a cursor, and is still a TGA.
        assert_eq!(sniff(&tga_head(0, 2, 4, 4, 24)), Some(Format::Tga));
        assert_eq!(sniff(&tga_head(0, 10, 4, 4, 32)), Some(Format::Tga));
        assert_eq!(sniff(&tga_head(0, 3, 4, 4, 8)), Some(Format::Tga));
        // Out-of-range fields: image type, depth, zero size, colour-map type.
        assert_eq!(sniff(&tga_head(0, 4, 4, 4, 24)), None);
        assert_eq!(sniff(&tga_head(0, 2, 4, 4, 7)), None);
        assert_eq!(sniff(&tga_head(0, 2, 0, 4, 24)), None);
        assert_eq!(sniff(&tga_head(2, 2, 4, 4, 24)), None);
        assert_eq!(
            sniff(&tga_head(0, 1, 4, 4, 8)),
            None,
            "palette type without a palette"
        );
        // A footer settles it even when the header looks odd.
        let mut odd = tga_head(0, 2, 4, 4, 24);
        odd[2] = 0x20;
        odd.extend_from_slice(&[0; 8]);
        odd.extend_from_slice(&[0; 8]);
        odd.extend_from_slice(b"TRUEVISION-XFILE.\0");
        assert_eq!(sniff(&odd), Some(Format::Tga));
    }

    #[test]
    fn ico_and_cur_need_a_coherent_directory() {
        let mut ico = vec![0, 0, 1, 0, 1, 0, 16, 16, 0, 0, 1, 0, 32, 0];
        ico.extend_from_slice(&40u32.to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        assert_eq!(sniff(&ico), Some(Format::Ico));
        ico[2] = 2;
        assert_eq!(sniff(&ico), Some(Format::Ico));
        // Zero entries, or an image offset inside the directory itself: not an icon.
        let mut none = ico.clone();
        none[4] = 0;
        assert_ne!(sniff(&none), Some(Format::Ico));
        let mut inside = ico.clone();
        inside[18] = 8;
        assert_ne!(sniff(&inside), Some(Format::Ico));
    }
}
