//! TIFF files and TIFF-structured camera raws: dimensions from the largest full-resolution IFD,
//! metadata from IFD0 (XMP tag 700, ICC tag 34675, IPTC tag 33723).

use super::RawHeader;
use crate::container::Embedded;
use crate::tiff::{Ifd, Tiff};
use crate::{Error, Format, Orientation, Result};
use std::borrow::Cow;

const MAX_IFDS: usize = 32;

pub(crate) fn header(b: &[u8], format: Format) -> Result<RawHeader> {
    let t = Tiff::new(b).ok_or_else(|| {
        Error::Malformed(
            format,
            "not a classic TIFF structure (BigTIFF and private magics are not read)".into(),
        )
    })?;
    let first = t
        .first_ifd()
        .ok_or_else(|| Error::Malformed(format, "no first IFD".into()))?;
    let ifd0 = t
        .ifd(first)
        .ok_or_else(|| Error::Malformed(format, "first IFD is damaged".into()))?;
    let orientation = ifd0
        .get(0x0112)
        .and_then(|e| t.uint(e))
        .and_then(|v| Orientation::from_exif(v as u16));

    // TIFF files describe the image in IFD0. Raws often keep a preview there and the sensor data
    // in a SubIFD or a later IFD; take the largest image that is not a reduced-resolution copy.
    let mut best: Option<(u32, u32, &Ifd)> = None;
    let mut pending = vec![first];
    let mut ifds: Vec<Ifd> = Vec::new();
    while let Some(pos) = pending.pop() {
        if ifds.len() >= MAX_IFDS {
            break;
        }
        let Some(ifd) = t.ifd(pos) else { continue };
        if ifd.next != 0 && ifd.next != pos && format == Format::Raw {
            pending.push(ifd.next);
        }
        if let Some(e) = ifd.get(0x014A) {
            for i in 0..(e.count as usize).min(8) {
                if let Some(p) = t.uint_at(e, i) {
                    pending.push(p as usize);
                }
            }
        }
        ifds.push(ifd);
    }
    for ifd in &ifds {
        let reduced = ifd
            .get(0x00FE)
            .and_then(|e| t.uint(e))
            .is_some_and(|v| v & 1 == 1);
        let (Some(w), Some(h)) = (
            ifd.get(0x0100).and_then(|e| t.uint(e)),
            ifd.get(0x0101).and_then(|e| t.uint(e)),
        ) else {
            continue;
        };
        if w == 0 || h == 0 || (reduced && format == Format::Raw) {
            continue;
        }
        if best
            .is_none_or(|(bw, bh, _)| u64::from(w) * u64::from(h) > u64::from(bw) * u64::from(bh))
        {
            best = Some((w, h, ifd));
        }
        if format == Format::Tiff {
            break; // IFD0 is the image; later IFDs are pages or thumbnails.
        }
    }
    let (width, height, ifd) =
        best.ok_or_else(|| Error::Malformed(format, "no IFD records an image size".into()))?;
    let bits = ifd
        .get(0x0102)
        .and_then(|e| t.uint_at(e, 0))
        .and_then(|v| u8::try_from(v).ok());
    let extra = ifd.get(0x0152).is_some();
    Ok(RawHeader {
        width,
        height,
        bit_depth: bits.or(Some(1)),
        has_alpha: Some(extra),
        animated: false,
        orientation,
    })
}

pub(crate) fn embedded(b: &[u8]) -> Embedded<'_> {
    let mut e = Embedded::default();
    let Some(t) = Tiff::new(b) else { return e };
    // The file is its own EXIF structure: IFD0 plus the Exif and GPS IFDs it points to.
    e.exif = Some(Cow::Borrowed(b));
    let Some(ifd0) = t.first_ifd().and_then(|p| t.ifd(p)) else {
        return e;
    };
    let blob = |tag: u16| {
        ifd0.get(tag)
            .and_then(|en| t.bytes(en))
            .filter(|d| !d.is_empty())
            .map(Cow::Borrowed)
    };
    e.xmp = blob(700);
    e.icc = blob(34675);
    e.iptc = blob(33723);
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiff::testutil::{Val, build};

    #[test]
    fn tiff_header_and_blocks() {
        let b = build(
            vec![
                (0x0100, Val::Long(1200)),
                (0x0101, Val::Long(800)),
                (0x0102, Val::Short(16)),
                (0x0112, Val::Short(8)),
                (0x0152, Val::Short(2)),
                (700, Val::Bytes(b"<x:xmpmeta/>".to_vec())),
                (34675, Val::Bytes(b"icc profile".to_vec())),
            ],
            vec![],
        );
        let h = header(&b, Format::Tiff).unwrap();
        assert_eq!(
            (h.width, h.height, h.bit_depth, h.has_alpha),
            (1200, 800, Some(16), Some(true))
        );
        assert_eq!(h.orientation, Some(Orientation::Rotate270));
        let e = embedded(&b);
        assert_eq!(e.xmp.as_deref(), Some(&b"<x:xmpmeta/>"[..]));
        assert_eq!(e.icc.as_deref(), Some(&b"icc profile"[..]));
        assert!(e.exif.is_some());
    }

    #[test]
    fn raw_takes_the_largest_full_resolution_ifd() {
        // IFD0 is a 160×120 preview (NewSubfileType 1); the SubIFD holds 6000×4000.
        let mut ifd0 = vec![
            (0x00FE, Val::Long(1)),
            (0x0100, Val::Long(160)),
            (0x0101, Val::Long(120)),
        ];
        ifd0.push((0xC612, Val::Bytes(vec![1, 4, 0, 0])));
        let b = build(
            ifd0,
            vec![(
                0x014A,
                vec![
                    (0x00FE, Val::Long(0)),
                    (0x0100, Val::Long(6000)),
                    (0x0101, Val::Long(4000)),
                ],
            )],
        );
        let h = header(&b, Format::Raw).unwrap();
        assert_eq!((h.width, h.height), (6000, 4000));
    }
}
