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
    /// Lossless WebP (VP8L). Lossy WebP needs an encoder this build does not have.
    WebpLossless,
}

impl OutputFormat {
    /// The container format written.
    pub fn format(self) -> Format {
        match self {
            OutputFormat::Jpeg { .. } => Format::Jpeg,
            OutputFormat::Png => Format::Png,
            OutputFormat::WebpLossless => Format::WebP,
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
