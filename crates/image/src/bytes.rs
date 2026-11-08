//! Bounds-checked integer reads. Every offset that comes from a file goes through these.

pub(crate) fn be_u16(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

pub(crate) fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

pub(crate) fn le_u16(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

pub(crate) fn le_u32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// 24-bit little-endian value (WebP canvas sizes).
pub(crate) fn le_u24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(3)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], 0]))
}

/// Text from a metadata field: lossy UTF-8, NULs and surrounding whitespace dropped, `None` when
/// nothing is left.
pub(crate) fn clean_text(raw: &[u8]) -> Option<String> {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    let s = String::from_utf8_lossy(raw.get(..end)?);
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_refuse_out_of_range_and_overflowing_offsets() {
        let b = [1u8, 2, 3, 4];
        assert_eq!(be_u16(&b, 2), Some(0x0304));
        assert_eq!(le_u32(&b, 0), Some(0x0403_0201));
        assert_eq!(be_u32(&b, 1), None);
        assert_eq!(le_u16(&b, usize::MAX), None);
        assert_eq!(le_u24(&b, 1), Some(0x04_0302));
    }

    #[test]
    fn clean_text_trims_and_stops_at_nul() {
        assert_eq!(clean_text(b"  Canon \0junk"), Some("Canon".into()));
        assert_eq!(clean_text(b"   \0"), None);
        assert_eq!(clean_text(b""), None);
    }
}
