//! DICOM (medical imaging), uncompressed pixel data only.
//!
//! * A 128-byte preamble and the `DICM` magic, then the file meta group (always explicit VR
//!   little endian) whose TransferSyntaxUID picks the data set's encoding: implicit VR little
//!   endian, explicit VR little endian, or explicit VR big endian. A file without meta
//!   information is read as implicit VR little endian.
//! * 8- and 16-bit allocated samples, `MONOCHROME1`, `MONOCHROME2` and `RGB` (interleaved or
//!   planar). The first frame of a multi-frame image is shown.
//! * Gray tone mapping: stored values go through the rescale slope and intercept, then the
//!   first **window centre/width** (the DICOM linear function) when both are present, else a
//!   **min/max stretch** over the image. `MONOCHROME1` is inverted. RGB samples are scaled from
//!   their stored bit depth to 8 bits.
//! * Compressed and encapsulated transfer syntaxes (JPEG, JPEG 2000, RLE, deflate) are refused
//!   with a clear error: PhotoCraft does not extract embedded JPEG frames either, so there is
//!   nothing to carry over.
//!
//! Element lengths come from the file and are never trusted: values and pixel data must lie
//! inside the input, sizes are checked against [`Limits`] before the output is allocated, and
//! only the elements needed are recorded.

use super::RawHeader;
use crate::{Error, Format, Limits, Result, Rgba8Image};

const F: Format = Format::Dicom;
const PREAMBLE: usize = 132; // 128 bytes + "DICM"
const UNDEFINED: u32 = 0xFFFF_FFFF;
const PIXEL_DATA: (u16, u16) = (0x7FE0, 0x0010);

fn bad(msg: &str) -> Error {
    Error::Malformed(F, msg.to_string())
}

/// A DICOM file has `DICM` after a 128-byte preamble.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    bytes.get(128..PREAMBLE) == Some(b"DICM")
}

pub(crate) fn header(bytes: &[u8]) -> Option<RawHeader> {
    let i = parse(bytes).ok()?;
    Some(RawHeader {
        width: i.cols,
        height: i.rows,
        bit_depth: u8::try_from(i.bits_stored).ok(),
        has_alpha: Some(false),
        animated: false,
        orientation: None,
    })
}

#[derive(Clone, Copy)]
struct Syntax {
    big: bool,
    explicit: bool,
}

/// VRs whose explicit-VR length field is two reserved bytes plus a `u32`.
fn long_vr(vr: [u8; 2]) -> bool {
    matches!(
        &vr,
        b"OB" | b"OW" | b"OF" | b"SQ" | b"UT" | b"UN" | b"OD" | b"OL" | b"OV" | b"UC" | b"UR"
    )
}

struct Cur<'a> {
    b: &'a [u8],
    pos: usize,
    big: bool,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }

    fn u16(&mut self) -> Option<u16> {
        let c: [u8; 2] = self.take(2)?.try_into().ok()?;
        Some(if self.big {
            u16::from_be_bytes(c)
        } else {
            u16::from_le_bytes(c)
        })
    }

    fn u32(&mut self) -> Option<u32> {
        let c: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(if self.big {
            u32::from_be_bytes(c)
        } else {
            u32::from_le_bytes(c)
        })
    }

    fn skip(&mut self, len: usize) -> Result<()> {
        self.pos = self
            .pos
            .checked_add(len)
            .filter(|p| *p <= self.b.len())
            .ok_or_else(|| bad("a value runs past the end of the file"))?;
        Ok(())
    }
}

/// One element's tag and value length, leaving the cursor at the value. Item and delimiter
/// tags (group `FFFE`) have a `u32` length and no VR even under explicit VR.
fn element_header(c: &mut Cur, syn: Syntax) -> Result<(u16, u16, u32)> {
    let cut = || bad("truncated element header");
    let group = c.u16().ok_or_else(cut)?;
    let elem = c.u16().ok_or_else(cut)?;
    let len = if group == 0xFFFE || !syn.explicit {
        c.u32().ok_or_else(cut)?
    } else {
        let vr = c.take(2).ok_or_else(cut)?;
        if long_vr([vr[0], vr[1]]) {
            c.take(2).ok_or_else(cut)?;
            c.u32().ok_or_else(cut)?
        } else {
            u32::from(c.u16().ok_or_else(cut)?)
        }
    };
    Ok((group, elem, len))
}

