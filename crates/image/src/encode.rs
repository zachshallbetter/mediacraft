//! Encoding 8-bit RGBA to delivery formats, with optional ICC and EXIF blocks.

use crate::formats::jpeg::segment;
use crate::{Error, Format, Result, Rgba8Image};
use image::ImageEncoder;
use serde::{Deserialize, Serialize};

/// Delivery formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "format")]
pub enum OutputFormat {
    /// Baseline JPEG, 4:2:0, quality 1–100. Alpha is flattened onto the background.
    Jpeg {
        /// 1–100.
        quality: u8,
    },
    /// PNG, RGB when every pixel is opaque, RGBA otherwise.
    Png,
    /// Lossy WebP: one VP8 key frame (`craft-vp8enc`), quality 1–100. Translucent pixels keep
    /// their alpha in an uncompressed `ALPH` chunk; ICC and EXIF are carried as chunks.
    Webp {
        /// 1–100.
        quality: u8,
    },
    /// Lossless WebP (VP8L).
    WebpLossless,
}

impl OutputFormat {
    /// The container format written.
    pub fn format(self) -> Format {
        match self {
            OutputFormat::Jpeg { .. } => Format::Jpeg,
            OutputFormat::Png => Format::Png,
            OutputFormat::Webp { .. } | OutputFormat::WebpLossless => Format::WebP,
        }
    }
}

/// Blocks to embed and how to flatten alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodeOptions {
    /// ICC profile to embed (JPEG APP2, PNG iCCP, WebP ICCP).
    pub icc: Option<Vec<u8>>,
    /// EXIF TIFF structure to embed (JPEG APP1, PNG eXIf). WebP output does not carry it.
    pub exif: Option<Vec<u8>>,
    /// Colour transparent pixels are composited onto for JPEG.
    pub background: [u8; 3],
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            icc: None,
            exif: None,
            background: [255, 255, 255],
        }
    }
}

/// Encode with no metadata.
pub fn encode_plain(img: &Rgba8Image, format: OutputFormat) -> Result<Vec<u8>> {
    encode(img, format, &EncodeOptions::default())
}

