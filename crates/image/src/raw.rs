//! Camera raws through `craft-raw`: the camera's embedded JPEG preview, or a development of the
//! sensor data. Files without colour calibration are developed with a look fitted to the
//! camera's own JPEG, as PhotoCraft's importer does.

use crate::container::embedded;
use crate::decode::{DecodeOptions, RawMode, jpeg};
use crate::formats::RawHeader;
use crate::icc::{self, Source};
use crate::{Error, Format, Orientation, Result, Rgba8Image};
use craft_cms::builtin::Builtin;
use craft_raw::{DevelopOptions, Preview, PreviewColorSpace};

/// A decoded raw: pixels, their ICC profile, the Adobe RGB hint, and whether the pixels are
/// already upright (developed) rather than in the file's stored orientation (previews).
pub(crate) struct RawPixels {
    pub image: Rgba8Image,
    pub icc: Option<Vec<u8>>,
    pub adobe_rgb_hint: bool,
    pub upright: bool,
}

/// Size and orientation for raws whose container is not classic TIFF, from the largest preview.
pub(crate) fn header(bytes: &[u8]) -> Option<RawHeader> {
    let p = craft_raw::embedded_preview(bytes)?;
    Some(RawHeader {
        width: p.width,
        height: p.height,
        bit_depth: None,
        has_alpha: Some(false),
        animated: false,
        orientation: craft_raw::orientation(bytes).and_then(Orientation::from_exif),
    })
}

/// The largest preview showing the whole frame. Cameras also store crops (Sony's 16:9 previews of
/// a 3:2 sensor), which would misrepresent the picture.
fn full_frame_preview(bytes: &[u8], width: u32, height: u32) -> Option<Preview<'_>> {
    let frame = f64::from(width) / f64::from(height.max(1));
    craft_raw::embedded_previews(bytes)
        .into_iter()
        .filter(|p| {
            let a = f64::from(p.width) / f64::from(p.height.max(1));
            // Either way round: a few bodies store portrait previews.
            ((a / frame) - 1.0).abs() < 0.03 || ((a * frame) - 1.0).abs() < 0.03
        })
        .max_by_key(|p| u64::from(p.width) * u64::from(p.height))
}

/// `width × height` is the raw's stored size from its header.
pub(crate) fn decode(
    bytes: &[u8],
    width: u32,
    height: u32,
    opts: &DecodeOptions,
) -> Result<RawPixels> {
    let full = full_frame_preview(bytes, width, height);
    match opts.raw {
        RawMode::Preview => match full {
            // A preview smaller than the caller needs is developed instead, when it can be.
            Some(p) if opts.fit.is_some_and(|(w, h)| p.width < w && p.height < h) => {
                develop(bytes, opts).or_else(|_| preview(bytes, p, opts))
            }
            Some(p) => preview(bytes, p, opts),
            None => develop(bytes, opts).or_else(|e| any_preview(bytes, opts).ok_or(e)?),
        },
        RawMode::Develop => develop(bytes, opts).or_else(|e| match full {
            Some(p) => preview(bytes, p, opts),
            None => any_preview(bytes, opts).ok_or(e)?,
        }),
    }
}

/// Last resort when nothing can be developed: the largest preview, cropped or not.
fn any_preview(bytes: &[u8], opts: &DecodeOptions) -> Option<Result<RawPixels>> {
    craft_raw::embedded_preview(bytes).map(|p| preview(bytes, p, opts))
}

fn preview(bytes: &[u8], p: Preview<'_>, opts: &DecodeOptions) -> Result<RawPixels> {
    let image = jpeg(p.jpeg, opts.fit, &opts.limits)
        .map_err(|e| Error::Decode(Format::Raw, format!("embedded preview: {e}")))?;
    let icc = embedded(p.jpeg).icc.map(|c| c.into_owned());
    let adobe_rgb_hint = icc.is_none()
        && craft_raw::embedded_preview_color_space(bytes) == Some(PreviewColorSpace::AdobeRgb);
    Ok(RawPixels {
        image,
        icc,
        adobe_rgb_hint,
        upright: false,
    })
}