/// Skip an undefined-length sequence, ending after its sequence delimiter. Nesting is a counter,
/// not recursion; everything else is skipped by its length.
fn skip_undefined(c: &mut Cur, syn: Syntax) -> Result<()> {
    let mut depth = 1u32;
    while depth > 0 {
        let (group, elem, len) = element_header(c, syn)?;
        if group == 0xFFFE && matches!(elem, 0xE00D | 0xE0DD) {
            depth -= 1;
        } else if len == UNDEFINED {
            depth += 1;
        } else {
            c.skip(len as usize)?;
        }
    }
    Ok(())
}

const UNSUPPORTED_SYNTAX: &str =
    "compressed or unrecognised transfer syntax (only uncompressed pixel data is read)";

/// The data set's encoding and where it starts.
fn transfer_syntax(b: &[u8]) -> Result<(Syntax, usize)> {
    let meta = Syntax {
        big: false,
        explicit: true,
    };
    let mut uid: Option<String> = None;
    let mut data_start = b.len();
    let mut c = Cur {
        b,
        pos: PREAMBLE,
        big: false,
    };
    while c.pos < b.len() {
        let save = c.pos;
        let (group, elem, len) = element_header(&mut c, meta)?;
        if group != 0x0002 {
            data_start = save;
            break;
        }
        if len == UNDEFINED {
            return Err(Error::Unsupported(F, "undefined-length meta element"));
        }
        let value = c
            .take(len as usize)
            .ok_or_else(|| bad("a meta element runs past the end of the file"))?;
        if elem == 0x0010 {
            uid = Some(
                String::from_utf8_lossy(value)
                    .trim_end_matches(['\0', ' '])
                    .to_string(),
            );
        }
    }
    let syn = match uid.as_deref() {
        Some("1.2.840.10008.1.2.1") => Syntax {
            big: false,
            explicit: true,
        },
        Some("1.2.840.10008.1.2.2") => Syntax {
            big: true,
            explicit: true,
        },
        // No meta information: the historical default.
        Some("1.2.840.10008.1.2") | None => Syntax {
            big: false,
            explicit: false,
        },
        Some(_) => return Err(Error::Unsupported(F, UNSUPPORTED_SYNTAX)),
    };
    Ok((syn, data_start))
}

#[derive(Clone, Copy, PartialEq)]
enum Photometric {
    Mono1,
    Mono2,
    Rgb,
}

struct Info {
    rows: u32,
    cols: u32,
    samples: usize,
    bits_alloc: u16,
    bits_stored: u16,
    high_bit: u16,
    signed: bool,
    planar: bool,
    photometric: Photometric,
    centre: Option<f64>,
    width: Option<f64>,
    slope: f64,
    intercept: f64,
    big: bool,
    /// Start of the pixel data, and how much of its declared length is inside the input.
    pixel_start: usize,
    pixel_len: usize,
}

/// First number of a (possibly multi-valued) decimal string.
fn ds(v: &[u8]) -> Option<f64> {
    String::from_utf8_lossy(v)
        .split('\\')
        .next()?
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
}

#[derive(Default)]
struct Raw {
    samples: Option<u16>,
    photometric: Option<String>,
    planar: Option<u16>,
    rows: Option<u16>,
    cols: Option<u16>,
    bits_alloc: Option<u16>,
    bits_stored: Option<u16>,
    high_bit: Option<u16>,
    signed: Option<u16>,
    centre: Option<f64>,
    width: Option<f64>,
    intercept: Option<f64>,
    slope: Option<f64>,
}

