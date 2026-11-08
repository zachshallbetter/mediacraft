//! Radiance HDR (RGBE): text header lines, a blank line, then a resolution string such as
//! `-Y 480 +X 640`.

use super::RawHeader;
use crate::{Error, Format, Result};

/// The header is text; stop looking after this many bytes.
const MAX_HEADER: usize = 64 << 10;

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Hdr, why.into());
    let b = b.get(..b.len().min(MAX_HEADER)).unwrap_or(b);
    let mut lines = b.split(|&c| c == b'\n');
    lines.next(); // the `#?RADIANCE` line
    // Everything up to the first empty line is variables and comments.
    let mut ended = false;
    for line in lines.by_ref() {
        if line.is_empty() {
            ended = true;
            break;
        }
    }
    if !ended {
        return Err(bad("header is cut short"));
    }
    let res = lines
        .next()
        .ok_or_else(|| bad("resolution line is missing"))?;
    let text = std::str::from_utf8(res).map_err(|_| bad("resolution line is not text"))?;
    let mut t = text.split_ascii_whitespace();
    let (Some(a), Some(n), Some(c), Some(m)) = (t.next(), t.next(), t.next(), t.next()) else {
        return Err(bad("resolution line is cut short"));
    };
    let (Ok(n), Ok(m)) = (n.parse::<u32>(), m.parse::<u32>()) else {
        return Err(bad("resolution is not a number"));
    };
    // The first axis is the slow one (rows): `Y` first means n rows of m pixels.
    let (width, height) = match (a.get(1..), c.get(1..)) {
        (Some("Y"), Some("X")) => (m, n),
        (Some("X"), Some("Y")) => (n, m),
        _ => return Err(bad("resolution axes are not X and Y")),
    };
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    Ok(RawHeader {
        width,
        height,
        // Decoded to f32 samples; the file stores 8-bit mantissas with a shared exponent.
        bit_depth: Some(32),
        has_alpha: Some(false),
        animated: false,
        orientation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_and_transposed_resolution() {
        let h = header(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 7 +X 9\n").unwrap();
        assert_eq!((h.width, h.height), (9, 7));
        let h = header(b"#?RGBE\n\n+X 7 -Y 9\n").unwrap();
        assert_eq!((h.width, h.height), (7, 9));
        assert!(header(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n").is_err());
        assert!(header(b"#?RADIANCE\n\n-Y 0 +X 9\n").is_err());
        assert!(header(b"#?RADIANCE\n\n-Y 7 +Q 9\n").is_err());
    }
}
