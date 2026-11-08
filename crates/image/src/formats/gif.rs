//! GIF89a logical screen descriptor and block structure.

use super::RawHeader;
use crate::bytes::le_u16;
use crate::{Error, Format, Result};

/// Most frames counted before we stop walking. Two are enough to call it animated.
const FRAME_SCAN: usize = 2;

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let (Some(w), Some(h), Some(&packed)) = (le_u16(b, 6), le_u16(b, 8), b.get(10)) else {
        return Err(Error::Malformed(
            Format::Gif,
            "logical screen descriptor is cut short".into(),
        ));
    };
    if w == 0 || h == 0 {
        return Err(Error::Malformed(
            Format::Gif,
            format!("screen size {w}×{h}"),
        ));
    }
    let (frames, transparent) = scan(b, packed);
    Ok(RawHeader {
        width: u32::from(w),
        height: u32::from(h),
        bit_depth: Some(8),
        has_alpha: Some(transparent),
        animated: frames >= FRAME_SCAN,
        orientation: None,
    })
}

/// Walk blocks to count image descriptors (up to [`FRAME_SCAN`]) and see whether a graphic
/// control extension declares transparency.
fn scan(b: &[u8], packed: u8) -> (usize, bool) {
    let mut p = 13usize;
    if packed & 0x80 != 0 {
        p += 3 * (1usize << ((packed & 7) + 1));
    }
    let (mut frames, mut transparent) = (0, false);
    let skip_sub_blocks = |mut p: usize| -> Option<usize> {
        loop {
            let n = usize::from(*b.get(p)?);
            p += 1;
            if n == 0 {
                return Some(p);
            }
            p += n;
        }
    };
    while frames < FRAME_SCAN {
        match b.get(p) {
            Some(0x21) => {
                if b.get(p + 1) == Some(&0xF9) && b.get(p + 3).is_some_and(|f| f & 1 == 1) {
                    transparent = true;
                }
                let Some(next) = skip_sub_blocks(p + 2) else {
                    break;
                };
                p = next;
            }
            Some(0x2C) => {
                frames += 1;
                let Some(&flags) = b.get(p + 9) else { break };
                let mut q = p + 10;
                if flags & 0x80 != 0 {
                    q += 3 * (1usize << ((flags & 7) + 1));
                }
                // LZW minimum code size, then the data sub-blocks.
                let Some(next) = skip_sub_blocks(q + 1) else {
                    break;
                };
                p = next;
            }
            _ => break,
        }
    }
    (frames, transparent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(frames: usize, transparent: bool) -> Vec<u8> {
        let mut b = b"GIF89a".to_vec();
        b.extend_from_slice(&5u16.to_le_bytes());
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&[0, 0, 0]); // no global table
        for _ in 0..frames {
            b.extend_from_slice(&[0x21, 0xF9, 4, u8::from(transparent), 0, 0, 0, 0]);
            b.extend_from_slice(&[0x2C, 0, 0, 0, 0, 5, 0, 3, 0, 0]);
            b.extend_from_slice(&[2, 2, 0x4C, 0x01, 0]);
        }
        b.push(0x3B);
        b
    }

    #[test]
    fn still_and_animated() {
        let h = header(&gif(1, false)).unwrap();
        assert_eq!(
            (h.width, h.height, h.animated, h.has_alpha),
            (5, 3, false, Some(false))
        );
        let h = header(&gif(3, true)).unwrap();
        assert_eq!((h.animated, h.has_alpha), (true, Some(true)));
        assert!(header(b"GIF89a\x05").is_err());
    }
}
