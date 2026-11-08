//! # craft-image
//!
//! Still images for asset pipelines: everything a media catalogue, a CMS or a delivery worker
//! needs between "a file arrived" and "here are its renditions".
//!
//! * [`sniff`] and [`read_header`]: format, size, depth, alpha, animation and orientation from
//!   the headers alone (JPEG, PNG, GIF, WebP, TIFF, BMP, PSD, HEIF, AVIF, TIFF-based raws).
//! * [`embedded`] finds the EXIF, XMP, ICC and IPTC blocks; [`Metadata`] decodes and merges
//!   them (camera, exposure, capture time, GPS, title, caption, alt text, keywords, creators,
//!   rights, location).
//! * [`probe`] does both in one call, for catalogues.
//! * [`decode`] gives 8-bit RGBA, with DCT-scaled JPEG decoding when a target size is known.
//! * [`thumbnail`] and [`render`] produce colour-managed (sRGB through `craft-cms`), upright,
//!   resized output, resampled in linear light, encoded as JPEG, PNG or lossless WebP with a
//!   metadata policy that never leaks location.
//!
//! Untrusted input is the normal case: sizes are checked against [`Limits`] before any pixel
//! buffer is allocated, every offset read from a file is bounds-checked, recursion is bounded,
//! and third-party decoders run behind a panic guard.
//!
//! ```
//! use craft_image::{OutputFormat, RenditionSpec, Rgba8Image, encode_plain, probe, render};
//!
//! let original = encode_plain(&Rgba8Image::filled(400, 300, [40, 90, 200, 255])?, OutputFormat::Png)?;
//! let info = probe(&original)?;
//! assert_eq!((info.header.width, info.header.height), (400, 300));
//!
//! let web = render(&original, &RenditionSpec::new(200, 200, OutputFormat::Jpeg { quality: 82 }))?;
//! assert_eq!((web.width, web.height, web.mime), (200, 150, "image/jpeg"));
//! # Ok::<(), craft_image::Error>(())
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
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod bytes;
mod container;
mod decode;
mod encode;
mod error;
mod exif;
mod format;
mod formats;
mod header;
mod icc;
mod iptc;
mod metadata;
mod orientation;
mod pixels;
mod rendition;
mod resize;
mod thumbnail;
mod tiff;
mod xmp;
pub mod yuv;

pub use container::{Embedded, embedded};
pub use decode::{DecodeOptions, Decoded, decode};
pub use encode::{EncodeOptions, OutputFormat, encode, encode_plain};
pub use error::{Error, Result};
pub use exif::{Exif, Gps, parse_exif};
pub use format::{Format, sniff};
pub use header::{Header, read_header};
pub use icc::{ColorConversion, IccInfo, icc_info};
pub use iptc::{Iptc, parse_iptc};
pub use metadata::Metadata;
pub use orientation::Orientation;
pub use pixels::{Limits, Rgba8Image};
pub use rendition::{MetadataPolicy, Rendition, RenditionSpec, render};
pub use resize::{Filter, ResizeOptions, fit_within, resize, resize_to_fit};
pub use thumbnail::{Thumbnail, ThumbnailOptions, ThumbnailSource, thumbnail};
pub use xmp::{Xmp, XmpError, parse_xmp};

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

/// How much of a file [`probe_file`] reads by default: enough for the headers and metadata of
/// every format here, including JPEGs with large ICC profiles.
pub const DEFAULT_PROBE_BYTES: u64 = 16 << 20;

/// Everything known about a file without decoding its pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Format, size, depth, alpha, animation, orientation.
    pub header: Header,
    /// Merged EXIF, XMP and IPTC.
    pub metadata: Metadata,
    /// The embedded ICC profile, described.
    pub icc: Option<IccInfo>,
    /// Whether EXIF carries a JPEG preview.
    pub has_embedded_thumbnail: bool,
    /// Whether this build can decode the pixels.
    pub decodable: bool,
    /// Blocks that could not be read. The rest of the result is still valid.
    pub warnings: Vec<String>,
}

/// Read a file's header and metadata from its bytes (or a prefix of them).
pub fn probe(bytes: &[u8]) -> Result<ImageInfo> {
    let header = read_header(bytes)?;
    let blocks = embedded(bytes);
    let (metadata, mut warnings) = Metadata::from_embedded(&blocks);
    let icc = blocks.icc.as_deref().and_then(|p| {
        let info = icc_info(p);
        if info.is_none() {
            warnings.push("ICC block is not a profile".to_string());
        }
        info
    });
    let has_embedded_thumbnail = metadata
        .exif
        .as_ref()
        .is_some_and(|x| x.thumbnail.is_some());
    Ok(ImageInfo {
        decodable: header.format.can_decode(),
        header,
        metadata,
        icc,
        has_embedded_thumbnail,
        warnings,
    })
}

/// [`probe`] the first `max_bytes` of a file.
pub fn probe_file(path: &Path, max_bytes: u64) -> Result<ImageInfo> {
    let mut buf = Vec::new();
    std::fs::File::open(path)?
        .take(max_bytes)
        .read_to_end(&mut buf)?;
    probe(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_reports_header_metadata_and_profile() {
        let img = Rgba8Image::filled(30, 20, [1, 2, 3, 255]).unwrap();
        let opts = EncodeOptions {
            icc: Some(icc::srgb_profile()),
            exif: Some(crate::exif::tests::camera_exif()),
            ..Default::default()
        };
        let bytes = encode(&img, OutputFormat::Jpeg { quality: 80 }, &opts).unwrap();
        let info = probe(&bytes).unwrap();
        assert_eq!((info.header.width, info.header.height), (30, 20));
        assert_eq!(info.header.orientation, Orientation::Rotate90);
        assert_eq!(info.header.display_size(), (20, 30));
        assert_eq!(info.metadata.camera_model.as_deref(), Some("Canon EOS R5"));
        assert!(info.icc.as_ref().is_some_and(|i| i.is_srgb));
        assert!(info.decodable);
        assert!(info.warnings.is_empty(), "{:?}", info.warnings);
        // Serialisable for catalogues and APIs, and back.
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"camera_model\":\"Canon EOS R5\""), "{json}");
        let back: ImageInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.header, info.header);
        assert_eq!(back.metadata.camera_model, info.metadata.camera_model);
    }

    #[test]
    fn probe_file_reads_a_prefix() {
        let img = Rgba8Image::filled(30, 20, [1, 2, 3, 255]).unwrap();
        let bytes = encode_plain(&img, OutputFormat::Png).unwrap();
        let dir = std::env::temp_dir().join(format!("craft-image-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        std::fs::write(&path, &bytes).unwrap();
        // The IHDR is in the first 33 bytes.
        let info = probe_file(&path, 64).unwrap();
        assert_eq!(info.header.width, 30);
        assert!(probe_file(&dir.join("missing.png"), 64).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
