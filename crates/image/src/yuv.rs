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
    to_rgba_display(y, cb, cr, alpha, bits, matrix, full_range, Display::SDR)
}

/// [`to_rgba`] for pictures in any transfer and primaries: HDR (PQ, HLG) is tone mapped and
/// wide gamut (BT.2020) converted, giving sRGB.
#[allow(clippy::too_many_arguments)]
pub fn to_rgba_display<T: Copy + Into<u32>>(
    y: Plane<'_, T>,
    cb: Plane<'_, T>,
    cr: Plane<'_, T>,
    alpha: Option<Plane<'_, T>>,
    bits: u32,
    matrix: u8,
    full_range: bool,
    display: Display,
) -> Option<Rgba8Image> {
    let map = Mapper::new(display);
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
            let [r, g, b] = map.map([r, g, b]);
            out.extend_from_slice(&[r, g, b, a]);
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
    rgb_to_rgba_display(g, b, r, alpha, bits, full_range, Display::SDR)
}

/// [`rgb_to_rgba`] for pictures in any transfer and primaries (see [`to_rgba_display`]).
pub fn rgb_to_rgba_display<T: Copy + Into<u32>>(
    g: Plane<'_, T>,
    b: Plane<'_, T>,
    r: Plane<'_, T>,
    alpha: Option<Plane<'_, T>>,
    bits: u32,
    full_range: bool,
    display: Display,
) -> Option<Rgba8Image> {
    let map = Mapper::new(display);
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
            let [r, g, b] = map.map([at(&r)?, at(&g)?, at(&b)?]);
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    Rgba8Image::new(w, h, out).ok()
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// How a picture's R′G′B′ values are meant to be shown (ITU-T H.273 code points).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display {
    /// Colour primaries (1 BT.709, 9 BT.2020, 2 unspecified, …).
    pub primaries: u8,
    /// Transfer characteristics (1 BT.709, 16 PQ, 18 HLG, 2 unspecified, …).
    pub transfer: u8,
}

impl Display {
    /// Standard-dynamic-range BT.709: the values are shown as they are.
    pub const SDR: Display = Display {
        primaries: 1,
        transfer: 1,
    };

    /// PQ (SMPTE ST 2084) or HLG (ARIB STD-B67).
    pub fn is_hdr(self) -> bool {
        matches!(self.transfer, 16 | 18)
    }
}

/// HDR assumptions: the mastering display's peak (unknown from the bitstream alone; 1000 nits
/// is the common grade and HLG's reference) and the HDR reference white (BT.2408).
const PEAK_NITS: f32 = 1000.0;
const REFERENCE_WHITE_NITS: f32 = 203.0;
/// SDR level where the highlight roll-off starts; reference white lands near 0.91.
const KNEE: f32 = 0.75;
/// Entries of the lookup tables.
const LUT: usize = 4096;

/// BT.2020 to BT.709 primaries, linear light (both D65).
const BT2020_TO_709: [[f32; 3]; 3] = [
    [1.660_491, -0.587_641_1, -0.072_849_86],
    [-0.124_550_5, 1.132_899_9, -0.008_349_42],
    [-0.018_150_76, -0.100_578_9, 1.118_729_6],
];

/// Turns R′G′B′ in the picture's transfer and primaries into 8-bit sRGB.
enum Mapper {
    /// Shown as is.
    Direct,
    /// Wide-gamut or HDR: to linear light, BT.709 primaries, tone mapped, sRGB encoded.
    Linear {
        /// Non-linear value (0..1, `LUT` steps) to linear light, relative to SDR white.
        decode: Vec<f32>,
        gamut: Option<[[f32; 3]; 3]>,
        /// HLG's OOTF: display luminance from scene luminance (BT.2100, system gamma 1.2).
        hlg: bool,
        tone_map: bool,
        /// Linear 0..1 (`LUT` steps) to the sRGB code value.
        encode: Vec<u8>,
    },
}

impl Mapper {
    fn new(d: Display) -> Self {
        let wide = d.primaries == 9;
        if !d.is_hdr() && !wide {
            return Mapper::Direct;
        }
        let decode = (0..=LUT)
            .map(|i| {
                let e = i as f32 / LUT as f32;
                match d.transfer {
                    16 => pq_eotf(e) / REFERENCE_WHITE_NITS,
                    18 => hlg_inverse_oetf(e),
                    // SDR BT.2020 (and BT.709-like curves): BT.1886's gamma 2.4.
                    _ => e.powf(2.4),
                }
            })
            .collect();
        let encode = (0..=LUT)
            .map(|i| {
                let l = i as f32 / LUT as f32;
                let v = if l <= 0.003_130_8 {
                    12.92 * l
                } else {
                    1.055 * l.powf(1.0 / 2.4) - 0.055
                };
                to8(v)
            })
            .collect();
        Mapper::Linear {
            decode,
            gamut: wide.then_some(BT2020_TO_709),
            hlg: d.transfer == 18,
            tone_map: d.is_hdr(),
            encode,
        }
    }

