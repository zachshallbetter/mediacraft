//! QOI: a 14-byte header of `qoif`, big-endian width and height, channel count and colour space.

use super::RawHeader;
use crate::bytes::be_u32;
use crate::{Error, Format, Result};

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Qoi, why.into());
    let (Some(width), Some(height), Some(&channels), Some(&space)) =
        (be_u32(b, 4), be_u32(b, 8), b.get(12), b.get(13))
    else {
        return Err(bad("header is cut short"));
    };
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    if !matches!(channels, 3 | 4) || space > 1 {
        return Err(bad("channel count or colour space out of range"));
    }
    Ok(RawHeader {
        width,
        height,
        bit_depth: Some(8),
        has_alpha: Some(channels == 4),
        animated: false,
        orientation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_size_and_channels() {
        let mut b = b"qoif".to_vec();
        b.extend_from_slice(&300u32.to_be_bytes());
        b.extend_from_slice(&200u32.to_be_bytes());
        b.extend_from_slice(&[4, 0]);
        let h = header(&b).unwrap();
        assert_eq!((h.width, h.height, h.has_alpha), (300, 200, Some(true)));
        b[12] = 5;
        assert!(header(&b).is_err());
        assert!(header(&b[..13]).is_err());
    }
}
