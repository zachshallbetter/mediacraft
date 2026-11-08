//! Netpbm headers: PBM, PGM and PPM (`P1`..`P6`, with `#` comments) and PAM (`P7`).

use super::RawHeader;
use crate::{Error, Format, Result};

/// A PNM header is a few dozen bytes; PAM comments can stretch it.
const MAX_HEADER: usize = 4096;

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Pnm, why.into());
    let b = b.get(..b.len().min(MAX_HEADER)).unwrap_or(b);
    match b.get(1) {
        Some(b'7') => pam(b),
        Some(&kind @ b'1'..=b'6') => {
            let mut p = Tokens { b, at: 2 };
            let width = p.number().ok_or_else(|| bad("width is missing"))?;
            let height = p.number().ok_or_else(|| bad("height is missing"))?;
            // Bitmaps (P1, P4) have no maximum sample value.
            let depth = if matches!(kind, b'1' | b'4') {
                1
            } else {
                let max = p.number().ok_or_else(|| bad("maximum value is missing"))?;
                max_to_depth(max).ok_or_else(|| bad("maximum value out of range"))?
            };
            sized(width, height, depth, false)
        }
        _ => Err(bad("not a Netpbm header")),
    }
}

fn max_to_depth(max: u32) -> Option<u8> {
    match max {
        1..=255 => Some(8),
        256..=65535 => Some(16),
        _ => None,
    }
}

fn sized(width: u32, height: u32, depth: u8, alpha: bool) -> Result<RawHeader> {
    if width == 0 || height == 0 {
        return Err(Error::Malformed(Format::Pnm, "zero image size".into()));
    }
    Ok(RawHeader {
        width,
        height,
        bit_depth: Some(depth),
        has_alpha: Some(alpha),
        animated: false,
        orientation: None,
    })
}

/// Whitespace-separated unsigned integers with `#` comments to the end of the line.
struct Tokens<'a> {
    b: &'a [u8],
    at: usize,
}

impl Tokens<'_> {
    fn number(&mut self) -> Option<u32> {
        loop {
            match self.b.get(self.at)? {
                b'#' => {
                    while *self.b.get(self.at)? != b'\n' {
                        self.at += 1;
                    }
                }
                c if c.is_ascii_whitespace() => self.at += 1,
                _ => break,
            }
        }
        let start = self.at;
        while self.b.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        // The digits must end at a delimiter that is actually in the data.
        let digits = self
            .b
            .get(start..self.at)
            .filter(|d| !d.is_empty() && d.len() <= 10)?;
        self.b.get(self.at).filter(|c| c.is_ascii_whitespace())?;
        std::str::from_utf8(digits).ok()?.parse().ok()
    }
}

/// PAM: `KEY value` lines up to `ENDHDR`.
fn pam(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Pnm, why.into());
    let (mut width, mut height, mut depth, mut max) = (None, None, None, None);
    let mut tuple_alpha = false;
    let mut lines = b.split(|&c| c == b'\n');
    lines.next(); // `P7`
    let mut ended = false;
    for line in lines {
        let line = String::from_utf8_lossy(line);
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let value = value.trim();
        match key {
            "ENDHDR" => {
                ended = true;
                break;
            }
            "WIDTH" => width = value.parse::<u32>().ok(),
            "HEIGHT" => height = value.parse::<u32>().ok(),
            "DEPTH" => depth = value.parse::<u32>().ok(),
            "MAXVAL" => max = value.parse::<u32>().ok(),
            "TUPLTYPE" => tuple_alpha |= value.ends_with("_ALPHA"),
            _ => {}
        }
    }
    if !ended {
        return Err(bad("PAM header is cut short"));
    }
    let (Some(width), Some(height), Some(depth), Some(max)) = (width, height, depth, max) else {
        return Err(bad("PAM header is missing a field"));
    };
    let bits = max_to_depth(max).ok_or_else(|| bad("maximum value out of range"))?;
    // Two channels are grey and alpha; four are RGB and alpha.
    sized(width, height, bits, tuple_alpha || matches!(depth, 2 | 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_binary_headers_with_comments() {
        let h = header(b"P6\n# made by hand\n3 2\n255\n").unwrap();
        assert_eq!((h.width, h.height, h.bit_depth), (3, 2, Some(8)));
        assert_eq!(header(b"P5 4 4 65535 ").unwrap().bit_depth, Some(16));
        assert_eq!(header(b"P4\n8 8\n").unwrap().bit_depth, Some(1));
        assert!(header(b"P6\n3 2\n").is_err());
        assert!(header(b"P6\n0 2\n255\n").is_err());
        assert!(header(b"P6\n99999999999 2\n255\n").is_err());
    }

    #[test]
    fn pam_reports_alpha() {
        let h = header(b"P7\nWIDTH 2\nHEIGHT 3\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")
            .unwrap();
        assert_eq!((h.width, h.height, h.has_alpha), (2, 3, Some(true)));
        let h = header(b"P7\nWIDTH 2\nHEIGHT 3\nDEPTH 1\nMAXVAL 255\nENDHDR\n").unwrap();
        assert_eq!(h.has_alpha, Some(false));
        assert!(header(b"P7\nWIDTH 2\nHEIGHT 3\n").is_err());
    }
}
