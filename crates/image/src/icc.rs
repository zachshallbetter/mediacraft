//! ICC profiles: a description of the embedded profile, and conversion of decoded pixels to
//! sRGB through `craft-cms`, the Craft suite's colour management module.

use crate::Rgba8Image;
use craft_cms::{Profile, TransformOptions, builtin};
use serde::{Deserialize, Serialize};

/// What an embedded ICC profile is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IccInfo {
    /// The profile's description (`desc`), e.g. "Display P3".
    pub description: Option<String>,
    /// Data colour space signature, trimmed: `RGB`, `GRAY`, `CMYK`, `Lab`.
    pub color_space: String,
    /// Device class signature: `mntr`, `scnr`, `prtr`, `spac`, …
    pub class: String,
    /// Profile version, e.g. `4.3`.
    pub version: String,
    /// Size in bytes.
    pub size: usize,
    /// Whether the profile reproduces the same colours as sRGB.
    pub is_srgb: bool,
}

/// Describe a profile. `None` when the bytes are not an ICC profile.
pub fn icc_info(bytes: &[u8]) -> Option<IccInfo> {
    // Header fields (ICC.1:2010 §7.2): version at 8, class at 12, colour space at 16, `acsp` at 36.
    if bytes.len() < 128 || bytes.get(36..40) != Some(b"acsp") {
        return None;
    }
    let sig = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|s| String::from_utf8_lossy(s).trim().to_string())
            .unwrap_or_default()
    };
    let (major, minor) = (bytes[8], bytes[9] >> 4);
    let profile = Profile::parse(bytes).ok();
    let description = profile
        .as_ref()
        .map(|p| p.description.trim().to_string())
        .filter(|d| !d.is_empty());
    let is_srgb = profile
        .as_ref()
        .is_some_and(|p| p.same_colors(builtin::srgb()));
    Some(IccInfo {
        description,
        color_space: sig(16),
        class: sig(12),
        version: format!("{major}.{minor}"),
        size: bytes.len(),
        is_srgb,
    })
}

/// What [`to_srgb`] did to the pixels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome", content = "detail")]
pub enum ColorConversion {
    /// Pixels were converted from the embedded (or implied) profile to sRGB.
    Converted,
    /// The source is already sRGB, or carries no profile and is taken as sRGB.
    AssumedSrgb,
    /// The profile could not be used; pixels are as decoded. The reason says why.
    Kept(String),
}

/// Profiles the pixels may be tagged with.
pub(crate) enum Source<'a> {
    Embedded(&'a [u8]),
    /// EXIF says Adobe RGB (ColorSpace uncalibrated + interop `R03`) without an ICC profile.
    AdobeRgbHint,
    None,
}

/// Convert RGB pixels to sRGB (relative colorimetric with black point compensation).
/// Gray and CMYK profiles are left alone: the decoder has already expanded those to RGB.
pub(crate) fn to_srgb(img: &mut Rgba8Image, source: Source<'_>) -> ColorConversion {
    let parsed;
    let src: &Profile = match source {
        Source::None => return ColorConversion::AssumedSrgb,
        Source::AdobeRgbHint => builtin::Builtin::AdobeRgbCompat.profile(),
        Source::Embedded(bytes) => match Profile::parse(bytes) {
            Ok(p) => {
                parsed = p;
                &parsed
            }
            Err(e) => return ColorConversion::Kept(format!("profile could not be read: {e}")),
        },
    };
    let dst = builtin::srgb();
    if src.same_colors(dst) {
        return ColorConversion::AssumedSrgb;
    }
    if src.channels() != 3 {
        return ColorConversion::Kept(format!(
            "{}-channel profile; the decoder already produced RGB",
            src.channels()
        ));
    }
    let t = match craft_cms::cached(src, dst, TransformOptions::default()) {
        Ok(t) => t,
        Err(e) => return ColorConversion::Kept(format!("no transform to sRGB: {e}")),
    };
    let source_pixels = img.data().to_vec();
    match t.convert_u8(&source_pixels, 4, img.data_mut(), 4, true) {
        Ok(()) => ColorConversion::Converted,
        Err(e) => {
            img.data_mut().copy_from_slice(&source_pixels);
            ColorConversion::Kept(format!("conversion failed: {e}"))
        }
    }
}

/// The sRGB profile bytes, for renditions that embed it.
pub(crate) fn srgb_profile() -> Vec<u8> {
    builtin::srgb().to_bytes().as_ref().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_cms::builtin::Builtin;

    #[test]
    fn describes_builtin_profiles() {
        let srgb = icc_info(&srgb_profile()).unwrap();
        assert!(srgb.is_srgb);
        assert_eq!(srgb.color_space, "RGB");
        let p3 = Builtin::DisplayP3.profile().to_bytes();
        let info = icc_info(&p3).unwrap();
        assert!(!info.is_srgb);
        assert_eq!(info.class, "mntr");
        assert!(icc_info(b"not a profile").is_none());
    }

    #[test]
    fn display_p3_red_becomes_out_of_gamut_clipped_srgb_red() {
        // Pure P3 red is more saturated than sRGB can show: it maps to (255, ~0, ~0) after
        // clipping; a mid P3 gray stays gray.
        let p3 = Builtin::DisplayP3.profile().to_bytes();
        let mut img = Rgba8Image::new(2, 1, vec![255, 0, 0, 255, 128, 128, 128, 77]).unwrap();
        assert_eq!(
            to_srgb(&mut img, Source::Embedded(&p3)),
            ColorConversion::Converted
        );
        let d = img.data();
        assert!(d[0] >= 250 && d[1] <= 10 && d[2] <= 10, "{:?}", &d[..4]);
        assert!(
            d[4].abs_diff(128) <= 2 && d[4].abs_diff(d[5]) <= 1,
            "{:?}",
            &d[4..]
        );
        assert_eq!(d[7], 77, "alpha is carried through");
    }

    #[test]
    fn srgb_and_missing_profiles_leave_pixels_alone() {
        let mut img = Rgba8Image::new(1, 1, vec![10, 20, 30, 255]).unwrap();
        assert_eq!(
            to_srgb(&mut img, Source::Embedded(&srgb_profile())),
            ColorConversion::AssumedSrgb
        );
        assert_eq!(
            to_srgb(&mut img, Source::None),
            ColorConversion::AssumedSrgb
        );
        assert!(matches!(
            to_srgb(&mut img, Source::Embedded(b"garbage")),
            ColorConversion::Kept(_)
        ));
        assert_eq!(img.data(), &[10, 20, 30, 255]);
    }

    #[test]
    fn adobe_rgb_hint_converts() {
        let mut img = Rgba8Image::new(1, 1, vec![0, 255, 0, 255]).unwrap();
        assert_eq!(
            to_srgb(&mut img, Source::AdobeRgbHint),
            ColorConversion::Converted
        );
        // Adobe RGB green is wider than sRGB's; red and blue stay near zero after clipping.
        assert!(img.data()[1] >= 250);
    }
}
