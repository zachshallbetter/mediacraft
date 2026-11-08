//! FITS (Flexible Image Transport System, FITS Standard 4.0): the image in the primary HDU.
//!
//! * `BITPIX` 8, 16, 32, -32 and -64 (64-bit integers and tile-compressed files are refused),
//!   with `BSCALE`, `BZERO` and integer `BLANK`.
//! * `NAXIS` 2 (gray) or 3 with `NAXIS3` of 3 (planes are R, G, B) or 1 (gray).
//! * Rows are stored bottom-to-top, so they are flipped, unless `ROWORDER = 'TOP-DOWN'`.
//! * Tone mapping: a **linear min/max stretch** of the physical values (`raw * BSCALE + BZERO`)
//!   to 0..=255. The range is taken over all finite, non-blank samples of all planes together,
//!   so an RGB image keeps its colour balance. NaN and blank pixels become 0, and a constant
//!   image becomes black. PhotoCraft keeps the full 16-bit or float range instead; 8-bit output
//!   is this crate's only target, so the stretch is the one lossy step.
//!
//! Sizes are checked against [`Limits`] before any allocation, and the pixel data must be
//! present in the input, so the output can never be larger than four times the input.

use super::RawHeader;
use crate::{Error, Format, Limits, Result, Rgba8Image};

const F: Format = Format::Fits;
const BLOCK: usize = 2880;
const CARD: usize = 80;

fn bad(msg: &str) -> Error {
    Error::Malformed(F, msg.to_string())
}

/// The primary header, reduced to what decoding needs.
struct Hdr {
    bitpix: i64,
    width: u32,
    height: u32,
    planes: usize,
    bscale: f64,
    bzero: f64,
    blank: Option<i64>,
    top_down: bool,
    data_start: usize,
}

/// A FITS file starts with the `SIMPLE  =` card.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SIMPLE  =")
}

pub(crate) fn header(bytes: &[u8]) -> Option<RawHeader> {
    let h = parse_header(bytes).ok()?;
    Some(RawHeader {
        width: h.width,
        height: h.height,
        bit_depth: u8::try_from(h.bitpix.unsigned_abs()).ok(),
        has_alpha: Some(false),
        animated: false,
        orientation: None,
    })
}

/// The text of a card's value field up to the comment slash; quoted strings are unquoted.
fn value_text(card: &[u8]) -> Option<String> {
    if card.get(8..10) != Some(b"= ") {
        return None;
    }
    let field = String::from_utf8_lossy(card.get(10..)?).into_owned();
    let t = field.trim_start();
    if let Some(rest) = t.strip_prefix('\'') {
        return Some(rest.split('\'').next().unwrap_or("").trim_end().to_string());
    }
    Some(t.split('/').next().unwrap_or("").trim().to_string())
}

fn as_f64(text: &str) -> Option<f64> {
    // Fortran-style `D` exponents are legal in FITS.
    text.replace(['D', 'd'], "E")
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
}

fn as_i64(text: &str) -> Option<i64> {
    text.parse::<i64>().ok().or_else(|| {
        as_f64(text)
            .filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15)
            .map(|f| f as i64)
    })
}

