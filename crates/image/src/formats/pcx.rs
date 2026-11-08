//! ZSoft PCX (Paintbrush) reader, ported from PhotoCraft's codec.
//!
//! Supported: version 0, 2, 3 or 5 files with RLE data; 1, 2 and 4 bits per pixel through the
//! 16-colour header palette (one to four bit planes, 4 bits per pixel in total at most), 8-bit
//! palette images through the 256-colour VGA palette at the end of the file (or grey when the
//! palette-info field says grey scale and there is none), and 24-bit images as three 8-bit
//! planes. A 1-bit image whose header palette is all black is shown as black and white. The
//! result is always opaque.
//!
//! RLE packets must not cross a scanline (planes of one scanline form a single run space, as
//! encoders write them).

use super::RawHeader;
use crate::{Error, Format, Limits, Result, Rgba8Image};

const HEADER: usize = 128;

fn bad(msg: impl std::fmt::Display) -> Error {
    Error::Malformed(Format::Pcx, msg.to_string())
}

fn le16(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2)
        .map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}

/// PCX has a weak magic number (0x0A), so require plausible complete header fields.
pub(crate) fn sniff(b: &[u8]) -> bool {
    layout(b).is_some()
}

struct Layout {
    width: u32,
    height: u32,
    bits: usize,
    planes: usize,
    stride: usize,
}

fn layout(b: &[u8]) -> Option<Layout> {
    if b.len() < HEADER || b[0] != 0x0a || !matches!(b[1], 0 | 2 | 3 | 5) || b[2] != 1 {
        return None;
    }
    let (bits, planes) = (usize::from(b[3]), usize::from(b[65]));
    let stride = usize::from(le16(b, 66));
    let (x0, y0, x1, y1) = (le16(b, 4), le16(b, 6), le16(b, 8), le16(b, 10));
    if x1 < x0 || y1 < y0 || !matches!(bits, 1 | 2 | 4 | 8) || !(1..=4).contains(&planes) {
        return None;
    }
    let width = u32::from(x1 - x0) + 1;
    let height = u32::from(y1 - y0) + 1;
    let min_stride = (width as usize * bits).div_ceil(8);
    let ok = stride >= min_stride
        && stride > 0
        && (bits != 8 || matches!(planes, 1 | 3))
        && (bits == 8 || bits * planes <= 4);
    ok.then_some(Layout {
        width,
        height,
        bits,
        planes,
        stride,
    })
}

/// Size and depth (bits per pixel across planes) from the header alone.
pub(crate) fn header(bytes: &[u8]) -> Option<RawHeader> {
    let l = layout(bytes)?;
    Some(RawHeader {
        width: l.width,
        height: l.height,
        bit_depth: u8::try_from(l.bits * l.planes).ok(),
        has_alpha: Some(false),
        animated: false,
        orientation: None,
    })
}

/// Read one compressed scanline, all planes. RLE packets never cross a scanline.
fn read_row(b: &[u8], pos: &mut usize, out: &mut [u8]) -> Result<()> {
    let mut n = 0;
    while n < out.len() {
        let code = *b.get(*pos).ok_or_else(|| bad("truncated pixel RLE"))?;
        *pos += 1;
        let (count, value) = if code >= 0xc0 {
            let value = *b.get(*pos).ok_or_else(|| bad("truncated RLE packet"))?;
            *pos += 1;
            (usize::from(code & 0x3f), value)
        } else {
            (1, code)
        };
        if count == 0 || count > out.len() - n {
            return Err(bad("RLE packet crosses a scanline or has zero length"));
        }
        out[n..n + count].fill(value);
        n += count;
    }
    Ok(())
}

