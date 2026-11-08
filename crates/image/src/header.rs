//! Image headers: size, depth, alpha, animation and orientation, without decoding pixels.

use crate::container::embedded;
use crate::exif::parse_exif;
use crate::formats::{self, RawHeader};
use crate::{Error, Format, Orientation, Result, sniff};
use serde::{Deserialize, Serialize};

/// What a file's headers say about its image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// Container format.
    pub format: Format,
    /// Stored width, before orientation.
    pub width: u32,
    /// Stored height, before orientation.
    pub height: u32,
    /// Bits per sample, when the header says.
    pub bit_depth: Option<u8>,
    /// Whether the image carries transparency, when the header can tell.
    pub has_alpha: Option<bool>,
    /// Whether the file holds more than one frame meant to play (GIF, APNG, animated WebP).
    pub animated: bool,
    /// How to turn the stored pixels upright.
    pub orientation: Orientation,
}

impl Header {
    /// Size once displayed upright.
    pub fn display_size(&self) -> (u32, u32) {
        self.orientation.display_size(self.width, self.height)
    }
}

/// Read a file's header. The first few hundred KiB of a file are normally enough; TIFF-based
/// raws may keep their main IFD further in.
pub fn read_header(bytes: &[u8]) -> Result<Header> {
    let format = sniff(bytes).ok_or(Error::UnknownFormat)?;
    let raw: RawHeader = match format {
        Format::Jpeg => formats::jpeg::header(bytes)?,
        Format::Png => formats::png::header(bytes)?,
        Format::WebP => formats::webp::header(bytes)?,
        Format::Gif => formats::gif::header(bytes)?,
        Format::Bmp => formats::bmp::header(bytes)?,
        Format::Tiff => formats::tiff_file::header(bytes, format)?,
        // CR3, RAF, ORF and RW2 are not classic TIFF; their size is the embedded preview's.
        Format::Raw => formats::tiff_file::header(bytes, format)
            .or_else(|e| crate::raw::header(bytes).ok_or(e))?,
        Format::Psd => formats::psd::header(bytes)?,
        Format::Heif | Format::Avif => formats::bmff::header(bytes, format)?,
        Format::Jxl => {
            return Err(Error::Unsupported(
                format,
                "JPEG XL headers are not read yet",
            ));
        }
    };
    // HEIF's own transform properties are authoritative and its EXIF orientation is
    // informational only (ISO/IEC 23008-12 §A.2.1); TIFF tag 274 is the EXIF value itself.
    let orientation = match raw.orientation {
        Some(o) => o,
        None => embedded(bytes)
            .exif
            .as_deref()
            .and_then(parse_exif)
            .and_then(|x| x.orientation)
            .unwrap_or_default(),
    };
    Ok(Header {
        format,
        width: raw.width,
        height: raw.height,
        bit_depth: raw.bit_depth,
        has_alpha: raw.has_alpha,
        animated: raw.animated,
        orientation,
    })
}
