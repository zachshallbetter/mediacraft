//! DPX (SMPTE 268M Digital Picture Exchange) reader, ported from PhotoCraft's codec.
//!
//! Supported: both byte orders (`SDPX` big-endian, `XPDS` little-endian); the first image
//! element only, unsigned and not run-length encoded, with descriptor 6 (luma), 50 (RGB) or 51
//! (RGBA) at 8, 10, 12 or 16 bits. 10-bit samples are bit-packed (packing 0) or three to a
//! 32-bit word with the padding at the bottom (1, method A) or the top (2, method B); 12-bit
//! samples are bit-packed (0) or one to a 16-bit word in its top bits (1, method A). Lines start
//! on a 32-bit word boundary (some writers leave 8- and 16-bit lines tight; that is accepted).
//!
//! Samples map to 8 bits as `round(c × 255 / (2^bits − 1))`. Luma becomes grey with opaque
//! alpha; RGB gets opaque alpha.
//!
//! Colour: the transfer characteristic and colorimetric fields are ignored, so the code values
//! are treated as display-referred. A log-encoded (Cineon-style) scan therefore comes out flat
//! and a linear one dark; converting those is a grading decision this decoder does not make.
//!
//! Orientation is not applied to the pixels; [`header`] reports it (mapped to the EXIF values)
//! for the pipeline to apply, like the other containers.

use super::RawHeader;
use crate::{Error, Format, Limits, Orientation, Result, Rgba8Image};

/// Offset of the orientation field, followed by the element count.
const ORIENTATION: usize = 768;
/// Offset of the first image element descriptor.
const ELEMENT0: usize = 780;
const UNDEF32: u32 = u32::MAX;
const MAX_ELEMENTS: u16 = 8;

fn bad(msg: impl std::fmt::Display) -> Error {
    Error::Malformed(Format::Dpx, msg.to_string())
}

fn unsupported(msg: impl std::fmt::Display) -> Error {
    Error::Decode(Format::Dpx, msg.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Big,
    Little,
}

impl Order {
    fn u16(self, b: &[u8], at: usize) -> Option<u16> {
        let s: [u8; 2] = b.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(match self {
            Order::Big => u16::from_be_bytes(s),
            Order::Little => u16::from_le_bytes(s),
        })
    }

    fn u32(self, b: &[u8], at: usize) -> Option<u32> {
        let s: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(match self {
            Order::Big => u32::from_be_bytes(s),
            Order::Little => u32::from_le_bytes(s),
        })
    }
}

fn order(b: &[u8]) -> Option<Order> {
    match b.get(..4)? {
        b"SDPX" => Some(Order::Big),
        b"XPDS" => Some(Order::Little),
        _ => None,
    }
}

/// Whether the bytes start with a DPX magic number.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    order(bytes).is_some()
}

struct Parsed {
    order: Order,
    width: u32,
    height: u32,
    signed: bool,
    descriptor: u8,
    bits: u8,
    packing: u16,
    encoding: u16,
    data_offset: usize,
    eol_padding: usize,
    /// DPX orientation code, 0 to 7.
    orientation: u16,
}