    fn map(&self, rgb: [f32; 3]) -> [u8; 3] {
        let Mapper::Linear {
            decode,
            gamut,
            hlg,
            tone_map,
            encode,
        } = self
        else {
            return rgb.map(to8);
        };
        let lookup = |v: f32| {
            let x = v.clamp(0.0, 1.0) * LUT as f32;
            let i = (x as usize).min(LUT - 1);
            let f = x - i as f32;
            let (a, b) = (
                decode.get(i).copied().unwrap_or(0.0),
                decode.get(i + 1).copied().unwrap_or(0.0),
            );
            a + (b - a) * f
        };
        let mut lin = rgb.map(lookup);
        if *hlg {
            // Scene light (0..1) to display light at a 1000-nit peak, relative to SDR white.
            let ys = 0.2627 * lin[0] + 0.6780 * lin[1] + 0.0593 * lin[2];
            let gain = ys.max(1e-6).powf(0.2) * PEAK_NITS / REFERENCE_WHITE_NITS;
            lin = lin.map(|v| v * gain);
        }
        if let Some(m) = gamut {
            lin = [0, 1, 2].map(|r| m[r][0] * lin[0] + m[r][1] * lin[1] + m[r][2] * lin[2]);
        }
        if *tone_map {
            lin = roll_off(lin);
        }
        lin.map(|v| {
            let x = v.clamp(0.0, 1.0) * LUT as f32;
            encode.get(x as usize).copied().unwrap_or(255)
        })
    }
}

/// Compress highlights above [`KNEE`] into the remaining headroom (C¹ at the knee). The curve
/// acts on the largest channel and scales all three by the same factor, as SMPTE ST 2094-40 does,
/// so hue and saturation are kept and no channel leaves 0..1.
fn roll_off(rgb: [f32; 3]) -> [f32; 3] {
    let rgb = rgb.map(|v| v.max(0.0));
    let m = rgb[0].max(rgb[1]).max(rgb[2]);
    if m <= KNEE {
        return rgb;
    }
    let mapped = KNEE + (1.0 - KNEE) * (1.0 - (-(m - KNEE) / (1.0 - KNEE)).exp());
    rgb.map(|v| v * mapped / m)
}

/// SMPTE ST 2084 EOTF: non-linear 0..1 to nits.
fn pq_eotf(e: f32) -> f32 {
    const M1: f32 = 2610.0 / 16384.0;
    const M2: f32 = 2523.0 / 4096.0 * 128.0;
    const C1: f32 = 3424.0 / 4096.0;
    const C2: f32 = 2413.0 / 4096.0 * 32.0;
    const C3: f32 = 2392.0 / 4096.0 * 32.0;
    let p = e.max(0.0).powf(1.0 / M2);
    ((p - C1).max(0.0) / (C2 - C3 * p)).powf(1.0 / M1) * 10_000.0
}

/// ARIB STD-B67 (HLG) inverse OETF: non-linear 0..1 to scene light 0..1.
fn hlg_inverse_oetf(e: f32) -> f32 {
    const A: f32 = 0.178_832_77;
    const B: f32 = 0.284_668_92;
    const C: f32 = 0.559_910_7;
    if e <= 0.5 {
        e * e / 3.0
    } else {
        (((e - C) / A).exp() + B) / 12.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full-range grey (equal R′G′B′) at `v` (0..1) through an HDR display.
    fn grey(v: f32, transfer: u8) -> u8 {
        Mapper::new(Display {
            primaries: 9,
            transfer,
        })
        .map([v; 3])[0]
    }

    #[test]
    fn pq_and_hlg_reference_white_land_just_below_sdr_white() {
        // 203 nits is PQ 0.5806 and HLG 0.75 (BT.2408).
        for (v, t) in [(0.5806, 16), (0.75, 18)] {
            let w = grey(v, t);
            assert!((236..=250).contains(&w), "transfer {t}: {w}");
        }
        assert_eq!(grey(0.0, 16), 0);
        assert_eq!(grey(0.0, 18), 0);
        // 1000 nits (PQ 0.7518) and HLG's peak reach (nearly) white without wrapping.
        assert!(grey(0.7518, 16) >= 253);
        assert!(grey(1.0, 18) >= 253);
        assert_eq!(grey(1.0, 16), 255);
        // Monotone.
        let mut last = 0;
        for i in 0..=100 {
            let g = grey(i as f32 / 100.0, 16);
            assert!(g >= last, "{i}");
            last = g;
        }
    }

    #[test]
    fn saturated_bt2020_highlights_stay_in_gamut_with_their_hue() {
        let m = Mapper::new(Display {
            primaries: 9,
            transfer: 16,
        });
        // A bright, fully saturated BT.2020 red: red stays dominant, nothing wraps.
        let [r, g, b] = m.map([0.75, 0.0, 0.0]);
        assert!(r > 200 && g < 60 && b < 60, "{r} {g} {b}");
        // SDR BT.709 is unchanged by the display path.
        assert_eq!(
            Mapper::new(Display::SDR).map([0.5, 0.25, 1.0]),
            [128, 64, 255]
        );
    }

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
