//! Pixel decoding to 8-bit RGBA, with the size checked against [`Limits`] before any pixel
//! buffer is allocated.

use crate::container::embedded;
use crate::exif::parse_exif;
use crate::icc::{self, ColorConversion};
use crate::{Error, Format, Limits, Orientation, Result, Rgba8Image, read_header, sniff};
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// How to decode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeOptions {
    /// Resource ceilings.
    pub limits: Limits,
    /// A size the caller will scale to. Decoders that can produce a smaller image cheaply
    /// (JPEG's DCT scaling) return the smallest one at least this large in both dimensions;
    /// others ignore it. In stored orientation.
    pub fit: Option<(u32, u32)>,
    /// How to read camera raws.
    pub raw: RawMode,
}

/// How [`decode`] turns a camera raw into pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RawMode {
    /// The camera's own embedded JPEG rendering (the largest one), which is what the photographer
    /// saw and is fast. Raws without a preview are developed.
    #[default]
    Preview,
    /// Develop the sensor data with `craft-raw` (demosaic, white balance, camera colour, tone).
    /// Slower; formats it cannot develop fall back to the embedded preview.
    Develop,
}

/// Decoded pixels in stored orientation and source colour space.
#[derive(Clone, Debug)]
pub struct Decoded {
    /// The pixels.
    pub image: Rgba8Image,
    /// The container format.
    pub format: Format,
    /// Full stored width (the image may be smaller after DCT scaling).
    pub source_width: u32,
    /// Full stored height.
    pub source_height: u32,
    /// How to turn the pixels upright. Not applied.
    pub orientation: Orientation,
    /// The embedded ICC profile. Not applied.
    pub icc: Option<Vec<u8>>,
    /// EXIF says Adobe RGB without a profile.
    pub adobe_rgb_hint: bool,
}

impl Decoded {
    /// Convert to sRGB and turn upright: the pixels as they should be shown.
    pub fn into_display(self) -> Result<(Rgba8Image, ColorConversion)> {
        let (img, conversion) = self.to_srgb();
        Ok((self.orientation.apply(&img)?, conversion))
    }

    /// The pixels converted to sRGB, in stored orientation.
    pub fn to_srgb(&self) -> (Rgba8Image, ColorConversion) {
        let mut img = self.image.clone();
        let c = self.convert_to_srgb(&mut img);
        (img, c)
    }

    /// Convert pixels derived from this decode (a resized copy, say) from its colour space to
    /// sRGB in place. Converting after downscaling is how thumbnails and renditions stay cheap.
    pub fn convert_to_srgb(&self, img: &mut Rgba8Image) -> ColorConversion {
        let source = match (&self.icc, self.adobe_rgb_hint) {
            (Some(p), _) => icc::Source::Embedded(p),
            (None, true) => icc::Source::AdobeRgbHint,
            (None, false) => icc::Source::None,
        };
        icc::to_srgb(img, source)
    }
}