/// Decode to opaque 8-bit RGBA.
pub(crate) fn decode(b: &[u8], limits: &Limits) -> Result<Rgba8Image> {
    limits.check_input(b.len())?;
    let l = layout(b).ok_or_else(|| bad("invalid header or unsupported pixel layout"))?;
    limits.check_size(l.width, l.height)?;
    let (w, h) = (l.width as usize, l.height as usize);
    let too_large = || Error::TooLarge {
        width: l.width,
        height: l.height,
        max_pixels: limits.max_pixels,
    };
    let total = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(too_large)?;
    if total as u64 > limits.max_alloc {
        return Err(too_large());
    }
    let (bits, planes, stride) = (l.bits, l.planes, l.stride);
    let row_len = stride * planes;
    // The last 769 bytes are the optional VGA palette of an 8-bit indexed image.
    let indexed8 = bits == 8 && planes == 1;
    let vga = indexed8 && b.len() >= HEADER + 769 && b[b.len() - 769] == 0x0c;
    if indexed8 && !vga && le16(b, 68) != 2 {
        return Err(bad("8-bit indexed image has no 256-colour palette"));
    }
    let image_end = if vga { b.len() - 769 } else { b.len() };
    // Smallest possible encoding of a row: runs of 63 take 2 bytes, a remainder of 1 can be a
    // literal. Refuse files that cannot hold every row before allocating.
    let min_row = (row_len / 63) * 2
        + match row_len % 63 {
            0 => 0,
            1 => 1,
            _ => 2,
        };
    let min_encoded = min_row.checked_mul(h).ok_or_else(too_large)?;
    if image_end.saturating_sub(HEADER) < min_encoded {
        return Err(bad("pixel data is too short for the declared image"));
    }
    let palette = |index: usize| -> [u8; 3] {
        let base = if vga { image_end + 1 } else { 16 };
        let at = base + index * 3;
        b.get(at..at + 3).map_or([0; 3], |s| [s[0], s[1], s[2]])
    };
    // A 1-bit image with an untouched (all black) header palette is black and white.
    let mono_default = bits == 1 && planes == 1 && b[16..22].iter().all(|&v| v == 0);

    let mut out = Vec::new();
    out.try_reserve_exact(total).map_err(|_| too_large())?;
    let mut row = vec![0u8; row_len];
    let mut pos = HEADER;
    let data = &b[..image_end];
    for _ in 0..h {
        read_row(data, &mut pos, &mut row)?;
        for x in 0..w {
            let rgb = if bits == 8 && planes == 3 {
                [row[x], row[stride + x], row[2 * stride + x]]
            } else {
                let index = if bits == 8 {
                    usize::from(row[x])
                } else {
                    let bit = x * bits;
                    let shift = 8 - bits - (bit % 8);
                    let mask = (1usize << bits) - 1;
                    (0..planes).fold(0usize, |v, plane| {
                        let sample = (usize::from(row[plane * stride + bit / 8]) >> shift) & mask;
                        v | sample << (plane * bits)
                    })
                };
                if mono_default {
                    [if index == 0 { 0 } else { 255 }; 3]
                } else if vga || bits != 8 {
                    palette(index)
                } else {
                    let g = index as u8;
                    [g, g, g] // palette-info 2: grey scale
                }
            };
            out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Rgba8Image::new(l.width, l.height, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rle(row: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < row.len() {
            let mut run = 1;
            while i + run < row.len() && row[i + run] == row[i] && run < 63 {
                run += 1;
            }
            if run > 1 || row[i] >= 0xc0 {
                out.push(0xc0 | run as u8);
            }
            out.push(row[i]);
            i += run;
        }
        out
    }

    struct Spec {
        w: u16,
        h: u16,
        bits: u8,
        planes: u8,
        stride: u16,
        /// 16-colour header palette.
        palette16: Vec<[u8; 3]>,
        /// Uncompressed rows (all planes).
        rows: Vec<Vec<u8>>,
        vga: Option<Vec<[u8; 3]>>,
        palette_info: u16,
    }

    fn encode(s: &Spec) -> Vec<u8> {
        let mut f = vec![0u8; HEADER];
        f[0] = 0x0a;
        f[1] = 5;
        f[2] = 1;
        f[3] = s.bits;
        f[8..10].copy_from_slice(&(s.w - 1).to_le_bytes());
        f[10..12].copy_from_slice(&(s.h - 1).to_le_bytes());
        for (i, c) in s.palette16.iter().enumerate() {
            f[16 + i * 3..19 + i * 3].copy_from_slice(c);
        }
        f[65] = s.planes;
        f[66..68].copy_from_slice(&s.stride.to_le_bytes());
        f[68..70].copy_from_slice(&s.palette_info.to_le_bytes());
        for r in &s.rows {
            f.extend(rle(r));
        }
        if let Some(p) = &s.vga {
            f.push(0x0c);
            for c in p {
                f.extend_from_slice(c);
            }
        }
        f
    }

    fn px(img: &Rgba8Image, x: u32, y: u32) -> [u8; 3] {
        let p = img.pixel(x, y).unwrap();
        assert_eq!(p[3], 255);
        [p[0], p[1], p[2]]
    }

    #[test]
    fn eight_bit_with_vga_palette() {
        let mut pal = vec![[0u8; 3]; 256];
        pal[1] = [10, 20, 30];
        pal[200] = [250, 0, 5];
        // Row 0 has a run (to exercise RLE) and a byte >= 0xC0 as a literal-in-run.
        let s = Spec {
            w: 5,
            h: 2,
            bits: 8,
            planes: 1,
            stride: 6,
            palette16: vec![],
            rows: vec![vec![1, 1, 1, 200, 0, 0], vec![200, 0, 1, 1, 1, 0]],
            vga: Some(pal),
            palette_info: 1,
        };
        let f = encode(&s);
        assert!(sniff(&f));
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!((img.width(), img.height()), (5, 2));
        assert_eq!(px(&img, 0, 0), [10, 20, 30]);
        assert_eq!(px(&img, 3, 0), [250, 0, 5]);
        assert_eq!(px(&img, 4, 0), [0, 0, 0]);
        assert_eq!(px(&img, 0, 1), [250, 0, 5]);
        assert_eq!(px(&img, 3, 1), [10, 20, 30]);
        let h = header(&f).unwrap();
        assert_eq!(
            (h.width, h.height, h.bit_depth, h.has_alpha),
            (5, 2, Some(8), Some(false))
        );
    }

    #[test]
    fn eight_bit_grey_without_palette() {
        let s = Spec {
            w: 3,
            h: 1,
            bits: 8,
            planes: 1,
            stride: 4,
            palette16: vec![],
            rows: vec![vec![0, 100, 255, 0]],
            vga: None,
            palette_info: 2,
        };
        let img = decode(&encode(&s), &Limits::default()).unwrap();
        assert_eq!(px(&img, 1, 0), [100; 3]);
        let s = Spec {
            palette_info: 1,
            ..s
        };
        assert!(decode(&encode(&s), &Limits::default()).is_err());
    }

    #[test]
    fn one_bit_black_and_white_and_custom_palette() {
        // 10 pixels, stride 2: 1010101011 -> bytes 0xAA, 0xC0.
        let mut s = Spec {
            w: 10,
            h: 1,
            bits: 1,
            planes: 1,
            stride: 2,
            palette16: vec![],
            rows: vec![vec![0xAA, 0xC0]],
            vga: None,
            palette_info: 1,
        };
        let img = decode(&encode(&s), &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0), [255; 3]);
        assert_eq!(px(&img, 1, 0), [0; 3]);
        assert_eq!(px(&img, 9, 0), [255; 3]);
        s.palette16 = vec![[1, 2, 3], [200, 100, 50]];
        let img = decode(&encode(&s), &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0), [200, 100, 50]);
        assert_eq!(px(&img, 1, 0), [1, 2, 3]);
    }

    #[test]
    fn four_bit_packed_and_planar() {
        let pal: Vec<[u8; 3]> = (0..16).map(|i| [i * 16, 255 - i * 16, i]).collect();
        // Packed: 3 pixels with indices 1, 15, 7 -> 0x1F, 0x70.
        let s = Spec {
            w: 3,
            h: 1,
            bits: 4,
            planes: 1,
            stride: 2,
            palette16: pal.clone(),
            rows: vec![vec![0x1F, 0x70]],
            vga: None,
            palette_info: 1,
        };
        let img = decode(&encode(&s), &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0), pal[1]);
        assert_eq!(px(&img, 1, 0), pal[15]);
        assert_eq!(px(&img, 2, 0), pal[7]);
        // Four 1-bit planes, 8 pixels, stride 1: pixel x has index bits (p0..p3).
        let idx = [0usize, 1, 2, 4, 8, 15, 9, 6];
        let plane = |p: usize| {
            idx.iter()
                .enumerate()
                .fold(0u8, |v, (x, &i)| v | (((i >> p) & 1) as u8) << (7 - x))
        };
        let s = Spec {
            w: 8,
            h: 1,
            bits: 1,
            planes: 4,
            stride: 1,
            palette16: pal.clone(),
            rows: vec![(0..4).map(plane).collect()],
            vga: None,
            palette_info: 1,
        };
        let img = decode(&encode(&s), &Limits::default()).unwrap();
        for (x, &i) in idx.iter().enumerate() {
            assert_eq!(px(&img, x as u32, 0), pal[i], "pixel {x}");
        }
    }

    #[test]
    fn twenty_four_bit_planar() {
        let rows = vec![
            vec![
                255, 0, 7, 0, /* G */ 0, 255, 8, 0, /* B */ 0, 0, 9, 0,
            ],
            vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0],
        ];
        let s = Spec {
            w: 3,
            h: 2,
            bits: 8,
            planes: 3,
            stride: 4,
            palette16: vec![],
            rows,
            vga: None,
            palette_info: 1,
        };
        let f = encode(&s);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(px(&img, 0, 0), [255, 0, 0]);
        assert_eq!(px(&img, 1, 0), [0, 255, 0]);
        assert_eq!(px(&img, 2, 0), [7, 8, 9]);
        assert_eq!(px(&img, 2, 1), [3, 6, 9]);
        assert_eq!(header(&f).unwrap().bit_depth, Some(24));
    }

    #[test]
    fn refuses_bad_input_and_limits() {
        assert!(!sniff(b"\x0a\x05\x01"));
        assert!(header(&[0u8; 200]).is_none());
        let s = Spec {
            w: 3,
            h: 1,
            bits: 8,
            planes: 1,
            stride: 4,
            palette16: vec![],
            rows: vec![vec![0, 1, 2, 0]],
            vga: None,
            palette_info: 2,
        };
        let f = encode(&s);
        let small = Limits {
            max_pixels: 2,
            ..Limits::default()
        };
        assert!(matches!(decode(&f, &small), Err(Error::TooLarge { .. })));
        assert!(decode(&f[..f.len() - 1], &Limits::default()).is_err());
    }

    #[test]
    fn prefixes_and_byte_flips_never_panic() {
        let limits = Limits {
            max_pixels: 1 << 16,
            max_alloc: 1 << 20,
            ..Limits::default()
        };
        let mut pal = vec![[0u8; 3]; 256];
        pal[3] = [9, 9, 9];
        let files = [
            encode(&Spec {
                w: 5,
                h: 2,
                bits: 8,
                planes: 1,
                stride: 6,
                palette16: vec![],
                rows: vec![vec![3, 3, 3, 200, 0, 0], vec![200, 0, 1, 1, 1, 0]],
                vga: Some(pal),
                palette_info: 1,
            }),
            encode(&Spec {
                w: 3,
                h: 1,
                bits: 8,
                planes: 3,
                stride: 4,
                palette16: vec![],
                rows: vec![vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0]],
                vga: None,
                palette_info: 1,
            }),
            encode(&Spec {
                w: 8,
                h: 1,
                bits: 1,
                planes: 4,
                stride: 1,
                palette16: vec![[5; 3]; 16],
                rows: vec![vec![0xF0, 0x0F, 0xAA, 0x55]],
                vga: None,
                palette_info: 1,
            }),
            encode(&Spec {
                w: 3,
                h: 1,
                bits: 4,
                planes: 1,
                stride: 2,
                palette16: vec![[5; 3]; 16],
                rows: vec![vec![0x1F, 0x70]],
                vga: None,
                palette_info: 1,
            }),
        ];
        for file in &files {
            for n in 0..file.len() {
                let _ = decode(&file[..n], &limits);
                let _ = header(&file[..n]);
            }
            for i in 0..file.len() {
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
