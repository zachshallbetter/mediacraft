//! Renditions: an original turned into a delivery file of bounded size, in sRGB, upright, with
//! only the metadata a policy allows.

use crate::container::embedded;
use crate::encode::{EncodeOptions, OutputFormat, encode};
use crate::exif::minimal_exif;
use crate::icc::{ColorConversion, srgb_profile};
use crate::resize::{ResizeOptions, fit_within, resize};
use crate::{
    DecodeOptions, Format, Limits, Metadata, Orientation, RawMode, Result, decode, read_header,
};
use serde::{Deserialize, Serialize};

/// Which metadata a rendition keeps. GPS, camera serials and the EXIF thumbnail are never
/// copied: a rendition is for publishing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataPolicy {
    /// No EXIF.
    Strip,
    /// A minimal EXIF with the creator and copyright notice (JPEG, PNG and lossy WebP; lossless
    /// WebP output carries no EXIF).
    #[default]
    Attribution,
}

/// What to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenditionSpec {
    /// Fit within this width…
    pub max_width: u32,
    /// …and this height, displayed upright. Never enlarges.
    pub max_height: u32,
    /// Output encoding.
    pub format: OutputFormat,
    /// Metadata to keep.
    pub metadata: MetadataPolicy,
    /// Embed the sRGB profile. Off by default: untagged images are shown as sRGB anyway.
    pub embed_srgb_profile: bool,
    /// Resampling.
    pub resize: ResizeOptions,
    /// Resource ceilings for the decode.
    pub limits: Limits,
    /// Background for flattening transparency into JPEG.
    pub background: [u8; 3],
    /// How to read camera raws. The embedded preview by default; it is developed instead when
    /// the preview is smaller than the rendition.
    pub raw: RawMode,
}

impl RenditionSpec {
    /// A rendition fitted within `max_width × max_height`, attribution kept, white background.
    pub fn new(max_width: u32, max_height: u32, format: OutputFormat) -> Self {
        Self {
            max_width,
            max_height,
            format,
            metadata: MetadataPolicy::default(),
            embed_srgb_profile: false,
            resize: ResizeOptions::default(),
            limits: Limits::default(),
            background: [255, 255, 255],
            raw: RawMode::default(),
        }
    }
}

/// A rendered file.
#[derive(Clone, Debug)]
pub struct Rendition {
    /// The encoded file.
    pub bytes: Vec<u8>,
    /// Its format.
    pub format: Format,
    /// Its media type.
    pub mime: &'static str,
    /// Width as displayed.
    pub width: u32,
    /// Height as displayed.
    pub height: u32,
    /// The colour conversion applied to reach sRGB.
    pub color: ColorConversion,
}