fn parse(b: &[u8]) -> Result<Info> {
    if !sniff(b) {
        return Err(bad("no DICM magic after the preamble"));
    }
    let (syn, start) = transfer_syntax(b)?;
    let mut c = Cur {
        b,
        pos: start,
        big: syn.big,
    };
    let mut r = Raw::default();
    let (pixel_start, pixel_len) = loop {
        if c.pos >= b.len() {
            return Err(Error::Unsupported(F, "no pixel data (7FE0,0010)"));
        }
        let (group, elem, len) = element_header(&mut c, syn)?;
        if (group, elem) == PIXEL_DATA {
            if len == UNDEFINED {
                return Err(Error::Unsupported(F, UNSUPPORTED_SYNTAX));
            }
            let avail = b.len() - c.pos;
            break (c.pos, (len as usize).min(avail));
        }
        if len == UNDEFINED {
            skip_undefined(&mut c, syn)?;
            continue;
        }
        let value = c
            .take(len as usize)
            .ok_or_else(|| bad("an element value runs past the end of the file"))?;
        let u16v = || -> Option<u16> {
            let a: [u8; 2] = value.get(..2)?.try_into().ok()?;
            Some(if syn.big {
                u16::from_be_bytes(a)
            } else {
                u16::from_le_bytes(a)
            })
        };
        // The first occurrence of each tag wins.
        match (group, elem) {
            (0x0028, 0x0002) => r.samples = r.samples.or_else(u16v),
            (0x0028, 0x0004) => {
                r.photometric.get_or_insert_with(|| {
                    String::from_utf8_lossy(value)
                        .trim_end_matches(['\0', ' '])
                        .to_string()
                });
            }
            (0x0028, 0x0006) => r.planar = r.planar.or_else(u16v),
            (0x0028, 0x0010) => r.rows = r.rows.or_else(u16v),
            (0x0028, 0x0011) => r.cols = r.cols.or_else(u16v),
            (0x0028, 0x0100) => r.bits_alloc = r.bits_alloc.or_else(u16v),
            (0x0028, 0x0101) => r.bits_stored = r.bits_stored.or_else(u16v),
            (0x0028, 0x0102) => r.high_bit = r.high_bit.or_else(u16v),
            (0x0028, 0x0103) => r.signed = r.signed.or_else(u16v),
            (0x0028, 0x1050) => r.centre = r.centre.or_else(|| ds(value)),
            (0x0028, 0x1051) => r.width = r.width.or_else(|| ds(value)),
            (0x0028, 0x1052) => r.intercept = r.intercept.or_else(|| ds(value)),
            (0x0028, 0x1053) => r.slope = r.slope.or_else(|| ds(value)),
            _ => {}
        }
    };

    let rows = u32::from(r.rows.ok_or_else(|| bad("missing Rows (0028,0010)"))?);
    let cols = u32::from(r.cols.ok_or_else(|| bad("missing Columns (0028,0011)"))?);
    if rows == 0 || cols == 0 {
        return Err(bad("zero image size"));
    }
    let bits_alloc = r
        .bits_alloc
        .ok_or_else(|| bad("missing BitsAllocated (0028,0100)"))?;
    if !matches!(bits_alloc, 8 | 16) {
        return Err(Error::Unsupported(
            F,
            "only 8- and 16-bit allocated samples are read",
        ));
    }
    let bits_stored = r.bits_stored.unwrap_or(bits_alloc);
    if bits_stored == 0 || bits_stored > bits_alloc {
        return Err(bad("BitsStored is outside the allocated bits"));
    }
    let high_bit = r.high_bit.unwrap_or(bits_stored - 1);
    if high_bit >= bits_alloc || high_bit + 1 < bits_stored {
        return Err(bad("HighBit is inconsistent with BitsStored"));
    }
    let photometric = match r
        .photometric
        .as_deref()
        .ok_or_else(|| bad("missing PhotometricInterpretation (0028,0004)"))?
    {
        "MONOCHROME1" => Photometric::Mono1,
        "MONOCHROME2" => Photometric::Mono2,
        "RGB" => Photometric::Rgb,
        _ => {
            return Err(Error::Unsupported(
                F,
                "photometric interpretation other than MONOCHROME1/2 and RGB",
            ));
        }
    };
    let samples = usize::from(r.samples.unwrap_or(1));
    if samples
        != if photometric == Photometric::Rgb {
            3
        } else {
            1
        }
    {
        return Err(Error::Unsupported(
            F,
            "samples per pixel do not match the photometric interpretation",
        ));
    }
    Ok(Info {
        rows,
        cols,
        samples,
        bits_alloc,
        bits_stored,
        high_bit,
        signed: r.signed == Some(1),
        planar: r.planar == Some(1),
        photometric,
        centre: r.centre,
        width: r.width,
        slope: r.slope.unwrap_or(1.0),
        intercept: r.intercept.unwrap_or(0.0),
        big: syn.big,
        pixel_start,
        pixel_len,
    })
}

pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Rgba8Image> {
    limits.check_input(bytes.len())?;
    let i = parse(bytes)?;
    limits.check_size(i.cols, i.rows)?;
    let npix = (i.cols as usize)
        .checked_mul(i.rows as usize)
        .ok_or_else(|| bad("image too large"))?;
    let bytes_per = usize::from(i.bits_alloc / 8);
    let need = npix
        .checked_mul(i.samples)
        .and_then(|n| n.checked_mul(bytes_per))
        .ok_or_else(|| bad("pixel data too large"))?;
    if need > i.pixel_len {
        return Err(bad("pixel data is truncated"));
    }
    let data = bytes
        .get(i.pixel_start..i.pixel_start + need)
        .ok_or_else(|| bad("pixel data is truncated"))?;
    let out_len = npix.checked_mul(4).ok_or_else(|| bad("image too large"))?;
    if out_len as u64 > limits.max_alloc {
        return Err(Error::TooLarge {
            width: i.cols,
            height: i.rows,
            max_pixels: limits.max_pixels,
        });
    }

    let shift = u32::from(i.high_bit + 1 - i.bits_stored);
    let mask = (1u32 << i.bits_stored) - 1;
    // Stored value of sample `idx`: bits extracted, sign-extended when the data are signed.
    let stored = |idx: usize| -> Option<i32> {
        let raw = if bytes_per == 2 {
            let a: [u8; 2] = data.get(idx * 2..idx * 2 + 2)?.try_into().ok()?;
            u32::from(if i.big {
                u16::from_be_bytes(a)
            } else {
                u16::from_le_bytes(a)
            })
        } else {
            u32::from(*data.get(idx)?)
        };
        let v = (raw >> shift) & mask;
        let half = 1u32 << (i.bits_stored - 1);
        Some(if i.signed && v & half != 0 {
            v as i32 - (1i32 << i.bits_stored)
        } else {
            v as i32
        })
    };

    let mut out = Vec::new();
    out.try_reserve_exact(out_len)
        .map_err(|_| Error::InvalidArgument("cannot allocate the image".into()))?;

    if i.photometric == Photometric::Rgb {
        let scale = |v: i32| -> u8 {
            (f64::from(v) / f64::from(mask) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        for p in 0..npix {
            let mut px = [0u8, 0, 0, 255];
            for (ch, slot) in px.iter_mut().take(3).enumerate() {
                let idx = if i.planar { ch * npix + p } else { p * 3 + ch };
                *slot = scale(stored(idx).ok_or_else(|| bad("pixel data is truncated"))?);
            }
            out.extend_from_slice(&px);
        }
    } else {
        let value = |p: usize| -> Result<f64> {
            let v = stored(p).ok_or_else(|| bad("pixel data is truncated"))?;
            Ok(f64::from(v) * i.slope + i.intercept)
        };
        let window = i.centre.zip(i.width).filter(|(_, w)| *w >= 1.0);
        let (lo, hi) = if window.is_some() {
            (0.0, 0.0)
        } else {
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in 0..npix {
                let v = value(p)?;
                lo = lo.min(v);
                hi = hi.max(v);
            }
            (lo, hi)
        };
        for p in 0..npix {
            let v = value(p)?;
            let mut y = match window {
                Some((c, w)) if w <= 1.0 => f64::from(u8::from(v > c - 0.5)),
                Some((c, w)) => ((v - (c - 0.5)) / (w - 1.0) + 0.5).clamp(0.0, 1.0),
                None if hi > lo => (v - lo) / (hi - lo),
                None => 0.0,
            };
            if i.photometric == Photometric::Mono1 {
                y = 1.0 - y;
            }
            let g = (y * 255.0).round().clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&[g, g, g, 255]);
        }
    }
    Rgba8Image::new(i.cols, i.rows, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMPLICIT: &str = "1.2.840.10008.1.2";
    const EXPLICIT: &str = "1.2.840.10008.1.2.1";
    const BIG: &str = "1.2.840.10008.1.2.2";

    struct W {
        out: Vec<u8>,
        explicit: bool,
        big: bool,
    }

    impl W {
        fn u16(&self, v: u16) -> [u8; 2] {
            if self.big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        }

        fn el(&mut self, g: u16, e: u16, vr: &[u8; 2], val: &[u8]) {
            let mut val = val.to_vec();
            if val.len() % 2 == 1 {
                val.push(if vr == b"UI" { 0 } else { b' ' });
            }
            let (gb, eb) = (self.u16(g), self.u16(e));
            self.out.extend_from_slice(&gb);
            self.out.extend_from_slice(&eb);
            let long = long_vr(*vr);
            if self.explicit {
                self.out.extend_from_slice(vr);
                if long {
                    self.out.extend_from_slice(&[0, 0]);
                    let l = if self.big {
                        (val.len() as u32).to_be_bytes()
                    } else {
                        (val.len() as u32).to_le_bytes()
                    };
                    self.out.extend_from_slice(&l);
                } else {
                    let l = self.u16(val.len() as u16);
                    self.out.extend_from_slice(&l);
                }
            } else {
                self.out
                    .extend_from_slice(&(val.len() as u32).to_le_bytes());
            }
            self.out.extend_from_slice(&val);
        }

        fn us(&mut self, g: u16, e: u16, v: u16) {
            let b = self.u16(v);
            self.el(g, e, b"US", &b);
        }
    }

    struct Spec<'a> {
        ts: &'a str,
        rows: u16,
        cols: u16,
        samples: u16,
        photometric: &'a str,
        bits: u16,
        stored: u16,
        signed: bool,
        planar: bool,
        window: Option<(&'a str, &'a str)>,
        pixels: Vec<u8>,
    }

    fn spec(pixels: Vec<u8>) -> Spec<'static> {
        Spec {
            ts: EXPLICIT,
            rows: 2,
            cols: 2,
            samples: 1,
            photometric: "MONOCHROME2",
            bits: 8,
            stored: 8,
            signed: false,
            planar: false,
            window: None,
            pixels,
        }
    }

    /// A minimal writer: preamble, meta group, then the data set in the spec's syntax.
    fn file(s: &Spec) -> Vec<u8> {
        let mut meta = W {
            out: Vec::new(),
            explicit: true,
            big: false,
        };
        meta.el(2, 0x10, b"UI", s.ts.as_bytes());
        let mut out = vec![0u8; 128];
        out.extend_from_slice(b"DICM");
        out.extend(meta.out);
        let mut w = W {
            out: Vec::new(),
            explicit: s.ts != IMPLICIT,
            big: s.ts == BIG,
        };
        w.us(0x28, 0x02, s.samples);
        w.el(0x28, 0x04, b"CS", s.photometric.as_bytes());
        if s.planar {
            w.us(0x28, 0x06, 1);
        }
        w.us(0x28, 0x10, s.rows);
        w.us(0x28, 0x11, s.cols);
        w.us(0x28, 0x100, s.bits);
        w.us(0x28, 0x101, s.stored);
        w.us(0x28, 0x102, s.stored - 1);
        w.us(0x28, 0x103, u16::from(s.signed));
        if let Some((c, wd)) = s.window {
            w.el(0x28, 0x1050, b"DS", c.as_bytes());
            w.el(0x28, 0x1051, b"DS", wd.as_bytes());
        }
        w.el(
            0x7FE0,
            0x10,
            if s.bits == 8 { b"OB" } else { b"OW" },
            &s.pixels,
        );
        out.extend(w.out);
        out
    }

    fn le16(v: &[i16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn grays(img: &Rgba8Image) -> Vec<u8> {
        img.data().chunks(4).map(|p| p[0]).collect()
    }

    #[test]
    fn explicit_and_implicit_little_endian_agree() {
        for ts in [EXPLICIT, IMPLICIT] {
            let mut s = spec(vec![0, 85, 170, 255]);
            s.ts = ts;
            let f = file(&s);
            assert!(sniff(&f));
            let img = decode(&f, &Limits::default()).unwrap();
            assert_eq!((img.width(), img.height()), (2, 2));
            // DICOM rows are top-down: no flip.
            assert_eq!(grays(&img), vec![0, 85, 170, 255], "{ts}");
            assert!(img.is_opaque());
            let h = header(&f).unwrap();
            assert_eq!((h.width, h.height, h.bit_depth), (2, 2, Some(8)));
        }
        assert!(!sniff(b"short"));
    }

    #[test]
    fn min_max_stretch_for_sixteen_bit_and_signed_data() {
        let mut s = spec(le16(&[1000, 2000, 3000, 5000]));
        s.bits = 16;
        s.stored = 16;
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(grays(&img), vec![0, 64, 128, 255]);

        let mut s = spec(le16(&[-100, 0, 100, 0]));
        s.bits = 16;
        s.stored = 16;
        s.signed = true;
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(grays(&img), vec![0, 128, 255, 128]);
    }

    #[test]
    fn window_centre_and_width_are_applied() {
        let mut s = spec(le16(&[0, 1000, 2000, 3000]));
        s.bits = 16;
        s.stored = 16;
        s.window = Some(("1500", "2000"));
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(grays(&img), vec![0, 64, 191, 255]);
        // Multi-valued windows use the first.
        s.window = Some(("1500\\9", "2000\\9"));
        assert_eq!(
            grays(&decode(&file(&s), &Limits::default()).unwrap()),
            vec![0, 64, 191, 255]
        );
    }

    #[test]
    fn monochrome1_is_inverted() {
        let mut s = spec(vec![0, 85, 170, 255]);
        s.photometric = "MONOCHROME1";
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(grays(&img), vec![255, 170, 85, 0]);
    }

    #[test]
    fn twelve_bit_values_use_the_stored_bits() {
        let mut s = spec(le16(&[0, 1024, 2048, 4095]));
        s.bits = 16;
        s.stored = 12;
        s.window = Some(("2048", "4096"));
        let img = decode(&file(&s), &Limits::default()).unwrap();
        let g = grays(&img);
        assert_eq!((g[0], g[3]), (0, 255));
        assert!(g[1] > 55 && g[1] < 70, "{g:?}");
    }

    #[test]
    fn rgb_interleaved_and_planar() {
        let mut s = spec(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 8, 7]);
        s.samples = 3;
        s.photometric = "RGB";
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(img.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(img.pixel(1, 0), Some([0, 255, 0, 255]));
        assert_eq!(img.pixel(0, 1), Some([0, 0, 255, 255]));
        assert_eq!(img.pixel(1, 1), Some([9, 8, 7, 255]));

        // Planar: R plane, G plane, B plane.
        let mut s = spec(vec![255, 0, 0, 9, 0, 255, 0, 8, 0, 0, 255, 7]);
        s.samples = 3;
        s.photometric = "RGB";
        s.planar = true;
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(img.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(img.pixel(1, 0), Some([0, 255, 0, 255]));
        assert_eq!(img.pixel(0, 1), Some([0, 0, 255, 255]));
        assert_eq!(img.pixel(1, 1), Some([9, 8, 7, 255]));
    }

    #[test]
    fn big_endian_sixteen_bit() {
        let mut s = spec(
            [0u16, 100, 200, 400]
                .iter()
                .flat_map(|v| v.to_be_bytes())
                .collect(),
        );
        s.ts = BIG;
        s.bits = 16;
        s.stored = 16;
        let img = decode(&file(&s), &Limits::default()).unwrap();
        assert_eq!(grays(&img), vec![0, 64, 128, 255]);
    }

    #[test]
    fn sequences_before_the_pixels_are_skipped() {
        // An undefined-length SQ with one undefined-length item, ahead of the image group.
        let s = spec(vec![0, 85, 170, 255]);
        let plain = file(&s);
        let at = plain
            .windows(4)
            .position(|w| w == [0x28, 0, 0x02, 0])
            .unwrap();
        let mut seq = Vec::new();
        seq.extend_from_slice(&[
            0x08, 0x00, 0x15, 0x11, b'S', b'Q', 0, 0, 0xFF, 0xFF, 0xFF, 0xFF,
        ]);
        seq.extend_from_slice(&[0xFE, 0xFF, 0x00, 0xE0, 0xFF, 0xFF, 0xFF, 0xFF]);
        seq.extend_from_slice(&[0x08, 0x00, 0x50, 0x00, b'S', b'H', 2, 0, b'A', b'B']);
        seq.extend_from_slice(&[0xFE, 0xFF, 0x0D, 0xE0, 0, 0, 0, 0]);
        seq.extend_from_slice(&[0xFE, 0xFF, 0xDD, 0xE0, 0, 0, 0, 0]);
        let mut f = plain[..at].to_vec();
        f.extend(seq);
        f.extend_from_slice(&plain[at..]);
        let img = decode(&f, &Limits::default()).unwrap();
        assert_eq!(grays(&img), vec![0, 85, 170, 255]);
    }

    #[test]
    fn compressed_syntaxes_are_unsupported() {
        for ts in [
            "1.2.840.10008.1.2.4.50",
            "1.2.840.10008.1.2.4.90",
            "1.2.840.10008.1.2.5",
            "1.2.840.10008.1.2.1.99",
        ] {
            let mut s = spec(vec![0; 4]);
            s.ts = ts;
            assert!(
                matches!(
                    decode(&file(&s), &Limits::default()),
                    Err(Error::Unsupported(..))
                ),
                "{ts}"
            );
        }
        // Encapsulated pixel data under an uncompressed syntax label: undefined length.
        let mut f = file(&spec(vec![0; 4]));
        let n = f.len();
        f[n - 8..n - 4].copy_from_slice(&[0xFF; 4]);
        assert!(matches!(
            decode(&f, &Limits::default()),
            Err(Error::Unsupported(..))
        ));
    }

    #[test]
    fn unsupported_layouts_are_refused() {
        let l = Limits::default();
        let mut s = spec(vec![0; 16]);
        s.bits = 32;
        assert!(matches!(decode(&file(&s), &l), Err(Error::Unsupported(..))));
        let mut s = spec(vec![0; 4]);
        s.photometric = "PALETTE COLOR";
        assert!(matches!(decode(&file(&s), &l), Err(Error::Unsupported(..))));
        let mut s = spec(vec![0; 4]);
        s.photometric = "RGB";
        assert!(matches!(decode(&file(&s), &l), Err(Error::Unsupported(..))));
    }

    #[test]
    fn lengths_are_never_trusted() {
        let l = Limits::default();
        // Dimensions far over the limits, tiny file: refused on size.
        let mut s = spec(vec![0; 4]);
        s.rows = 60_000;
        s.cols = 60_000;
        assert!(matches!(decode(&file(&s), &l), Err(Error::TooLarge { .. })));
        // Pixel data claiming 4 GiB: clamped to what the file holds, nothing is allocated for it.
        let mut f = file(&spec(vec![0; 4]));
        let n = f.len();
        f[n - 8..n - 4].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
        assert_eq!(decode(&f, &l).unwrap().width(), 2);
        // ... and a short file with that claim is truncated, not trusted.
        assert!(matches!(decode(&f[..n - 1], &l), Err(Error::Malformed(..))));
        // Fewer pixels than the image needs.
        let mut s = spec(vec![0; 2]);
        s.rows = 2;
        assert!(matches!(decode(&file(&s), &l), Err(Error::Malformed(..))));
        let small = Limits {
            max_alloc: 8,
            ..Limits::default()
        };
        assert!(matches!(
            decode(&file(&spec(vec![0; 4])), &small),
            Err(Error::TooLarge { .. })
        ));
        let tiny = Limits {
            max_input_bytes: 100,
            ..Limits::default()
        };
        assert!(matches!(
            decode(&file(&spec(vec![0; 4])), &tiny),
            Err(Error::InputTooLarge { .. })
        ));
    }

    #[test]
    fn every_prefix_and_byte_flip_is_handled() {
        for ts in [EXPLICIT, IMPLICIT, BIG] {
            let mut s = spec(le16(&[1, 2, 3, 4]));
            s.ts = ts;
            s.bits = 16;
            s.stored = 16;
            s.window = Some(("2", "4"));
            let f = file(&s);
            let l = Limits::default();
            for n in 0..f.len() {
                let _ = decode(&f[..n], &l);
                let _ = header(&f[..n]);
            }
            for i in 0..f.len() {
                for flip in [0x01u8, 0x80, 0xFF] {
                    let mut g = f.clone();
                    g[i] ^= flip;
                    let _ = decode(&g, &l);
                    let _ = header(&g);
                }
            }
        }
    }
}