/// Decode a file's pixels (the first frame of an animation, the merged composite of a PSD).
pub fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Decoded> {
    opts.limits.check_input(bytes.len())?;
    let format = sniff(bytes).ok_or(Error::UnknownFormat)?;
    if let Some(why) = format.not_decodable() {
        return Err(Error::Unsupported(format, why));
    }
    let header = read_header(bytes)?;
    opts.limits.check_size(header.width, header.height)?;
    let blocks = embedded(bytes);
    let exif = blocks.exif.as_deref().and_then(parse_exif);
    let mut icc = blocks.icc.map(|c| c.into_owned());

    let image = match format {
        Format::Jpeg => match opts.fit {
            Some((w, h)) if w < header.width || h < header.height => {
                guarded(format, || jpeg_scaled(bytes, w, h, &opts.limits))
                    .or_else(|_| full(bytes, format, &opts.limits))?
            }
            _ => full(bytes, format, &opts.limits)?,
        },
        Format::Heif | Format::Avif => {
            let (img, profile) = guarded(format, || {
                crate::heif::Heif::parse(bytes, format)?.decode(&opts.limits)
            })?;
            if icc.is_none() {
                icc = profile;
            }
            img
        }
        Format::Raw => return decode_raw(bytes, &header, opts),
        Format::Psd => {
            let (img, profile) = psd(bytes)?;
            if icc.is_none() {
                icc = profile;
            }
            img
        }
        Format::Jxl => {
            let (img, profile) = jxl(bytes, &opts.limits)?;
            if icc.is_none() {
                icc = profile;
            }
            img
        }
        Format::Dpx => crate::formats::dpx::decode(bytes, &opts.limits)?,
        Format::Pcx => crate::formats::pcx::decode(bytes, &opts.limits)?,
        Format::Fits => crate::formats::fits::decode(bytes, &opts.limits)?,
        Format::Dicom => crate::formats::dicom::decode(bytes, &opts.limits)?,
        Format::Exr | Format::Hdr => float(bytes, format, &opts.limits)?,
        _ => full(bytes, format, &opts.limits)?,
    };
    Ok(Decoded {
        image,
        format,
        source_width: header.width,
        source_height: header.height,
        orientation: header.orientation,
        adobe_rgb_hint: icc.is_none() && exif.as_ref().is_some_and(|x| x.adobe_rgb_hint()),
        icc,
    })
}

/// A camera raw (`raw` feature): its pixels are already upright when developed.
#[cfg(feature = "raw")]
fn decode_raw(bytes: &[u8], header: &crate::Header, opts: &DecodeOptions) -> Result<Decoded> {
    let r = guarded(Format::Raw, || {
        crate::raw::decode(bytes, header.width, header.height, opts)
    })?;
    // The raw's own container profile (rare) does not describe these pixels.
    Ok(Decoded {
        image: r.image,
        format: Format::Raw,
        source_width: header.width,
        source_height: header.height,
        orientation: if r.upright {
            Orientation::Identity
        } else {
            header.orientation
        },
        adobe_rgb_hint: r.adobe_rgb_hint,
        icc: r.icc,
    })
}

#[cfg(not(feature = "raw"))]
fn decode_raw(_: &[u8], _: &crate::Header, _: &DecodeOptions) -> Result<Decoded> {
    Err(Format::Raw.unsupported())
}

/// Run a third-party decoder, turning an escaped panic into an error. Our own code does not
/// panic; this guards the dependencies on targets that unwind.
pub(crate) fn guarded<T>(format: Format, f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(Error::Decode(format, "decoder panicked".into())))
}

fn image_format(format: Format) -> Option<image::ImageFormat> {
    Some(match format {
        Format::Jpeg => image::ImageFormat::Jpeg,
        Format::Png => image::ImageFormat::Png,
        Format::Gif => image::ImageFormat::Gif,
        Format::WebP => image::ImageFormat::WebP,
        Format::Tiff => image::ImageFormat::Tiff,
        Format::Bmp => image::ImageFormat::Bmp,
        Format::Exr => image::ImageFormat::OpenExr,
        Format::Hdr => image::ImageFormat::Hdr,
        Format::Dds => image::ImageFormat::Dds,
        Format::Ico => image::ImageFormat::Ico,
        Format::Pnm => image::ImageFormat::Pnm,
        Format::Qoi => image::ImageFormat::Qoi,
        Format::Tga => image::ImageFormat::Tga,
        _ => return None,
    })
}

/// Full-resolution decode through `image`.
fn full(bytes: &[u8], format: Format, limits: &Limits) -> Result<Rgba8Image> {
    let fmt =
        image_format(format).ok_or(Error::Unsupported(format, "no decoder for this format"))?;
    guarded(format, || {
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), fmt);
        let mut l = image::Limits::default();
        l.max_alloc = Some(limits.max_alloc);
        reader.limits(l);
        let img = reader
            .decode()
            .map_err(|e| Error::Decode(format, e.to_string()))?;
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        Rgba8Image::new(w, h, rgba.into_raw())
    })
}