fn parse(b: &[u8]) -> Result<Parsed> {
    let o = order(b).ok_or_else(|| bad("missing SDPX/XPDS magic number"))?;
    let cut = || bad("the header is cut short");
    let image_offset = o.u32(b, 4).ok_or_else(cut)?;
    let orientation = o.u16(b, ORIENTATION).ok_or_else(cut)?;
    let elements = o.u16(b, ORIENTATION + 2).ok_or_else(cut)?;
    if !(1..=MAX_ELEMENTS).contains(&elements) {
        return Err(bad(format!("invalid number of image elements {elements}")));
    }
    let width = o.u32(b, 772).ok_or_else(cut)?;
    let height = o.u32(b, 776).ok_or_else(cut)?;
    if width == 0 || height == 0 || width == UNDEF32 || height == UNDEF32 {
        return Err(bad("zero or undefined image size"));
    }
    let e = ELEMENT0;
    let data_offset = match o.u32(b, e + 28).ok_or_else(cut)? {
        0 | UNDEF32 => image_offset,
        v => v,
    };
    let eol_padding = match o.u32(b, e + 32).ok_or_else(cut)? {
        UNDEF32 => 0,
        v => v,
    };
    Ok(Parsed {
        order: o,
        width,
        height,
        signed: o.u32(b, e).ok_or_else(cut)? == 1,
        descriptor: *b.get(e + 20).ok_or_else(cut)?,
        bits: *b.get(e + 23).ok_or_else(cut)?,
        packing: o.u16(b, e + 24).ok_or_else(cut)?,
        encoding: o.u16(b, e + 26).ok_or_else(cut)?,
        data_offset: usize::try_from(data_offset)
            .map_err(|_| bad("image data offset out of range"))?,
        eol_padding: usize::try_from(eol_padding).map_err(|_| bad("line padding out of range"))?,
        orientation,
    })
}

/// Size, bits per sample, alpha and orientation from the headers alone.
pub(crate) fn header(bytes: &[u8]) -> Option<RawHeader> {
    let p = parse(bytes).ok()?;
    // DPX orientation codes 0 to 7 in SMPTE order, as EXIF values.
    let exif = match p.orientation {
        1 => 2,
        2 => 4,
        3 => 3,
        4 => 5,
        5 => 6,
        6 => 8,
        7 => 7,
        _ => 1,
    };
    Some(RawHeader {
        width: p.width,
        height: p.height,
        bit_depth: Some(p.bits),
        has_alpha: Some(p.descriptor == 51),
        animated: false,
        orientation: Orientation::from_exif(exif),
    })
}

/// Bytes in one stored line of `samples` samples, padded to a 32-bit word.
fn line_bytes(samples: usize, bits: u8, packing: u16) -> Option<usize> {
    let words = match (bits, packing) {
        (10, 1 | 2) => samples.div_ceil(3),
        (12, 1) => samples.div_ceil(2),
        _ => samples.checked_mul(usize::from(bits))?.div_ceil(32),
    };
    words.checked_mul(4)
}

/// Code values of one stored line.
fn unpack_line(line: &[u8], o: Order, bits: u8, packing: u16, out: &mut [u16]) -> Result<()> {
    let short = || bad("truncated image data");
    match (bits, packing) {
        (8, _) => {
            let src = line.get(..out.len()).ok_or_else(short)?;
            for (v, &c) in out.iter_mut().zip(src) {
                *v = u16::from(c);
            }
        }
        (16, _) => {
            for (i, v) in out.iter_mut().enumerate() {
                *v = o.u16(line, i * 2).ok_or_else(short)?;
            }
        }
        (10, 1 | 2) => {
            let base: u32 = if packing == 1 { 22 } else { 20 };
            for (i, v) in out.iter_mut().enumerate() {
                let word = o.u32(line, i / 3 * 4).ok_or_else(short)?;
                *v = ((word >> (base - 10 * (i % 3) as u32)) & 0x3FF) as u16;
            }
        }
        (12, 1) => {
            for (i, v) in out.iter_mut().enumerate() {
                *v = (o.u16(line, i * 2).ok_or_else(short)? >> 4) & 0xFFF;
            }
        }
        // Packing 0: one bit stream through the line's 32-bit words, high bits first.
        _ => {
            let mask = (1u64 << bits) - 1;
            let (mut acc, mut have, mut word) = (0u64, 0u32, 0usize);
            for v in out.iter_mut() {
                while have < u32::from(bits) {
                    acc = (acc << 32) | u64::from(o.u32(line, word * 4).ok_or_else(short)?);
                    word += 1;
                    have += 32;
                }
                have -= u32::from(bits);
                *v = ((acc >> have) & mask) as u16;
            }
        }
    }
    Ok(())
}

/// `round(c × 255 / max)`.
fn to_u8(c: u16, max: u32) -> u8 {
    ((u32::from(c) * 510 + max) / (2 * max)).min(255) as u8
}