/// Render one file.
pub fn render(bytes: &[u8], spec: &RenditionSpec) -> Result<Rendition> {
    let header = read_header(bytes)?;
    let (dw, dh) = header.display_size();
    let (fw, fh) = fit_within(dw, dh, spec.max_width.max(1), spec.max_height.max(1));
    // The same box in stored orientation.
    let (sw, sh) = if header.orientation.swaps_dimensions() {
        (fh, fw)
    } else {
        (fw, fh)
    };

    let d = decode(
        bytes,
        &DecodeOptions {
            limits: spec.limits,
            fit: Some((sw, sh)),
            raw: spec.raw,
            ..Default::default()
        },
    )?;
    let mut sized = resize(&d.image, sw, sh, &spec.resize)?;
    let color = d.convert_to_srgb(&mut sized);
    let upright = d.orientation.apply(&sized)?;

    let exif = match spec.metadata {
        MetadataPolicy::Strip => None,
        MetadataPolicy::Attribution => {
            let (m, _) = Metadata::from_embedded(&embedded(bytes));
            let creator = (!m.creators.is_empty()).then(|| m.creators.join("; "));
            Some(minimal_exif(
                Orientation::Identity,
                creator.as_deref(),
                m.copyright.as_deref(),
            ))
        }
    };
    let opts = EncodeOptions {
        icc: spec.embed_srgb_profile.then(srgb_profile),
        exif,
        background: spec.background,
    };
    let out = encode(&upright, spec.format, &opts)?;
    let format = spec.format.format();
    Ok(Rendition {
        bytes: out,
        format,
        mime: format.mime(),
        width: upright.width(),
        height: upright.height(),
        color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "png")]
    use crate::encode::encode_plain;
    use crate::{Rgba8Image, parse_exif};

    /// A 300×200 JPEG whose EXIF says "rotate 90° clockwise", with camera, GPS and rights.
    #[cfg(feature = "jpeg")]
    fn rotated_camera_jpeg() -> Vec<u8> {
        let mut d = Vec::new();
        for _y in 0..200u32 {
            for x in 0..300u32 {
                // Red on the left third, so we can see where it ends up.
                d.extend_from_slice(if x < 100 {
                    &[230, 20, 20, 255]
                } else {
                    &[20, 20, 230, 255]
                });
            }
        }
        let img = Rgba8Image::new(300, 200, d).unwrap();
        let exif = crate::exif::tests::camera_exif(); // orientation 6, GPS, artist, copyright
        encode(
            &img,
            OutputFormat::Jpeg { quality: 92 },
            &EncodeOptions {
                exif: Some(exif),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[cfg(feature = "jpeg")]
    #[test]
    fn renders_upright_fitted_and_without_location() {
        let src = rotated_camera_jpeg();
        let r = render(
            &src,
            &RenditionSpec::new(100, 100, OutputFormat::Jpeg { quality: 85 }),
        )
        .unwrap();
        // Stored 300×200 turned upright is 200×300; fitted within 100×100 → 67×100.
        assert_eq!((r.width, r.height), (67, 100));
        let h = read_header(&r.bytes).unwrap();
        assert_eq!(
            (h.width, h.height, h.orientation),
            (67, 100, Orientation::Identity)
        );
        // Turning clockwise moves the stored left edge (red) to the top.
        let d = decode(&r.bytes, &DecodeOptions::default()).unwrap();
        let top = d.image.pixel(33, 5).unwrap();
        let bottom = d.image.pixel(33, 95).unwrap();
        assert!(top[0] > 150 && top[2] < 100, "top {top:?}");
        assert!(bottom[2] > 150 && bottom[0] < 100, "bottom {bottom:?}");

        let x = parse_exif(embedded(&r.bytes).exif.as_deref().unwrap()).unwrap();
        assert_eq!(x.gps, None, "no location in renditions");
        assert_eq!(x.make, None, "no camera in renditions");
        assert_eq!(x.artist.as_deref(), Some("Ada Photographer"));
        assert_eq!(x.copyright.as_deref(), Some("(c) 2024 Ada"));
    }

    #[cfg(all(feature = "png", feature = "webp"))]
    #[test]
    fn strip_policy_writes_no_exif_and_small_sources_are_not_enlarged() {
        let src = encode_plain(
            &Rgba8Image::filled(40, 30, [9, 9, 9, 255]).unwrap(),
            OutputFormat::Png,
        )
        .unwrap();
        let spec = RenditionSpec {
            metadata: MetadataPolicy::Strip,
            ..RenditionSpec::new(1000, 1000, OutputFormat::WebpLossless)
        };
        let r = render(&src, &spec).unwrap();
        assert_eq!(
            (r.width, r.height, r.format, r.mime),
            (40, 30, Format::WebP, "image/webp")
        );
        assert!(embedded(&r.bytes).exif.is_none());
    }

    #[cfg(feature = "png")]
    #[test]
    fn embeds_srgb_when_asked() {
        let src = encode_plain(
            &Rgba8Image::filled(8, 8, [9, 9, 9, 255]).unwrap(),
            OutputFormat::Png,
        )
        .unwrap();
        let spec = RenditionSpec {
            embed_srgb_profile: true,
            ..RenditionSpec::new(4, 4, OutputFormat::Png)
        };
        let r = render(&src, &spec).unwrap();
        let icc = embedded(&r.bytes).icc.unwrap();
        assert!(crate::icc_info(&icc).unwrap().is_srgb);
    }
}