/// Decode through `image`, keeping the samples as `f32`.
fn float_image(bytes: &[u8], format: Format, limits: &Limits) -> Result<image::Rgba32FImage> {
    let fmt =
        image_format(format).ok_or(Error::Unsupported(format, "no decoder for this format"))?;
    let header = read_header(bytes)?;
    // Four f32 samples per pixel are held at once, before the 8-bit copy.
    let bytes_needed = u64::from(header.width) * u64::from(header.height) * 16;
    if bytes_needed > limits.max_alloc {
        return Err(Error::TooLarge {
            width: header.width,
            height: header.height,
            max_pixels: limits.max_alloc / 16,
        });
    }
    guarded(format, || {
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), fmt);
        let mut l = image::Limits::default();
        l.max_alloc = Some(limits.max_alloc);
        reader.limits(l);
        let img = reader
            .decode()
            .map_err(|e| Error::Decode(format, e.to_string()))?;
        Ok(img.into_rgba32f())
    })
}

/// Floating-point formats (OpenEXR, Radiance HDR) to 8-bit sRGB.
///
/// This is a plain clamp, not a tone map: linear values are clamped to `[0, 1]` and encoded with
/// the sRGB transfer function (IEC 61966-2-1), so everything above 1.0 is white. Alpha is
/// clamped and scaled linearly. NaN becomes 0.
fn float(bytes: &[u8], format: Format, limits: &Limits) -> Result<Rgba8Image> {
    let img = float_image(bytes, format, limits)?;
    let (w, h) = img.dimensions();
    let mut out = Vec::with_capacity(img.as_raw().len());
    for p in img.as_raw().as_chunks::<4>().0 {
        out.extend_from_slice(&[
            srgb_oetf_u8(p[0]),
            srgb_oetf_u8(p[1]),
            srgb_oetf_u8(p[2]),
            unit_u8(p[3]),
        ]);
    }
    Rgba8Image::new(w, h, out)
}

