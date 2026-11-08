//! Windows bitmap file and DIB headers (BITMAPCOREHEADER through BITMAPV5HEADER).

use super::RawHeader;
use crate::bytes::{le_u16, le_u32};
use crate::{Error, Format, Result};

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Bmp, why.into());
    let dib = le_u32(b, 14).ok_or_else(|| bad("DIB header is cut short"))?;
    let (w, h, bits) = if dib == 12 {
        let w = le_u16(b, 18).ok_or_else(|| bad("core header is cut short"))?;
        let h = le_u16(b, 20).ok_or_else(|| bad("core header is cut short"))?;
        let bits = le_u16(b, 24).ok_or_else(|| bad("core header is cut short"))?;
        (i64::from(w), i64::from(h), bits)
    } else {
        let w = le_u32(b, 18).ok_or_else(|| bad("info header is cut short"))? as i32;
        let h = le_u32(b, 22).ok_or_else(|| bad("info header is cut short"))? as i32;
        let bits = le_u16(b, 28).ok_or_else(|| bad("info header is cut short"))?;
        // A negative height marks a top-down bitmap.
        (i64::from(w), i64::from(h).abs(), bits)
    };
    let (Ok(width), Ok(height)) = (u32::try_from(w), u32::try_from(h)) else {
        return Err(bad("negative width"));
    };
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    Ok(RawHeader {
        width,
        height,
        bit_depth: Some(u8::try_from(bits.min(255)).unwrap_or(u8::MAX)),
        // 32-bit BI_RGB usually leaves the fourth byte unused; only a decode can tell.
        has_alpha: (bits != 32).then_some(false),
        animated: false,
        orientation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_header_with_top_down_height() {
        let mut b = b"BM".to_vec();
        b.extend_from_slice(&[0; 12]);
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&7i32.to_le_bytes());
        b.extend_from_slice(&(-9i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        let h = header(&b).unwrap();
        assert_eq!(
            (h.width, h.height, h.bit_depth, h.has_alpha),
            (7, 9, Some(24), Some(false))
        );
        assert!(header(&b[..20]).is_err());
    }
}
