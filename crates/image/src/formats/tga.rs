//! Truevision TGA: the fixed 18-byte header (the same for versions 1 and 2).

use super::RawHeader;
use crate::bytes::le_u16;
use crate::{Error, Format, Result};

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Tga, why.into());
    let (Some(&cmap_type), Some(&image_type), Some(&cmap_entry), Some(&bpp), Some(&desc)) =
        (b.get(1), b.get(2), b.get(7), b.get(16), b.get(17))
    else {
        return Err(bad("header is cut short"));
    };
    let (Some(width), Some(height)) = (le_u16(b, 12), le_u16(b, 14)) else {
        return Err(bad("header is cut short"));
    };
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    if cmap_type > 1 || !matches!(image_type, 1 | 2 | 3 | 9 | 10 | 11) {
        return Err(bad("unsupported colour-map or image type"));
    }
    // Colour-mapped images take their colours (and alpha) from the map's entry size.
    let colour_bits = if matches!(image_type, 1 | 9) {
        cmap_entry
    } else {
        bpp
    };
    if !matches!(colour_bits, 8 | 15 | 16 | 24 | 32) {
        return Err(bad("pixel depth out of range"));
    }
    // The low four descriptor bits count attribute (alpha) bits per pixel.
    let alpha = (desc & 0x0F) > 0 && matches!(colour_bits, 16 | 32);
    Ok(RawHeader {
        width: u32::from(width),
        height: u32::from(height),
        bit_depth: Some(if matches!(colour_bits, 15 | 16) { 5 } else { 8 }),
        has_alpha: Some(alpha),
        animated: false,
        orientation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolour_with_alpha_bits() {
        let mut b = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 7, 0, 32, 8];
        let h = header(&b).unwrap();
        assert_eq!((h.width, h.height, h.has_alpha), (5, 7, Some(true)));
        b[17] = 0;
        assert_eq!(header(&b).unwrap().has_alpha, Some(false));
        b[2] = 4;
        assert!(header(&b).is_err());
        assert!(header(&b[..10]).is_err());
    }
}