/// Encode an image.
pub fn encode(img: &Rgba8Image, format: OutputFormat, opts: &EncodeOptions) -> Result<Vec<u8>> {
    let f = format.format();
    let err = |e: image::ImageError| Error::Encode(f, e.to_string());
    let (w, h) = (img.width(), img.height());
    let mut out = Vec::new();
    match format {
        OutputFormat::Jpeg { quality } => {
            let rgb = flatten(img, opts.background);
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
            if let Some(icc) = &opts.icc {
                enc.set_icc_profile(icc.clone())
                    .map_err(|e| Error::Encode(f, e.to_string()))?;
            }
            enc.write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
                .map_err(err)?;
            if let Some(exif) = &opts.exif {
                out = insert_jpeg_exif(out, exif)?;
            }
        }
        OutputFormat::Png => {
            let mut enc = image::codecs::png::PngEncoder::new_with_quality(
                &mut out,
                image::codecs::png::CompressionType::Default,
                image::codecs::png::FilterType::Adaptive,
            );
            if let Some(icc) = &opts.icc {
                enc.set_icc_profile(icc.clone())
                    .map_err(|e| Error::Encode(f, e.to_string()))?;
            }
            if img.is_opaque() {
                enc.write_image(
                    &flatten(img, [0, 0, 0]),
                    w,
                    h,
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(err)?;
            } else {
                enc.write_image(img.data(), w, h, image::ExtendedColorType::Rgba8)
                    .map_err(err)?;
            }
            if let Some(exif) = &opts.exif {
                out = insert_png_chunk(out, b"eXIf", exif)?;
            }
        }
        OutputFormat::Webp { quality } => {
            out = encode_webp_lossy(img, quality, opts)?;
        }
        OutputFormat::WebpLossless => {
            let mut enc = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
            if let Some(icc) = &opts.icc {
                enc.set_icc_profile(icc.clone())
                    .map_err(|e| Error::Encode(f, e.to_string()))?;
            }
            enc.write_image(img.data(), w, h, image::ExtendedColorType::Rgba8)
                .map_err(err)?;
        }
    }
    Ok(out)
}

/// Lossy WebP (RFC 9649): the colour as a VP8 key frame and, when any pixel is translucent, the
/// alpha plane as an uncompressed `ALPH` chunk. The extended (`VP8X`) form is written whenever
/// there is alpha, ICC or EXIF, with chunks in the order the specification gives.
fn encode_webp_lossy(img: &Rgba8Image, quality: u8, opts: &EncodeOptions) -> Result<Vec<u8>> {
    let f = Format::WebP;
    let (w, h) = (img.width(), img.height());
    if w > craft_vp8enc::MAX_DIMENSION || h > craft_vp8enc::MAX_DIMENSION {
        return Err(Error::Encode(
            f,
            format!(
                "lossy WebP holds at most {} pixels per side",
                craft_vp8enc::MAX_DIMENSION
            ),
        ));
    }
    // Colour as stored (not flattened): alpha travels separately. `from_rgb` takes the bytes
    // per pixel (4 here), not a row stride.
    let yuv = craft_vp8enc::Yuv::from_rgb(img.data(), w, h, 4);
    let frame = craft_vp8enc::encode_frame(
        &yuv,
        craft_vp8enc::Params {
            quality: quality.clamp(1, 100),
        },
    )
    .map_err(|e| Error::Encode(f, e.to_string()))?;
    let alpha = (!img.is_opaque()).then(|| {
        // ALPH header: no preprocessing, no filter, no compression; then one byte per pixel.
        let mut a = Vec::with_capacity(w as usize * h as usize + 1);
        a.push(0);
        a.extend(img.data().as_chunks::<4>().0.iter().map(|p| p[3]));
        a
    });
    let chunk = |out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]| -> Result<()> {
        let len = u32::try_from(payload.len())
            .map_err(|_| Error::Encode(f, "WebP chunk over 4 GiB".into()))?;
        out.extend_from_slice(fourcc);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
        Ok(())
    };
    let mut body = b"WEBP".to_vec();
    if alpha.is_some() || opts.icc.is_some() || opts.exif.is_some() {
        let flags = (u8::from(opts.icc.is_some()) << 5)
            | (u8::from(alpha.is_some()) << 4)
            | (u8::from(opts.exif.is_some()) << 3);
        let mut x = vec![flags, 0, 0, 0];
        x.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
        x.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
        chunk(&mut body, b"VP8X", &x)?;
        if let Some(icc) = &opts.icc {
            chunk(&mut body, b"ICCP", icc)?;
        }
        if let Some(a) = &alpha {
            chunk(&mut body, b"ALPH", a)?;
        }
    }
    chunk(&mut body, b"VP8 ", &frame)?;
    if let Some(exif) = &opts.exif {
        chunk(&mut body, b"EXIF", exif)?;
    }
    let size =
        u32::try_from(body.len()).map_err(|_| Error::Encode(f, "WebP file over 4 GiB".into()))?;
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// RGB bytes with alpha composited onto `bg`.
fn flatten(img: &Rgba8Image, bg: [u8; 3]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(img.data().len() / 4 * 3);
    for p in img.data().as_chunks::<4>().0.iter() {
        let a = u32::from(p[3]);
        for c in 0..3 {
            let v = (u32::from(p[c]) * a + u32::from(bg[c]) * (255 - a) + 127) / 255;
            rgb.push(v as u8);
        }
    }
    rgb
}

/// Put an `Exif\0\0` APP1 after SOI and any APP0 (JFIF wants APP0 first).
fn insert_jpeg_exif(jpeg: Vec<u8>, exif: &[u8]) -> Result<Vec<u8>> {
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(exif);
    let seg = segment(0xE1, &payload)
        .ok_or_else(|| Error::Encode(Format::Jpeg, "EXIF block is over 64 KiB".into()))?;
    let mut at = 2;
    if jpeg.get(2..4) == Some(&[0xFF, 0xE0]) {
        let len = crate::bytes::be_u16(&jpeg, 4).map_or(0, usize::from);
        at = 4 + len;
    }
    if at > jpeg.len() {
        return Err(Error::Encode(
            Format::Jpeg,
            "encoder output has no room for EXIF".into(),
        ));
    }
    let mut out = Vec::with_capacity(jpeg.len() + seg.len());
    out.extend_from_slice(&jpeg[..at]);
    out.extend_from_slice(&seg);
    out.extend_from_slice(&jpeg[at..]);
    Ok(out)
}

/// Put a chunk before the first IDAT.
fn insert_png_chunk(png: Vec<u8>, ty: &[u8; 4], data: &[u8]) -> Result<Vec<u8>> {
    let mut p = 8usize;
    let at = loop {
        let Some(len) = crate::bytes::be_u32(&png, p) else {
            return Err(Error::Encode(
                Format::Png,
                "encoder output has no IDAT".into(),
            ));
        };
        if png.get(p + 4..p + 8) == Some(b"IDAT") {
            break p;
        }
        p += 12 + len as usize;
    };
    let len = u32::try_from(data.len())
        .map_err(|_| Error::Encode(Format::Png, "chunk too large".into()))?;
    let mut chunk = len.to_be_bytes().to_vec();
    chunk.extend_from_slice(ty);
    chunk.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(ty);
    crc.update(data);
    chunk.extend_from_slice(&crc.sum().to_be_bytes());
    let mut out = Vec::with_capacity(png.len() + chunk.len());
    out.extend_from_slice(&png[..at]);
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[at..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Orientation, embedded, exif::minimal_exif, parse_exif, read_header};

    #[test]
    fn jpeg_and_png_carry_icc_and_exif_that_we_read_back() {
        let img = Rgba8Image::filled(16, 8, [200, 100, 50, 255]).unwrap();
        let icc = crate::icc::srgb_profile();
        let exif = minimal_exif(Orientation::Identity, Some("Ada"), Some("(c) Ada"));
        let opts = EncodeOptions {
            icc: Some(icc.clone()),
            exif: Some(exif),
            ..Default::default()
        };
        for f in [OutputFormat::Jpeg { quality: 85 }, OutputFormat::Png] {
            let bytes = encode(&img, f, &opts).unwrap();
            let h = read_header(&bytes).unwrap();
            assert_eq!((h.format, h.width, h.height), (f.format(), 16, 8));
            let e = embedded(&bytes);
            assert_eq!(e.icc.as_deref(), Some(icc.as_slice()), "{f:?}");
            let x = parse_exif(e.exif.as_deref().unwrap()).unwrap();
            assert_eq!(x.artist.as_deref(), Some("Ada"), "{f:?}");
        }
    }

    #[test]
    fn jpeg_flattens_alpha_onto_the_background() {
        let img = Rgba8Image::new(1, 1, vec![0, 0, 0, 0]).unwrap();
        assert_eq!(flatten(&img, [255, 255, 255]), vec![255, 255, 255]);
        let img = Rgba8Image::new(1, 1, vec![0, 0, 0, 128]).unwrap();
        assert_eq!(flatten(&img, [255, 255, 255]), vec![127, 127, 127]);
    }

    #[test]
    fn png_writes_rgb_when_opaque_and_rgba_when_not() {
        let opaque = encode_plain(
            &Rgba8Image::filled(4, 4, [1, 2, 3, 255]).unwrap(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!(read_header(&opaque).unwrap().has_alpha, Some(false));
        let clear = encode_plain(
            &Rgba8Image::filled(4, 4, [1, 2, 3, 9]).unwrap(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!(read_header(&clear).unwrap().has_alpha, Some(true));
    }
}

#[cfg(test)]
mod webp_tests {
    use super::*;
    use crate::{
        DecodeOptions, Orientation, decode, embedded, exif::minimal_exif, parse_exif, read_header,
    };

    fn gradient(w: u32, h: u32, alpha: impl Fn(u32) -> u8) -> Rgba8Image {
        let mut d = Vec::new();
        for y in 0..h {
            for x in 0..w {
                d.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 140, alpha(x)]);
            }
        }
        Rgba8Image::new(w, h, d).unwrap()
    }

    #[test]
    fn lossy_webp_round_trips_through_the_webp_decoder() {
        let img = gradient(96, 64, |_| 255);
        let bytes = encode_plain(&img, OutputFormat::Webp { quality: 85 }).unwrap();
        let h = read_header(&bytes).unwrap();
        assert_eq!(
            (h.format, h.width, h.height, h.has_alpha),
            (Format::WebP, 96, 64, Some(false))
        );
        let back = decode(&bytes, &DecodeOptions::default()).unwrap().image;
        let diff: u64 = img
            .data()
            .iter()
            .zip(back.data())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = diff as f64 / img.data().len() as f64;
        assert!(mean < 4.0, "mean error {mean}");
        // Smaller than lossless on textured, photo-like content (flat synthetic gradients are the
        // one place lossless wins).
        let mut seed = 12345u32;
        let mut noisy = img.clone();
        for v in noisy.data_mut().iter_mut().filter(|v| **v != 255) {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            *v = v.saturating_add((seed >> 27) as u8);
        }
        let lossy = encode_plain(&noisy, OutputFormat::Webp { quality: 85 }).unwrap();
        let lossless = encode_plain(&noisy, OutputFormat::WebpLossless).unwrap();
        assert!(
            lossy.len() < lossless.len(),
            "{} vs {}",
            lossy.len(),
            lossless.len()
        );
    }

    #[test]
    fn lossy_webp_keeps_alpha_icc_and_exif() {
        let img = gradient(40, 30, |x| if x < 20 { 255 } else { 77 });
        let icc = crate::icc::srgb_profile();
        let exif = minimal_exif(Orientation::Identity, Some("Ada"), None);
        let opts = EncodeOptions {
            icc: Some(icc.clone()),
            exif: Some(exif),
            ..Default::default()
        };
        let bytes = encode(&img, OutputFormat::Webp { quality: 70 }, &opts).unwrap();
        assert_eq!(read_header(&bytes).unwrap().has_alpha, Some(true));
        let back = decode(&bytes, &DecodeOptions::default()).unwrap().image;
        assert_eq!(back.pixel(5, 5).unwrap()[3], 255);
        assert_eq!(
            back.pixel(35, 5).unwrap()[3],
            77,
            "alpha is stored losslessly"
        );
        let e = embedded(&bytes);
        assert_eq!(e.icc.as_deref(), Some(icc.as_slice()));
        assert_eq!(
            parse_exif(e.exif.as_deref().unwrap())
                .unwrap()
                .artist
                .as_deref(),
            Some("Ada")
        );
    }

    #[test]
    fn lossy_webp_quality_trades_size() {
        let img = gradient(128, 128, |_| 255);
        let low = encode_plain(&img, OutputFormat::Webp { quality: 20 }).unwrap();
        let high = encode_plain(&img, OutputFormat::Webp { quality: 95 }).unwrap();
        assert!(low.len() < high.len(), "{} vs {}", low.len(), high.len());
    }
}
