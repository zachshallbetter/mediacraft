//! Planar Y′CbCr (and G/B/R) to 8-bit RGBA (ITU-T H.273 matrices, limited or full range, any chroma
//! subsampling the decoders produce, 8- to 16-bit samples).

use crate::Rgba8Image;

/// Luma coefficients (Kr, Kb) of the matrices the decoders signal.
fn coefficients(matrix: u8, height: u32) -> (f32, f32) {
    match matrix {
        1 => (0.2126, 0.0722),      // BT.709
        4 => (0.30, 0.11),          // FCC
        5 | 6 => (0.299, 0.114),    // BT.601 (625 / 525)
        7 => (0.212, 0.087),        // SMPTE 240M
        9 | 10 => (0.2627, 0.0593), // BT.2020
        // Unspecified: what players assume — BT.709 for HD, BT.601 for SD.
        _ if height >= 720 => (0.2126, 0.0722),
        _ => (0.299, 0.114),
    }
}

/// One plane: samples, stride in samples, and its size.
pub struct Plane<'a, T> {
    /// Samples.
    pub data: &'a [T],
    /// Samples per row.
    pub stride: usize,
    /// Plane width.
    pub width: u32,
    /// Plane height.
    pub height: u32,
}

/// Convert planes to RGBA. `T` is `u8` or `u16`; `bits` is the significant bit depth.
pub fn to_rgba<T: Copy + Into<u32>>(
    y: Plane<'_, T>,
    cb: Plane<'_, T>,
    cr: Plane<'_, T>,
    alpha: Option<Plane<'_, T>>,
    bits: u32,
    matrix: u8,
    full_range: bool,
) -> Option<Rgba8Image> {
    let (w, h) = (y.width, y.height);
    if w == 0 || h == 0 || cb.width == 0 || cb.height == 0 || bits == 0 || bits > 16 {
        return None;
    }
    // Chroma subsampling from the plane sizes: 2 when the chroma plane is half size (rounded up).
    let sx = if cb.width < w { 2 } else { 1 };
    let sy = if cb.height < h { 2 } else { 1 };
    let (kr, kb) = coefficients(matrix, h);
    let kg = 1.0 - kr - kb;
    let max = ((1u32 << bits) - 1) as f32;
    let shift = bits.saturating_sub(8);
    let (y_off, y_scale, c_scale) = if full_range {
        (0.0, max, max)
    } else {
        (
            (16u32 << shift) as f32,
            (219u32 << shift) as f32,
            (224u32 << shift) as f32,
        )
    };
    let c_mid = (128u32 << shift) as f32;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h as usize {
        let crow = (row / sy).min(cb.height as usize - 1);
        for col in 0..w as usize {
            let ccol = (col / sx).min(cb.width as usize - 1);
            let yv = (*y.data.get(row * y.stride + col)?).into() as f32;
            let u = (*cb.data.get(crow * cb.stride + ccol)?).into() as f32;
            let v = (*cr.data.get(crow * cr.stride + ccol)?).into() as f32;
            let luma = (yv - y_off) / y_scale;
            let pb = (u - c_mid) / c_scale;
            let pr = (v - c_mid) / c_scale;
            let r = luma + 2.0 * (1.0 - kr) * pr;
            let b = luma + 2.0 * (1.0 - kb) * pb;
            let g = (luma - kr * r - kb * b) / kg;
            let a = match &alpha {
                Some(p) => {
                    let s = (*p.data.get(row * p.stride + col)?).into() as f32;
                    (s / max * 255.0 + 0.5) as u8
                }
                None => 255,
            };
            out.extend_from_slice(&[to8(r), to8(g), to8(b), a]);
        }
    }
    Rgba8Image::new(w, h, out).ok()
}

/// Planes that hold G, B and R (DNx RGB, VP9 sRGB, AV1 with the identity matrix).
pub fn rgb_to_rgba<T: Copy + Into<u32>>(
    g: Plane<'_, T>,
    b: Plane<'_, T>,
    r: Plane<'_, T>,
    alpha: Option<Plane<'_, T>>,
    bits: u32,
    full_range: bool,
) -> Option<Rgba8Image> {
    let (w, h) = (g.width, g.height);
    if w == 0
        || h == 0
        || bits == 0
        || bits > 16
        || (b.width, b.height, r.width, r.height) != (w, h, w, h)
    {
        return None;
    }
    let max = ((1u32 << bits) - 1) as f32;
    let shift = bits.saturating_sub(8);
    let (off, scale) = if full_range {
        (0.0, max)
    } else {
        ((16u32 << shift) as f32, (219u32 << shift) as f32)
    };
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h as usize {
        for col in 0..w as usize {
            let at = |p: &Plane<'_, T>| {
                p.data
                    .get(row * p.stride + col)
                    .map(|v| ((*v).into() as f32 - off) / scale)
            };
            let a = match &alpha {
                Some(p) => {
                    ((*p.data.get(row * p.stride + col)?).into() as f32 / max * 255.0 + 0.5) as u8
                }
                None => 255,
            };
            out.extend_from_slice(&[to8(at(&r)?), to8(at(&g)?), to8(at(&b)?), a]);
        }
    }
    Rgba8Image::new(w, h, out).ok()
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane<T>(data: &[T], width: u32, height: u32) -> Plane<'_, T> {
        Plane {
            data,
            stride: width as usize,
            width,
            height,
        }
    }

    #[test]
    fn limited_range_black_white_and_bt709_red() {
        // 2×1 luma, 1×1 chroma (4:2:0-like): video black and white with neutral chroma.
        let img = to_rgba(
            plane(&[16u8, 235], 2, 1),
            plane(&[128u8], 1, 1),
            plane(&[128u8], 1, 1),
            None,
            8,
            1,
            false,
        )
        .unwrap();
        assert_eq!(img.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(img.pixel(1, 0), Some([255, 255, 255, 255]));
        // BT.709 limited-range red is Y'=63 Cb=102 Cr=240.
        let red = to_rgba(
            plane(&[63u8], 1, 1),
            plane(&[102u8], 1, 1),
            plane(&[240u8], 1, 1),
            None,
            8,
            1,
            false,
        )
        .unwrap();
        let p = red.pixel(0, 0).unwrap();
        assert!(p[0] >= 250 && p[1] <= 4 && p[2] <= 4, "{p:?}");
    }

    #[test]
    fn ten_bit_full_range_and_alpha() {
        let max = 1023u16;
        let img = to_rgba(
            plane(&[max, 0], 2, 1),
            plane(&[512u16, 512], 2, 1),
            plane(&[512u16, 512], 2, 1),
            Some(plane(&[max, 0], 2, 1)),
            10,
            9,
            true,
        )
        .unwrap();
        assert_eq!(img.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(img.pixel(1, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn short_planes_are_refused() {
        assert!(
            to_rgba(
                plane(&[16u8], 2, 1),
                plane(&[128u8], 1, 1),
                plane(&[128u8], 1, 1),
                None,
                8,
                1,
                false
            )
            .is_none()
        );
        assert!(
            to_rgba(
                plane::<u8>(&[], 0, 0),
                plane(&[128u8], 1, 1),
                plane(&[128u8], 1, 1),
                None,
                8,
                1,
                false
            )
            .is_none()
        );
    }
}