/// Decode element 0 to 8-bit RGBA, in the stored line order.
pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Rgba8Image> {
    limits.check_input(bytes.len())?;
    let p = parse(bytes)?;
    let channels = match p.descriptor {
        6 => 1usize,
        50 => 3,
        51 => 4,
        d => {
            return Err(unsupported(format!(
                "descriptor {d} (only 6 luma, 50 RGB and 51 RGBA)"
            )));
        }
    };
    if p.signed {
        return Err(unsupported("signed samples"));
    }
    if p.encoding != 0 {
        return Err(unsupported("run-length encoded data"));
    }
    let packing_ok = match p.bits {
        8 | 16 => p.packing == 0,
        10 => p.packing <= 2,
        12 => p.packing <= 1,
        b => return Err(unsupported(format!("bit size {b} (only 8, 10, 12 and 16)"))),
    };
    if !packing_ok {
        return Err(unsupported(format!(
            "packing {} at {} bits",
            p.packing, p.bits
        )));
    }
    limits.check_size(p.width, p.height)?;
    let (w, h) = (p.width as usize, p.height as usize);
    let too_large = || Error::TooLarge {
        width: p.width,
        height: p.height,
        max_pixels: limits.max_pixels,
    };
    let rgba_len = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(too_large)?;
    if rgba_len as u64 > limits.max_alloc {
        return Err(too_large());
    }

    let per_line = w.checked_mul(channels).ok_or_else(too_large)?;
    let data = bytes
        .get(p.data_offset..)
        .ok_or_else(|| bad("the image data offset is past the end of the file"))?;
    let mut line_len = line_bytes(per_line, p.bits, p.packing).ok_or_else(too_large)?;
    let mut stride = line_len.checked_add(p.eol_padding).ok_or_else(too_large)?;
    let span = |stride: usize, line_len: usize| stride.checked_mul(h - 1)?.checked_add(line_len);
    // Some writers do not pad 8- and 16-bit lines to a word; take that layout when the file is
    // too short for padded lines.
    let tight = per_line * usize::from(p.bits / 8);
    if matches!(p.bits, 8 | 16)
        && p.eol_padding == 0
        && line_len != tight
        && span(stride, line_len).is_none_or(|need| data.len() < need)
    {
        (line_len, stride) = (tight, tight);
    }
    if span(stride, line_len).ok_or_else(too_large)? > data.len() {
        return Err(bad("truncated image data"));
    }

    let max = (1u32 << p.bits) - 1;
    let mut out = Vec::new();
    out.try_reserve_exact(rgba_len).map_err(|_| too_large())?;
    let mut row = vec![0u16; per_line];
    for y in 0..h {
        let line = data
            .get(y * stride..)
            .ok_or_else(|| bad("truncated image data"))?;
        unpack_line(line, p.order, p.bits, p.packing, &mut row)?;
        for px in row.chunks_exact(channels) {
            let v = |i: usize| px.get(i).map_or(0, |&c| to_u8(c, max));
            out.extend_from_slice(&match channels {
                1 => [v(0), v(0), v(0), 255],
                3 => [v(0), v(1), v(2), 255],
                _ => [v(0), v(1), v(2), v(3)],
            });
        }
    }
    Rgba8Image::new(p.width, p.height, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal DPX writer for tests: one element, codes given per sample at `bits`.
    struct Spec {
        big: bool,
        w: u32,
        h: u32,
        descriptor: u8,
        bits: u8,
        packing: u16,
        orientation: u16,
        /// `w × h × channels` code values.
        codes: Vec<u16>,
    }

    fn channels(descriptor: u8) -> usize {
        match descriptor {
            6 => 1,
            50 => 3,
            _ => 4,
        }
    }

    fn encode(s: &Spec) -> Vec<u8> {
        let p16 = |v: u16| {
            if s.big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let p32 = |v: u32| {
            if s.big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let mut f = vec![0u8; 2048];
        let put = |f: &mut Vec<u8>, at: usize, b: &[u8]| f[at..at + b.len()].copy_from_slice(b);
        put(&mut f, 0, if s.big { b"SDPX" } else { b"XPDS" });
        put(&mut f, 4, &p32(2048));
        put(&mut f, 768, &p16(s.orientation));
        put(&mut f, 770, &p16(1));
        put(&mut f, 772, &p32(s.w));
        put(&mut f, 776, &p32(s.h));
        f[780 + 20] = s.descriptor;
        f[780 + 23] = s.bits;
        put(&mut f, 780 + 24, &p16(s.packing));
        put(&mut f, 780 + 28, &p32(2048));
        put(&mut f, 780 + 32, &p32(UNDEF32));
        let nc = channels(s.descriptor);
        let per_line = s.w as usize * nc;
        for row in s.codes.chunks(per_line) {
            let mut line: Vec<u8> = Vec::new();
            match (s.bits, s.packing) {
                (8, _) => line.extend(row.iter().map(|&c| c as u8)),
                (16, _) => row.iter().for_each(|&c| line.extend(p16(c))),
                (10, 1 | 2) => {
                    for t in row.chunks(3) {
                        let mut word = 0u32;
                        for (i, &c) in t.iter().enumerate() {
                            let base = if s.packing == 1 { 22 } else { 20 };
                            word |= u32::from(c) << (base - 10 * i as u32);
                        }
                        line.extend(p32(word));
                    }
                }
                (12, 1) => row.iter().for_each(|&c| line.extend(p16(c << 4))),
                (b, _) => {
                    let (mut acc, mut n) = (0u64, 0u32);
                    for &c in row {
                        acc = (acc << b) | u64::from(c);
                        n += u32::from(b);
                        while n >= 32 {
                            line.extend(p32((acc >> (n - 32)) as u32));
                            n -= 32;
                            acc &= (1u64 << n) - 1;
                        }
                    }
                    if n > 0 {
                        line.extend(p32((acc << (32 - n)) as u32));
                    }
                }
            }
            while !line.len().is_multiple_of(4) {
                line.push(0);
            }
            f.extend(line);
        }
        f
    }

    fn spec(bits: u8, packing: u16, descriptor: u8, codes: Vec<u16>) -> Spec {
        Spec {
            big: true,
            w: 3,
            h: 2,
            descriptor,
            bits,
            packing,
            orientation: 0,
            codes,
        }
    }

    fn rgb_codes(max: u16) -> Vec<u16> {
        // 3×2 RGB: black, white, mid red; then three others.
        let m = max;
        let half = (u32::from(m) * 2).div_ceil(4) as u16;
        vec![
            0, 0, 0, m, m, m, m, 0, 0, 0, m, 0, 0, 0, m, half, half, half,
        ]
    }

    fn expect_rgb() -> Vec<[u8; 4]> {
        vec![
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [128, 128, 128, 255],
        ]
    }

    fn pixels(img: &Rgba8Image) -> Vec<[u8; 4]> {
        img.data()
            .chunks(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect()
    }

    fn decode_default(b: &[u8]) -> Result<Rgba8Image> {
        decode(b, &Limits::default())
    }

    #[test]
    fn every_depth_packing_and_order_decodes_rgb() {
        for (bits, packing) in [(8, 0), (10, 0), (10, 1), (10, 2), (12, 0), (12, 1), (16, 0)] {
            for big in [true, false] {
                let max = ((1u32 << bits) - 1) as u16;
                let mut s = spec(bits, packing, 50, rgb_codes(max));
                s.big = big;
                let file = encode(&s);
                assert!(sniff(&file));
                let img =
                    decode_default(&file).unwrap_or_else(|e| panic!("{bits}/{packing}/{big}: {e}"));
                assert_eq!((img.width(), img.height()), (3, 2));
                let got = pixels(&img);
                assert_eq!(got, expect_rgb(), "{bits}/{packing}/{big}");
                assert_eq!(got[0], [0, 0, 0, 255]);
                assert_eq!(got[1], [255, 255, 255, 255]);
                assert_eq!(got[2], [255, 0, 0, 255]);
                assert_eq!(got[4], [0, 0, 255, 255]);
            }
        }
    }

    #[test]
    fn rounding_to_eight_bits() {
        // 10-bit 512 is 127.75 -> 128; 100 is 24.9 -> 25. 16-bit 257 -> 1; 32767 -> 128.
        let img = decode_default(&encode(&spec(
            10,
            1,
            50,
            vec![
                512, 100, 1, 0, 1023, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
            ],
        )))
        .unwrap();
        assert_eq!(&img.data()[..4], &[128, 25, 0, 255]);
        let img = decode_default(&encode(&spec(
            16,
            0,
            6,
            vec![257, 32767, 128, 0, 65535, 129],
        )))
        .unwrap();
        let g: Vec<u8> = img.data().chunks(4).map(|p| p[0]).collect();
        assert_eq!(g, [1, 127, 0, 0, 255, 1]);
    }

    #[test]
    fn luma_and_rgba() {
        let img = decode_default(&encode(&spec(8, 0, 6, vec![0, 10, 20, 30, 40, 255]))).unwrap();
        assert_eq!(img.pixel(1, 0), Some([10, 10, 10, 255]));
        assert_eq!(img.pixel(2, 1), Some([255, 255, 255, 255]));
        let codes: Vec<u16> = (0..24).map(|i| i * 10).collect();
        let img = decode_default(&encode(&spec(8, 0, 51, codes))).unwrap();
        assert_eq!(img.pixel(0, 0), Some([0, 10, 20, 30]));
        assert_eq!(img.pixel(2, 1), Some([200, 210, 220, 230]));
        let h = header(&encode(&spec(8, 0, 51, vec![0; 24]))).unwrap();
        assert_eq!(h.has_alpha, Some(true));
    }

    #[test]
    fn header_reports_size_depth_and_orientation() {
        let mut s = spec(10, 1, 50, rgb_codes(1023));
        s.orientation = 5;
        let h = header(&encode(&s)).unwrap();
        assert_eq!(
            (h.width, h.height, h.bit_depth, h.has_alpha),
            (3, 2, Some(10), Some(false))
        );
        assert_eq!(h.orientation, Orientation::from_exif(6));
        assert!(header(b"nope").is_none());
    }

    #[test]
    fn refuses_unsupported_and_oversized() {
        let mut f = encode(&spec(8, 0, 50, vec![0; 18]));
        f[780 + 20] = 100; // CbYCrY
        assert!(decode_default(&f).is_err());
        let mut f = encode(&spec(8, 0, 50, vec![0; 18]));
        f[780 + 27] = 1; // RLE
        assert!(decode_default(&f).is_err());
        let f = encode(&spec(8, 0, 50, vec![0; 18]));
        let small = Limits {
            max_pixels: 5,
            ..Limits::default()
        };
        assert!(matches!(decode(&f, &small), Err(Error::TooLarge { .. })));
        let tiny = Limits {
            max_alloc: 8,
            ..Limits::default()
        };
        assert!(decode(&f, &tiny).is_err());
    }

    #[test]
    fn prefixes_and_byte_flips_never_panic() {
        let limits = Limits {
            max_pixels: 1 << 16,
            max_alloc: 1 << 20,
            ..Limits::default()
        };
        for (bits, packing) in [(8, 0), (10, 0), (10, 1), (12, 1), (16, 0)] {
            let max = ((1u32 << bits) - 1) as u16;
            let file = encode(&spec(bits, packing, 50, rgb_codes(max)));
            for n in 0..file.len() {
                let _ = decode(&file[..n], &limits);
                let _ = header(&file[..n]);
            }
            // Only the header region and the first pixel bytes matter; flip them all.
            for i in (0..file.len()).filter(|i| !(820..2048).contains(i)) {
                for x in [0xFF, 0x55, 0x80, 0x01] {
                    let mut f = file.clone();
                    f[i] ^= x;
                    let _ = decode(&f, &limits);
                    let _ = header(&f);
                }
            }
        }
    }
}
