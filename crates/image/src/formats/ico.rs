//! ICO and CUR: a directory of images. The header describes the entry a decoder shows by
//! default: the one with the greatest colour depth, then the largest area, the last of equals.

use super::RawHeader;
use crate::bytes::{be_u32, le_u16, le_u32};
use crate::{Error, Format, Result};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Ico, why.into());
    let count = le_u16(b, 4).ok_or_else(|| bad("directory is cut short"))?;
    if count == 0 {
        return Err(bad("directory has no images"));
    }
    let is_cursor = le_u16(b, 2) == Some(2);
    let mut best: Option<((u16, u32), usize)> = None;
    // Same visiting order as the decoder: the last entry first, then the rest from the top.
    let last = usize::from(count) - 1;
    for i in std::iter::once(last).chain(0..last) {
        let at = 6 + i * 16;
        let (Some(&w), Some(&h)) = (b.get(at), b.get(at + 1)) else {
            return Err(bad("directory is cut short"));
        };
        // A cursor reuses the planes and bit-count fields for its hotspot.
        let bits = if is_cursor {
            0
        } else {
            le_u16(b, at + 6).ok_or_else(|| bad("directory is cut short"))?
        };
        let dim = |v: u8| if v == 0 { 256 } else { u32::from(v) };
        let score = (bits, dim(w) * dim(h));
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, at));
        }
    }
    let (_, at) = best.ok_or_else(|| bad("directory has no images"))?;
    let dim = |v: Option<&u8>| v.map(|&v| if v == 0 { 256 } else { u32::from(v) });
    let (mut width, mut height) = (
        dim(b.get(at)).ok_or_else(|| bad("directory is cut short"))?,
        dim(b.get(at + 1)).ok_or_else(|| bad("directory is cut short"))?,
    );
    let bits = if is_cursor {
        0
    } else {
        le_u16(b, at + 6).unwrap_or(0)
    };
    // An entry stored as PNG is sized by the PNG, which wins over the directory.
    let offset = le_u32(b, at + 12).map(|o| o as usize);
    if let Some(o) = offset.filter(|&o| b.get(o..o.saturating_add(8)) == Some(PNG_SIGNATURE))
        && let (Some(w), Some(h)) = (be_u32(b, o + 16), be_u32(b, o + 20))
        && w > 0
        && h > 0
    {
        (width, height) = (w, h);
    }
    Ok(RawHeader {
        width,
        height,
        bit_depth: (bits > 0).then(|| u8::try_from(bits.min(255)).unwrap_or(u8::MAX)),
        has_alpha: (bits == 32).then_some(true),
        animated: false,
        orientation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(w: u8, h: u8, bits: u16) -> Vec<u8> {
        let mut e = vec![w, h, 0, 0, 1, 0];
        e.extend_from_slice(&bits.to_le_bytes());
        e.extend_from_slice(&40u32.to_le_bytes());
        e.extend_from_slice(&100u32.to_le_bytes());
        e
    }

    #[test]
    fn picks_the_deepest_then_the_largest_entry() {
        let mut b = vec![0, 0, 1, 0, 3, 0];
        b.extend(entry(16, 16, 32));
        b.extend(entry(0, 0, 8));
        b.extend(entry(32, 32, 32));
        let h = header(&b).unwrap();
        assert_eq!((h.width, h.height, h.bit_depth), (32, 32, Some(32)));
        assert_eq!(h.has_alpha, Some(true));
    }

    #[test]
    fn empty_or_short_directories_are_errors() {
        assert!(header(&[0, 0, 1, 0, 0, 0]).is_err());
        assert!(header(&[0, 0, 1, 0, 2, 0, 1, 1]).is_err());
    }
}
