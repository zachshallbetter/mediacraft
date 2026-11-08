//! Resampling. Pixels are converted to linear light with premultiplied alpha first: resizing
//! gamma-encoded values darkens fine detail, and resizing straight alpha bleeds the colour of
//! transparent pixels into edges. Big reductions are prefiltered with an exact box average
//! before the final filter.

use crate::{Error, Result, Rgba8Image};
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};

/// Resampling filter for the final pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    /// Lanczos, radius 3: sharpest, for photos.
    #[default]
    Lanczos3,
    /// Catmull-Rom cubic.
    CatmullRom,
    /// Bilinear.
    Triangle,
    /// Nearest neighbour: pixel art, masks.
    Nearest,
}

impl Filter {
    fn image(self) -> FilterType {
        match self {
            Filter::Lanczos3 => FilterType::Lanczos3,
            Filter::CatmullRom => FilterType::CatmullRom,
            Filter::Triangle => FilterType::Triangle,
            Filter::Nearest => FilterType::Nearest,
        }
    }
}

/// How to resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResizeOptions {
    /// Final-pass filter.
    pub filter: Filter,
    /// Resample in linear light (default). Off resamples the sRGB-encoded values, which is
    /// faster and matches most browsers' scaling.
    pub linear_light: bool,
}

impl Default for ResizeOptions {
    fn default() -> Self {
        Self {
            filter: Filter::Lanczos3,
            linear_light: true,
        }
    }
}

/// The largest size within `max_w × max_h` that keeps the aspect ratio, never larger than the
/// source and never zero.
pub fn fit_within(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width <= max_w && height <= max_h {
        return (width.max(1), height.max(1));
    }
    let scale = (f64::from(max_w) / f64::from(width)).min(f64::from(max_h) / f64::from(height));
    let w = (f64::from(width) * scale)
        .round()
        .clamp(1.0, f64::from(max_w.max(1)));
    let h = (f64::from(height) * scale)
        .round()
        .clamp(1.0, f64::from(max_h.max(1)));
    (w as u32, h as u32)
}

/// Resize to exactly `width × height`.
pub fn resize(
    img: &Rgba8Image,
    width: u32,
    height: u32,
    opts: &ResizeOptions,
) -> Result<Rgba8Image> {
    if width == 0 || height == 0 {
        return Err(Error::InvalidArgument(format!(
            "cannot resize to {width}×{height}"
        )));
    }
    if (width, height) == (img.width(), img.height()) {
        return Ok(img.clone());
    }
    let lut = DecodeLut::new(opts.linear_light);
    // Box prefilter: a whole-number reduction straight from the 8-bit source leaves at most 2×
    // for the final filter, so the float buffer is never larger than about 4× the output.
    let k = (img.width() / width).min(img.height() / height) / 2;
    let (buf, w, h) = if k >= 2 {
        box_reduce(img, k, &lut)
    } else {
        (to_premultiplied(img, &lut), img.width(), img.height())
    };
    let src = image::ImageBuffer::<image::Rgba<f32>, Vec<f32>>::from_raw(w, h, buf)
        .ok_or_else(|| Error::InvalidArgument("resize buffer has the wrong length".into()))?;
    let out = image::imageops::resize(&src, width, height, opts.filter.image());
    from_premultiplied(out.as_raw(), width, height, opts.linear_light)
}

/// Resize to fit within `max_w × max_h`, keeping the aspect ratio and never enlarging.
pub fn resize_to_fit(
    img: &Rgba8Image,
    max_w: u32,
    max_h: u32,
    opts: &ResizeOptions,
) -> Result<Rgba8Image> {
    let (w, h) = fit_within(img.width(), img.height(), max_w, max_h);
    resize(img, w, h, opts)
}

/// sRGB-encoded byte → linear (or plain normalised) value.
struct DecodeLut([f32; 256]);

impl DecodeLut {
    fn new(linear: bool) -> Self {
        let mut t = [0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if !linear {
                c
            } else if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
        }
        Self(t)
    }
}

fn encode_srgb(l: f32) -> f32 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

fn to_premultiplied(img: &Rgba8Image, lut: &DecodeLut) -> Vec<f32> {
    let mut out = Vec::with_capacity(img.data().len());
    for p in img.data().as_chunks::<4>().0.iter() {
        let a = f32::from(p[3]) / 255.0;
        out.extend_from_slice(&[
            lut.0[usize::from(p[0])] * a,
            lut.0[usize::from(p[1])] * a,
            lut.0[usize::from(p[2])] * a,
            a,
        ]);
    }
    out
}

