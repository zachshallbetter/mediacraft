//! Photoshop PSD/PSB file header and image resources section. Pixels come from `craft-psd`;
//! this reads only the first sections so a catalogue can work from a file's prefix.

use super::RawHeader;
use crate::bytes::{be_u16, be_u32};
use crate::container::{Embedded, photoshop_resources};
use crate::{Error, Format, Result};
use std::borrow::Cow;

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Psd, why.into());
    let channels = be_u16(b, 12).ok_or_else(|| bad("header is cut short"))?;
    let height = be_u32(b, 14).ok_or_else(|| bad("header is cut short"))?;
    let width = be_u32(b, 18).ok_or_else(|| bad("header is cut short"))?;
    let depth = be_u16(b, 22).ok_or_else(|| bad("header is cut short"))?;
    let mode = be_u16(b, 24).ok_or_else(|| bad("header is cut short"))?;
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    // Colour channels per mode: bitmap, gray, indexed, RGB, CMYK, multichannel, duotone, Lab.
    let colour = match mode {
        0..=2 | 8 => 1,
        3 | 9 => 3,
        4 => 4,
        _ => channels,
    };
    Ok(RawHeader {
        width,
        height,
        bit_depth: u8::try_from(depth).ok(),
        has_alpha: Some(channels > colour),
        animated: false,
        orientation: None,
    })
}

/// The image resources section: after the 26-byte header and the colour mode data.
fn resources(b: &[u8]) -> Option<&[u8]> {
    let cm_len = be_u32(b, 26)? as usize;
    let res_len_at = 30usize.checked_add(cm_len)?;
    let res_len = be_u32(b, res_len_at)? as usize;
    let start = res_len_at + 4;
    // A prefix may cut the section short; read what is there.
    let end = start.checked_add(res_len)?.min(b.len());
    b.get(start..end)
}

pub(crate) fn embedded(b: &[u8]) -> Embedded<'_> {
    let mut e = Embedded::default();
    let Some(section) = resources(b) else {
        return e;
    };
    for (id, d) in photoshop_resources(section) {
        match id {
            0x0404 if e.iptc.is_none() => e.iptc = Some(Cow::Borrowed(d)),
            0x040F if e.icc.is_none() => e.icc = Some(Cow::Borrowed(d)),
            0x0422 if e.exif.is_none() => {
                e.exif = crate::container::exif_payload(d).map(Cow::Borrowed)
            }
            0x0424 if e.xmp.is_none() => e.xmp = Some(Cow::Borrowed(d)),
            _ => {}
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_resources() {
        let mut b = b"8BPS\0\x01\0\0\0\0\0\0".to_vec();
        b.extend_from_slice(&4u16.to_be_bytes()); // RGB + alpha
        b.extend_from_slice(&300u32.to_be_bytes());
        b.extend_from_slice(&500u32.to_be_bytes());
        b.extend_from_slice(&8u16.to_be_bytes());
        b.extend_from_slice(&3u16.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes()); // colour mode data
        let res = b"8BIM\x04\x24\0\0\0\0\0\x04<x/>8BIM\x04\x0F\0\0\0\0\0\x02pp".to_vec();
        b.extend_from_slice(&(res.len() as u32).to_be_bytes());
        b.extend_from_slice(&res);
        let h = header(&b).unwrap();
        assert_eq!((h.width, h.height, h.has_alpha), (500, 300, Some(true)));
        let e = embedded(&b);
        assert_eq!(e.xmp.as_deref(), Some(&b"<x/>"[..]));
        assert_eq!(e.icc.as_deref(), Some(&b"pp"[..]));
    }
}