fn parse_header(b: &[u8]) -> Result<Hdr> {
    let mut bitpix = None;
    let mut naxis = None;
    let mut axes: [Option<i64>; 3] = [None; 3];
    let (mut bscale, mut bzero, mut blank, mut roworder) = (None, None, None, None);
    let mut pos = 0usize;
    for index in 0usize.. {
        let card = pos
            .checked_add(CARD)
            .and_then(|end| b.get(pos..end))
            .ok_or_else(|| bad("header ends before END"))?;
        pos += CARD;
        let key = String::from_utf8_lossy(card.get(..8).unwrap_or(&[]))
            .trim_end()
            .to_string();
        if index == 0 && (key != "SIMPLE" || value_text(card).as_deref() != Some("T")) {
            return Err(bad("missing SIMPLE = T"));
        }
        if key == "END" {
            break;
        }
        let Some(text) = value_text(card) else {
            continue;
        };
        match key.as_str() {
            "BITPIX" if bitpix.is_none() => bitpix = as_i64(&text),
            "NAXIS" if naxis.is_none() => naxis = as_i64(&text),
            "NAXIS1" if axes[0].is_none() => axes[0] = as_i64(&text),
            "NAXIS2" if axes[1].is_none() => axes[1] = as_i64(&text),
            "NAXIS3" if axes[2].is_none() => axes[2] = as_i64(&text),
            "BSCALE" if bscale.is_none() => bscale = as_f64(&text),
            "BZERO" if bzero.is_none() => bzero = as_f64(&text),
            "BLANK" if blank.is_none() => blank = as_i64(&text),
            "ROWORDER" if roworder.is_none() => roworder = Some(text),
            _ => {}
        }
    }
    let data_start = pos
        .div_ceil(BLOCK)
        .checked_mul(BLOCK)
        .ok_or_else(|| bad("header too long"))?;

    let bitpix = bitpix.ok_or_else(|| bad("missing BITPIX"))?;
    match bitpix {
        8 | 16 | 32 | -32 | -64 => {}
        64 => {
            return Err(Error::Unsupported(F, "64-bit integer FITS (BITPIX 64)"));
        }
        _ => return Err(bad("invalid BITPIX")),
    }
    let naxis = naxis.ok_or_else(|| bad("missing NAXIS"))?;
    let planes = match naxis {
        0 => return Err(Error::Unsupported(F, "primary HDU holds no image")),
        2 => 1,
        3 => match axes[2] {
            Some(1) => 1,
            Some(3) => 3,
            Some(_) => return Err(Error::Unsupported(F, "FITS cubes other than 3 planes")),
            None => return Err(bad("missing NAXIS3")),
        },
        _ => return Err(Error::Unsupported(F, "FITS images with more than 3 axes")),
    };
    let dim = |a: Option<i64>| {
        a.and_then(|v| u32::try_from(v).ok())
            .filter(|&v| v > 0)
            .ok_or_else(|| bad("missing or empty NAXIS1/NAXIS2"))
    };
    Ok(Hdr {
        bitpix,
        width: dim(axes[0])?,
        height: dim(axes[1])?,
        planes,
        bscale: bscale.unwrap_or(1.0),
        bzero: bzero.unwrap_or(0.0),
        blank: if bitpix > 0 { blank } else { None },
        top_down: roworder.is_some_and(|r| r.eq_ignore_ascii_case("TOP-DOWN")),
        data_start,
    })
}

fn arr<const N: usize>(s: &[u8]) -> Option<[u8; N]> {
    s.try_into().ok()
}

impl Hdr {
    /// Physical value of sample `i` (planar order); `None` for a blank, NaN or missing sample.
    fn physical(&self, data: &[u8], i: usize) -> Option<f64> {
        let bps = self.bitpix.unsigned_abs() as usize / 8;
        let s = data.get(i.checked_mul(bps)?..i.checked_add(1)?.checked_mul(bps)?)?;
        let raw: i64 = match self.bitpix {
            8 => i64::from(*s.first()?),
            16 => i64::from(i16::from_be_bytes(arr(s)?)),
            32 => i64::from(i32::from_be_bytes(arr(s)?)),
            -32 => {
                let v = f64::from(f32::from_be_bytes(arr(s)?));
                return Some(v * self.bscale + self.bzero).filter(|v| v.is_finite());
            }
            _ => {
                let v = f64::from_be_bytes(arr(s)?);
                return Some(v * self.bscale + self.bzero).filter(|v| v.is_finite());
            }
        };
        if self.blank == Some(raw) {
            return None;
        }
        Some(raw as f64 * self.bscale + self.bzero).filter(|v| v.is_finite())
    }
}

pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Rgba8Image> {
    limits.check_input(bytes.len())?;
    let h = parse_header(bytes)?;
    limits.check_size(h.width, h.height)?;
    let (w, ht) = (h.width as usize, h.height as usize);
    let npix = w.checked_mul(ht).ok_or_else(|| bad("image too large"))?;
    let total = npix
        .checked_mul(h.planes)
        .ok_or_else(|| bad("image too large"))?;
    let need = total
        .checked_mul(h.bitpix.unsigned_abs() as usize / 8)
        .and_then(|n| n.checked_add(h.data_start))
        .ok_or_else(|| bad("data size overflows"))?;
    let data = bytes
        .get(h.data_start..need)
        .ok_or_else(|| bad("truncated pixel data"))?;
    let out_len = npix.checked_mul(4).ok_or_else(|| bad("image too large"))?;
    if out_len as u64 > limits.max_alloc {
        return Err(Error::TooLarge {
            width: h.width,
            height: h.height,
            max_pixels: limits.max_pixels,
        });
    }

    // Linear stretch bounds over every usable sample.
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..total {
        if let Some(v) = h.physical(data, i) {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    let range = hi - lo;
    let level = |i: usize| -> u8 {
        match h.physical(data, i) {
            Some(v) if range > 0.0 => ((v - lo) / range * 255.0).round().clamp(0.0, 255.0) as u8,
            _ => 0,
        }
    };

    let mut out = Vec::new();
    out.try_reserve_exact(out_len)
        .map_err(|_| Error::InvalidArgument("cannot allocate the image".into()))?;
    for y in 0..ht {
        // FITS stores the bottom row first.
        let src_y = if h.top_down { y } else { ht - 1 - y };
        for x in 0..w {
            let i = src_y * w + x;
            if h.planes == 3 {
                out.extend_from_slice(&[level(i), level(npix + i), level(2 * npix + i), 255]);
            } else {
                let g = level(i);
                out.extend_from_slice(&[g, g, g, 255]);
            }
        }
    }
    Rgba8Image::new(h.width, h.height, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(text: &str) -> Vec<u8> {
        let mut c = text.as_bytes().to_vec();
        c.resize(CARD, b' ');
        c
    }

    /// A minimal FITS writer: header cards, END, padding to a block, then the data.
    fn file(bitpix: i64, axes: &[u32], extra: &[&str], data: &[u8]) -> Vec<u8> {
        let mut out = card("SIMPLE  =                    T");
        out.extend(card(&format!("BITPIX  = {bitpix:>20}")));
        out.extend(card(&format!("NAXIS   = {:>20}", axes.len())));
        for (i, a) in axes.iter().enumerate() {
            out.extend(card(&format!("NAXIS{:<3}= {a:>20}", i + 1)));
        }
        for e in extra {
            out.extend(card(e));
        }
        out.extend(card("END"));
        out.resize(out.len().div_ceil(BLOCK) * BLOCK, b' ');
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(BLOCK) * BLOCK, 0);
        out
    }

    fn px(img: &Rgba8Image, x: u32, y: u32) -> [u8; 4] {
        img.pixel(x, y).unwrap()
    }

    #[test]
    fn sniff_and_header() {
        let f = file(16, &[3, 2], &[], &[0; 12]);
        assert!(sniff(&f));
        assert!(!sniff(b"GIF89a"));
        let h = header(&f).unwrap();
        assert_eq!((h.width, h.height, h.bit_depth), (3, 2, Some(16)));
        assert_eq!(h.has_alpha, Some(false));
        assert!(header(&f[..100]).is_none());
        assert!(header(b"").is_none());
    }

    #[test]
    fn eight_bit_is_flipped_and_stretched() {
        // Stored bottom row first: bottom = [0, 85], top = [170, 255].
        let f = file(8, &[2, 2], &[], &[0, 85, 170, 255]);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!((img.width(), img.height()), (2, 2));
        assert_eq!(px(&img, 0, 0), [170, 170, 170, 255]);
        assert_eq!(px(&img, 1, 0), [255, 255, 255, 255]);
        assert_eq!(px(&img, 0, 1), [0, 0, 0, 255]);
        assert_eq!(px(&img, 1, 1), [85, 85, 85, 255]);
    }

    #[test]
    fn roworder_top_down_is_not_flipped() {
        let f = file(8, &[1, 2], &["ROWORDER= 'TOP-DOWN'"], &[0, 255]);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0)[0], 0);
        assert_eq!(px(&img, 0, 1)[0], 255);
    }

    #[test]
    fn bzero_and_bscale_apply_before_the_stretch() {
        // The unsigned-16 convention: raw -32768, 0, 32767 are 0, 32768, 65535.
        let raw: Vec<u8> = [-32768i16, 0, 32767]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        let f = file(16, &[3, 1], &["BZERO   =                32768"], &raw);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0)[0], 0);
        assert_eq!(px(&img, 1, 0)[0], 128);
        assert_eq!(px(&img, 2, 0)[0], 255);
        // A negative scale reverses the ramp.
        let f = file(
            16,
            &[2, 1],
            &["BSCALE  =                 -1.0"],
            &[0, 0, 0, 10],
        );
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!((px(&img, 0, 0)[0], px(&img, 1, 0)[0]), (255, 0));
    }

    #[test]
    fn thirty_two_bit_integers_and_floats() {
        let ints: Vec<u8> = [100i32, 300].iter().flat_map(|v| v.to_be_bytes()).collect();
        let img = decode(&file(32, &[2, 1], &[], &ints), &Limits::default()).unwrap();
        assert_eq!((px(&img, 0, 0)[0], px(&img, 1, 0)[0]), (0, 255));

        let f32s: Vec<u8> = [0.5f32, f32::NAN, 1.5]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        let img = decode(&file(-32, &[3, 1], &[], &f32s), &Limits::default()).unwrap();
        assert_eq!(
            (px(&img, 0, 0)[0], px(&img, 1, 0)[0], px(&img, 2, 0)[0]),
            (0, 0, 255)
        );

        let f64s: Vec<u8> = [-1.0f64, 0.0, 1.0]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        let img = decode(&file(-64, &[3, 1], &[], &f64s), &Limits::default()).unwrap();
        assert_eq!(
            (px(&img, 0, 0)[0], px(&img, 1, 0)[0], px(&img, 2, 0)[0]),
            (0, 128, 255)
        );
    }

    #[test]
    fn blank_pixels_are_black_and_excluded_from_the_range() {
        let raw: Vec<u8> = [-1i16, 10, 20]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        let f = file(16, &[3, 1], &["BLANK   =                   -1"], &raw);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(
            (px(&img, 0, 0)[0], px(&img, 1, 0)[0], px(&img, 2, 0)[0]),
            (0, 0, 255)
        );
    }

    #[test]
    fn three_planes_become_rgb() {
        // w=2, h=1: R = [255, 0], G = [0, 255], B = [0, 0].
        let f = file(8, &[2, 1, 3], &[], &[255, 0, 0, 255, 0, 0]);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
        assert_eq!(px(&img, 1, 0), [0, 255, 0, 255]);
        // Planes are flipped with the rows: two rows, red on the stored bottom row.
        let f = file(8, &[1, 2, 3], &[], &[255, 0, 0, 0, 0, 0]);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 1), [255, 0, 0, 255]);
        assert_eq!(px(&img, 0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn constant_image_is_black() {
        let img = decode(&file(8, &[2, 1], &[], &[7, 7]), &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn unsupported_and_malformed_inputs_are_errors() {
        let l = Limits::default();
        assert!(matches!(
            decode(&file(64, &[1, 1], &[], &[0; 8]), &l),
            Err(Error::Unsupported(..))
        ));
        assert!(matches!(
            decode(&file(8, &[], &[], &[]), &l),
            Err(Error::Unsupported(..))
        ));
        assert!(matches!(
            decode(&file(8, &[1, 1, 4], &[], &[0; 4]), &l),
            Err(Error::Unsupported(..))
        ));
        assert!(matches!(
            decode(&file(7, &[1, 1], &[], &[0]), &l),
            Err(Error::Malformed(..))
        ));
        assert!(matches!(
            decode(&file(8, &[0, 4], &[], &[]), &l),
            Err(Error::Malformed(..))
        ));
        // Header says 4 pixels, data holds none.
        let mut f = file(8, &[2, 2], &[], &[]);
        f.truncate(BLOCK);
        assert!(matches!(decode(&f, &l), Err(Error::Malformed(..))));
    }

    #[test]
    fn limits_apply_before_allocation() {
        // 100000 x 100000 with no data at all: refused on size, not on the missing data.
        let f = file(8, &[100_000, 100_000], &[], &[]);
        assert!(matches!(
            decode(&f, &Limits::default()),
            Err(Error::TooLarge { .. })
        ));
        let small = Limits {
            max_pixels: 3,
            ..Limits::default()
        };
        assert!(matches!(
            decode(&file(8, &[2, 2], &[], &[0; 4]), &small),
            Err(Error::TooLarge { .. })
        ));
        let tiny_input = Limits {
            max_input_bytes: 10,
            ..Limits::default()
        };
        assert!(matches!(
            decode(&file(8, &[1, 1], &[], &[0]), &tiny_input),
            Err(Error::InputTooLarge { .. })
        ));
        let tiny_alloc = Limits {
            max_alloc: 8,
            ..Limits::default()
        };
        assert!(matches!(
            decode(&file(8, &[2, 2], &[], &[0; 4]), &tiny_alloc),
            Err(Error::TooLarge { .. })
        ));
    }

    #[test]
    fn every_prefix_and_byte_flip_is_handled() {
        let raw: Vec<u8> = [1i16, 2, 3, 4, 5, 6]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        let f = file(16, &[2, 1, 3], &["BZERO   =                  1.5"], &raw);
        let l = Limits::default();
        assert!(decode(&f, &l).is_ok());
        for n in 0..f.len() {
            let _ = decode(&f[..n], &l);
            let _ = header(&f[..n]);
            let _ = sniff(&f[..n]);
        }
        // The header block and the first data bytes.
        for i in 0..f.len().min(BLOCK + 16) {
            for flip in [0x01u8, 0x80, 0xFF] {
                let mut g = f.clone();
                g[i] ^= flip;
                let _ = decode(&g, &l);
                let _ = header(&g);
            }
        }
    }
}