/// Clamp to `[0, 1]` (NaN to 0) and scale to a byte.
fn unit_u8(v: f32) -> u8 {
    if v.is_nan() {
        return 0;
    }
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The sRGB opto-electronic transfer function on a linear value, clamped first.
fn srgb_oetf_u8(linear: f32) -> u8 {
    if linear.is_nan() {
        return 0;
    }
    let l = linear.clamp(0.0, 1.0);
    let e = if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    unit_u8(e)
}

#[cfg(all(feature = "jxl", not(target_arch = "wasm32")))]
fn jxl(bytes: &[u8], limits: &Limits) -> Result<(Rgba8Image, Option<Vec<u8>>)> {
    crate::formats::jxl::decode(bytes, limits)
}

#[cfg(not(all(feature = "jxl", not(target_arch = "wasm32"))))]
fn jxl(_bytes: &[u8], _limits: &Limits) -> Result<(Rgba8Image, Option<Vec<u8>>)> {
    Err(Format::Jxl.unsupported())
}

/// A JPEG stream, DCT-scaled toward `fit` when that is smaller.
#[cfg(feature = "raw")]
pub(crate) fn jpeg(bytes: &[u8], fit: Option<(u32, u32)>, limits: &Limits) -> Result<Rgba8Image> {
    let header = read_header(bytes)?;
    limits.check_size(header.width, header.height)?;
    match fit {
        Some((w, h)) if w < header.width || h < header.height => {
            guarded(Format::Jpeg, || jpeg_scaled(bytes, w, h, limits))
                .or_else(|_| full(bytes, Format::Jpeg, limits))
        }
        _ => full(bytes, Format::Jpeg, limits),
    }
}

/// DCT-domain downscaled JPEG decode (1/2, 1/4 or 1/8). Grayscale and RGB only; CMYK and
/// 16-bit lossless JPEGs fall back to [`full`].
#[cfg(feature = "jpeg")]
fn jpeg_scaled(bytes: &[u8], w: u32, h: u32, limits: &Limits) -> Result<Rgba8Image> {
    let err = |e: jpeg_decoder::Error| Error::Decode(Format::Jpeg, e.to_string());
    let mut d = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    d.set_max_decoding_buffer_size(usize::try_from(limits.max_alloc).unwrap_or(usize::MAX));
    d.read_info().map_err(err)?;
    let clamp = |v: u32| u16::try_from(v.max(1)).unwrap_or(u16::MAX);
    let (sw, sh) = d.scale(clamp(w), clamp(h)).map_err(err)?;
    let px = d.decode().map_err(err)?;
    let info = d
        .info()
        .ok_or_else(|| Error::Decode(Format::Jpeg, "no image info after decoding".into()))?;
    let n = usize::from(sw) * usize::from(sh);
    let mut out = Vec::with_capacity(n * 4);
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 if px.len() == n => {
            for &l in &px {
                out.extend_from_slice(&[l, l, l, 255]);
            }
        }
        jpeg_decoder::PixelFormat::RGB24 if px.len() == n * 3 => {
            for p in px.as_chunks::<3>().0.iter() {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        other => {
            return Err(Error::Decode(
                Format::Jpeg,
                format!("scaled decode does not handle {other:?}"),
            ));
        }
    }
    Rgba8Image::new(u32::from(sw), u32::from(sh), out)
}

#[cfg(not(feature = "jpeg"))]
fn jpeg_scaled(_: &[u8], _: u32, _: u32, _: &Limits) -> Result<Rgba8Image> {
    Err(Format::Jpeg.unsupported())
}

/// The merged composite of a PSD/PSB and its ICC profile, through `craft-psd`.
#[cfg(feature = "psd")]
fn psd(bytes: &[u8]) -> Result<(Rgba8Image, Option<Vec<u8>>)> {
    let file = craft_psd::PsdFile::from_bytes(bytes)
        .map_err(|e| Error::Decode(Format::Psd, e.to_string()))?;
    let c = file
        .composite_rgba8()
        .map_err(|e| Error::Decode(Format::Psd, e.to_string()))?;
    let img = Rgba8Image::new(c.width, c.height, c.data)?;
    Ok((img, file.icc_profile().map(<[u8]>::to_vec)))
}

#[cfg(not(feature = "psd"))]
fn psd(_: &[u8]) -> Result<(Rgba8Image, Option<Vec<u8>>)> {
    Err(Format::Psd.unsupported())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "png", feature = "jpeg"))]
    use crate::encode::{OutputFormat, encode_plain};

    #[cfg(any(feature = "png", feature = "jpeg"))]
    fn gradient(w: u32, h: u32) -> Rgba8Image {
        let mut d = Vec::new();
        for y in 0..h {
            for x in 0..w {
                d.extend_from_slice(&[
                    (x * 255 / w.max(2)) as u8,
                    (y * 255 / h.max(2)) as u8,
                    128,
                    255,
                ]);
            }
        }
        Rgba8Image::new(w, h, d).unwrap()
    }

    #[cfg(all(feature = "png", feature = "jpeg", feature = "webp"))]
    #[test]
    fn decodes_every_writable_format() {
        let img = gradient(40, 30);
        for f in [
            OutputFormat::Png,
            OutputFormat::Jpeg { quality: 95 },
            OutputFormat::WebpLossless,
        ] {
            let bytes = encode_plain(&img, f).unwrap();
            let d = decode(&bytes, &DecodeOptions::default()).unwrap();
            assert_eq!((d.image.width(), d.image.height()), (40, 30), "{f:?}");
            let p = d.image.pixel(20, 15).unwrap();
            let q = img.pixel(20, 15).unwrap();
            assert!(
                p.iter().zip(q).all(|(a, b)| a.abs_diff(b) <= 6),
                "{f:?}: {p:?} vs {q:?}"
            );
        }
    }

    /// A format whose feature is off is refused with `Unsupported`, never a panic, and its
    /// header is still readable.
    #[test]
    fn formats_built_out_are_unsupported_not_fatal() {
        let samples: [(&[u8], Format, bool); 4] = [
            (
                b"8BPS\0\x01\0\0\0\0\0\0",
                Format::Psd,
                cfg!(feature = "psd"),
            ),
            (
                b"qoif\0\0\0\x02\0\0\0\x02\x04\0",
                Format::Qoi,
                cfg!(feature = "qoi"),
            ),
            (
                b"GIF89a\x02\0\x02\0\0\0\0;",
                Format::Gif,
                cfg!(feature = "gif"),
            ),
            (b"v/1\x01\x02\0\0\0", Format::Exr, cfg!(feature = "exr")),
        ];
        for (bytes, format, built) in samples {
            assert_eq!(sniff(bytes), Some(format));
            assert_eq!(format.can_decode(), built, "{format}");
            if !built {
                assert!(
                    matches!(
                        decode(bytes, &DecodeOptions::default()),
                        Err(Error::Unsupported(f, _)) if f == format
                    ),
                    "{format}"
                );
            }
        }
    }

    #[test]
    fn srgb_encoding_of_linear_values() {
        assert_eq!(srgb_oetf_u8(0.0), 0);
        assert_eq!(srgb_oetf_u8(1.0), 255);
        assert_eq!(srgb_oetf_u8(7.5), 255, "above 1.0 clamps");
        assert_eq!(srgb_oetf_u8(-3.0), 0);
        assert_eq!(srgb_oetf_u8(f32::NAN), 0);
        // Linear 0.5 is 188 in sRGB; the linear toe is 12.92 × l.
        assert_eq!(srgb_oetf_u8(0.5), 188);
        assert_eq!(srgb_oetf_u8(0.002), 7);
        assert_eq!(unit_u8(0.5), 128);
    }

    #[cfg(feature = "jpeg")]
    #[test]
    fn jpeg_fit_uses_dct_scaling() {
        let bytes = encode_plain(&gradient(800, 600), OutputFormat::Jpeg { quality: 90 }).unwrap();
        let d = decode(
            &bytes,
            &DecodeOptions {
                fit: Some((100, 100)),
                ..Default::default()
            },
        )
        .unwrap();
        // 1/8 scale is the smallest that still covers 100×100: 100×75.
        assert_eq!((d.image.width(), d.image.height()), (100, 75));
        assert_eq!((d.source_width, d.source_height), (800, 600));
    }

    #[cfg(feature = "png")]
    #[test]
    fn limits_refuse_before_decoding() {
        let bytes = encode_plain(&gradient(64, 64), OutputFormat::Png).unwrap();
        let opts = DecodeOptions {
            limits: Limits {
                max_pixels: 1000,
                ..Limits::default()
            },
            ..DecodeOptions::default()
        };
        assert!(matches!(decode(&bytes, &opts), Err(Error::TooLarge { .. })));
        let opts = DecodeOptions {
            limits: Limits {
                max_input_bytes: 10,
                ..Limits::default()
            },
            ..DecodeOptions::default()
        };
        assert!(matches!(
            decode(&bytes, &opts),
            Err(Error::InputTooLarge { .. })
        ));
    }

    #[test]
    fn undecodable_formats_say_why() {
        // A bare JPEG XL signature with nothing behind it is damaged, not unsupported.
        assert!(decode(b"\xFF\x0A\xFA\x7F\0\0\0\0", &DecodeOptions::default()).is_err());
        // A HEIF with no items is malformed, not undecodable.
        assert!(
            decode(
                b"\0\0\0\x18ftypheic\0\0\0\0mif1heic",
                &DecodeOptions::default()
            )
            .is_err()
        );
        assert!(matches!(
            decode(b"garbage!", &DecodeOptions::default()),
            Err(Error::UnknownFormat)
        ));
    }

    #[cfg(feature = "psd")]
    #[test]
    fn psd_composites_through_craft_psd() {
        let mut b = craft_psd::PsdBuilder::new(2, 1);
        b.composite(craft_psd::PixelData::Rgba8(vec![
            255, 0, 0, 255, 0, 0, 255, 128,
        ]));
        let bytes = b.to_bytes().unwrap();
        let d = decode(&bytes, &DecodeOptions::default()).unwrap();
        assert_eq!(d.format, Format::Psd);
        assert_eq!(d.image.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(d.image.pixel(1, 0).map(|p| p[3]), Some(128));
    }
}