fn from_premultiplied(buf: &[f32], width: u32, height: u32, linear: bool) -> Result<Rgba8Image> {
    let mut out = Vec::with_capacity(buf.len());
    for p in buf.as_chunks::<4>().0.iter() {
        let a = p[3].clamp(0.0, 1.0);
        if a <= 1.0 / 512.0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for &c in &p[..3] {
            let v = (c / a).clamp(0.0, 1.0);
            let e = if linear { encode_srgb(v) } else { v };
            out.push((e * 255.0 + 0.5) as u8);
        }
        out.push((a * 255.0 + 0.5) as u8);
    }
    Rgba8Image::new(width, height, out)
}

/// Average `k × k` blocks of the 8-bit source into premultiplied (linear) floats, one source row
/// at a time. Partial blocks at the right and bottom edges average what they have.
fn box_reduce(img: &Rgba8Image, k: u32, lut: &DecodeLut) -> (Vec<f32>, u32, u32) {
    let (w, h, k) = (img.width() as usize, img.height() as usize, k as usize);
    let (nw, nh) = (w.div_ceil(k), h.div_ceil(k));
    let mut out = vec![0f32; nw * nh * 4];
    let src = img.data();
    for ny in 0..nh {
        let y0 = ny * k;
        let y1 = (y0 + k).min(h);
        let acc = &mut out[ny * nw * 4..(ny + 1) * nw * 4];
        for y in y0..y1 {
            let Some(row) = src.get(y * w * 4..(y + 1) * w * 4) else {
                continue;
            };
            for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
                let o = (x / k) * 4;
                let a = f32::from(p[3]) / 255.0;
                acc[o] += lut.0[usize::from(p[0])] * a;
                acc[o + 1] += lut.0[usize::from(p[1])] * a;
                acc[o + 2] += lut.0[usize::from(p[2])] * a;
                acc[o + 3] += a;
            }
        }
        let rows = (y1 - y0) as f32;
        for nx in 0..nw {
            let cols = ((nx * k + k).min(w) - nx * k) as f32;
            let n = rows * cols;
            for v in &mut acc[nx * 4..nx * 4 + 4] {
                *v /= n;
            }
        }
    }
    (out, nw as u32, nh as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_never_enlarges() {
        assert_eq!(fit_within(4000, 3000, 1280, 1280), (1280, 960));
        assert_eq!(fit_within(3000, 4000, 1280, 1280), (960, 1280));
        assert_eq!(fit_within(100, 50, 1280, 1280), (100, 50));
        assert_eq!(fit_within(10_000, 1, 100, 100), (100, 1));
        assert_eq!(fit_within(1, 10_000, 100, 100), (1, 100));
    }

    #[test]
    fn linear_light_keeps_the_average_brightness_of_fine_detail() {
        // A 1-pixel black/white checkerboard averages to 50% *light*, which is sRGB ≈ 188, not 128.
        let mut d = Vec::new();
        for y in 0..64u32 {
            for x in 0..64u32 {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                d.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let img = Rgba8Image::new(64, 64, d).unwrap();
        let linear = resize(&img, 8, 8, &ResizeOptions::default()).unwrap();
        let gamma = resize(
            &img,
            8,
            8,
            &ResizeOptions {
                linear_light: false,
                ..Default::default()
            },
        )
        .unwrap();
        let mid = |i: &Rgba8Image| i.pixel(4, 4).unwrap()[0];
        assert!(mid(&linear).abs_diff(188) <= 3, "{}", mid(&linear));
        assert!(mid(&gamma).abs_diff(128) <= 3, "{}", mid(&gamma));
    }

    #[test]
    fn transparent_pixels_do_not_bleed_into_edges() {
        // Left half opaque red, right half fully transparent *green*. The green must not show.
        let mut d = Vec::new();
        for _y in 0..16 {
            for x in 0..16 {
                d.extend_from_slice(if x < 8 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 255, 0, 0]
                });
            }
        }
        let img = Rgba8Image::new(16, 16, d).unwrap();
        let out = resize(&img, 5, 5, &ResizeOptions::default()).unwrap();
        for p in out.data().as_chunks::<4>().0.iter().filter(|p| p[3] > 0) {
            assert!(p[1] <= 2, "green leaked: {p:?}");
        }
    }

    #[test]
    fn box_prefilter_handles_ragged_edges() {
        let img = Rgba8Image::filled(1001, 777, [90, 160, 30, 255]).unwrap();
        let out = resize_to_fit(&img, 64, 64, &ResizeOptions::default()).unwrap();
        assert_eq!((out.width(), out.height()), (64, 50));
        for p in out.data().as_chunks::<4>().0.iter() {
            assert!(
                p[0].abs_diff(90) <= 1
                    && p[1].abs_diff(160) <= 1
                    && p[2].abs_diff(30) <= 1
                    && p[3] == 255,
                "{p:?}"
            );
        }
        assert!(resize(&img, 0, 10, &ResizeOptions::default()).is_err());
    }
}