fn develop(bytes: &[u8], opts: &DecodeOptions) -> Result<RawPixels> {
    let err = |e: craft_raw::RawError| Error::Decode(Format::Raw, e.to_string());
    let limits = craft_raw::Limits {
        max_pixels: opts.limits.max_pixels,
        max_alloc: opts.limits.max_alloc,
        ..craft_raw::Limits::default()
    };
    let mut sensor = craft_raw::decode(bytes, &limits).map_err(err)?;
    let mut dev = DevelopOptions {
        limits,
        ..Default::default()
    };
    if sensor.color.calibrations.is_empty() && !sensor.is_monochrome() {
        dev.camera_look = fitted_look(bytes, &mut sensor, &dev, opts);
    }
    let d = craft_raw::develop_sensor(&sensor, &dev).map_err(err)?;
    opts.limits.check_size(d.width, d.height)?;
    let n = (d.width as usize) * (d.height as usize);
    if d.rgb.len() != n * 3 {
        return Err(Error::Decode(
            Format::Raw,
            "developed image has the wrong size".into(),
        ));
    }
    let mut out = Vec::with_capacity(n * 4);
    for p in d.rgb.as_chunks::<3>().0 {
        // 16 → 8 bits, rounded.
        out.extend(
            p.iter()
                .map(|&v| ((u32::from(v) * 255 + 32_767) / 65_535) as u8),
        );
        out.push(255);
    }
    let icc = Builtin::ProPhotoCompat
        .profile()
        .to_bytes()
        .as_ref()
        .clone();
    Ok(RawPixels {
        image: Rgba8Image::new(d.width, d.height, out)?,
        icc: Some(icc),
        adobe_rgb_hint: false,
        upright: true,
    })
}

/// A colour matrix and tone curve fitted to the camera's own JPEG (upright, sRGB), for a raw
/// that carries no colour calibration. `None` when there is no usable preview or the fit fails.
fn fitted_look(
    bytes: &[u8],
    sensor: &mut craft_raw::Sensor,
    dev: &DevelopOptions,
    opts: &DecodeOptions,
) -> Option<craft_raw::CameraLook> {
    let edge = 2 * craft_raw::PROXY_EDGE as u32;
    let previews = craft_raw::embedded_previews(bytes);
    let long = |p: &Preview| p.width.max(p.height);
    let pick = previews
        .iter()
        .filter(|p| long(p) >= edge)
        .min_by_key(|p| long(p))
        .or_else(|| previews.iter().max_by_key(|p| long(p)))?;
    let mut img = jpeg(pick.jpeg, None, &opts.limits).ok()?;
    let profile = embedded(pick.jpeg).icc.map(|c| c.into_owned());
    let source = match (&profile, craft_raw::embedded_preview_color_space(bytes)) {
        (Some(p), _) => Source::Embedded(p),
        (None, Some(PreviewColorSpace::AdobeRgb)) => Source::AdobeRgbHint,
        (None, _) => Source::None,
    };
    let _ = icc::to_srgb(&mut img, source);
    let orientation = craft_raw::orientation(bytes)
        .and_then(Orientation::from_exif)
        .unwrap_or_default();
    let img = orientation.apply(&img).ok()?;
    let rgb: Vec<u8> = img
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    // A preview cropped to another aspect ratio (Sony's 16:9 of a 3:2 sensor) is matched by
    // fitting against the same centred crop of the sensor; the full frame is still developed.
    let full = sensor.crop;
    sensor.crop = centred_crop(full, f64::from(pick.width) / f64::from(pick.height.max(1)));
    let reference = craft_raw::Reference {
        width: img.width(),
        height: img.height(),
        rgb: &rgb,
    };
    let look = craft_raw::fit_look(sensor, dev, &reference).ok();
    sensor.crop = full;
    look
}

/// The largest centred part of `r` with aspect ratio `aspect` (stored orientation), on even
/// coordinates so a colour filter pattern keeps its phase. `r` itself when the ratios agree.
fn centred_crop(r: craft_raw::Rect, aspect: f64) -> craft_raw::Rect {
    let current = r.width as f64 / r.height.max(1) as f64;
    if !aspect.is_finite() || aspect <= 0.0 || ((aspect / current) - 1.0).abs() < 0.03 {
        return r;
    }
    let even = |v: usize| v & !1;
    let (w, h) = if aspect > current {
        (r.width, even((r.width as f64 / aspect) as usize))
    } else {
        (even((r.height as f64 * aspect) as usize), r.height)
    };
    if w < 16 || h < 16 {
        return r;
    }
    craft_raw::Rect::new(
        r.x + even((r.width - w) / 2),
        r.y + even((r.height - h) / 2),
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_raw::Rect;

    #[test]
    fn centred_crop_matches_a_wider_or_taller_preview_on_even_coordinates() {
        let sensor = Rect::new(0, 0, 6048, 4024);
        // Sony's 16:9 preview of a 3:2 sensor: full width, a centred band.
        let band = centred_crop(sensor, 1920.0 / 1080.0);
        assert_eq!((band.x, band.width, band.height), (0, 6048, 3402));
        assert_eq!(band.y, 310);
        // A 4:3 thumbnail: full height, a centred column.
        let column = centred_crop(sensor, 4.0 / 3.0);
        assert_eq!((column.y, column.height), (0, 4024));
        assert_eq!(column.width % 2, 0);
        assert_eq!(column.x % 2, 0);
        // Same shape, or nonsense: unchanged.
        assert_eq!(centred_crop(sensor, 1.5), sensor);
        assert_eq!(centred_crop(sensor, f64::NAN), sensor);
        assert_eq!(centred_crop(sensor, 1e-9), sensor);
    }
}
