//! DirectDraw Surface: the 124-byte `DDS_HEADER` behind the `DDS ` magic, and the optional
//! DX10 extension.

use super::RawHeader;
use crate::bytes::le_u32;
use crate::{Error, Format, Result};

const DDPF_ALPHAPIXELS: u32 = 0x1;
const DDPF_ALPHA: u32 = 0x2;

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Dds, why.into());
    let field = |at: usize| le_u32(b, at).ok_or_else(|| bad("header is cut short"));
    if field(4)? != 124 || field(76)? != 32 {
        return Err(bad("header or pixel-format size is wrong"));
    }
    let (height, width) = (field(12)?, field(16)?);
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    let pf_flags = field(80)?;
    let fourcc = b.get(84..88).ok_or_else(|| bad("header is cut short"))?;
    let has_alpha = match fourcc {
        b"DXT1" => None, // BC1 may or may not use its 1-bit alpha
        b"DXT2" | b"DXT3" | b"DXT4" | b"DXT5" => Some(true),
        b"DX10" => None,
        _ => Some(pf_flags & (DDPF_ALPHAPIXELS | DDPF_ALPHA) != 0),
    };
    Ok(RawHeader {
        width,
        height,
        // Block-compressed data decodes to 8-bit channels.
        bit_depth: Some(8),
        has_alpha,
        animated: false,
        orientation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn dds(w: u32, h: u32, fourcc: &[u8; 4]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(fourcc);
        b
    }

    #[test]
    fn reads_size_and_alpha() {
        let h = header(&dds(64, 32, b"DXT5")).unwrap();
        assert_eq!((h.width, h.height, h.has_alpha), (64, 32, Some(true)));
        assert_eq!(header(&dds(4, 4, b"DXT1")).unwrap().has_alpha, None);
        assert!(header(&dds(0, 4, b"DXT1")).is_err());
        assert!(header(&dds(4, 4, b"DXT1")[..60]).is_err());
    }
}
