//! JPEG markers (ITU-T T.81 Annex B), the EXIF APP1 (CIPA DC-008), XMP APP1, ICC APP2 (ICC.1
//! Annex B.4) and Photoshop APP13 segments.

use super::RawHeader;
use crate::bytes::be_u16;
use crate::container::{Embedded, MAX_BLOCK, exif_payload, photoshop_resources};
use crate::{Error, Format, Result};
use std::borrow::Cow;

const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const ICC_SIG: &[u8] = b"ICC_PROFILE\0";
const PS_SIG: &[u8] = b"Photoshop 3.0\0";

/// A marker segment: its marker byte and payload (after the length field).
pub(crate) struct Segment<'a> {
    pub marker: u8,
    pub data: &'a [u8],
}

/// The marker segments before the first scan. Stops at SOS, EOI, or the first damage.
/// `reached_scan` is true when an SOS marker was found.
pub(crate) fn segments(b: &[u8]) -> (Vec<Segment<'_>>, bool) {
    let mut out = Vec::new();
    let mut p = 2usize;
    loop {
        if b.get(p) != Some(&0xFF) {
            return (out, false);
        }
        while b.get(p) == Some(&0xFF) {
            p += 1;
        }
        let Some(&marker) = b.get(p) else {
            return (out, false);
        };
        p += 1;
        match marker {
            0xD0..=0xD7 | 0x01 | 0xD8 => continue,
            0xD9 => return (out, false),
            _ => {}
        }
        let Some(len) = be_u16(b, p) else {
            return (out, false);
        };
        let len = usize::from(len);
        if len < 2 {
            return (out, false);
        }
        let Some(data) = b.get(p + 2..p + len) else {
            return (out, false);
        };
        if marker == 0xDA {
            return (out, true);
        }
        out.push(Segment { marker, data });
        p += len;
    }
}

/// SOFn markers: C0–CF except DHT (C4), JPG (C8) and DAC (CC).
fn is_sof(m: u8) -> bool {
    matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC)
}

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let (segs, _) = segments(b);
    let sof = segs.iter().find(|s| is_sof(s.marker)).ok_or_else(|| {
        Error::Malformed(
            Format::Jpeg,
            "no frame header before the end of the data".into(),
        )
    })?;
    let d = sof.data;
    let (Some(&precision), Some(height), Some(width)) = (d.first(), be_u16(d, 1), be_u16(d, 3))
    else {
        return Err(Error::Malformed(
            Format::Jpeg,
            "frame header is cut short".into(),
        ));
    };
    if width == 0 || height == 0 {
        // A zero height means the size is in a DNL marker after the first scan: rare, refused.
        return Err(Error::Malformed(
            Format::Jpeg,
            format!("frame size {width}×{height}"),
        ));
    }
    Ok(RawHeader {
        width: u32::from(width),
        height: u32::from(height),
        bit_depth: Some(precision),
        has_alpha: Some(false),
        animated: false,
        orientation: None,
    })
}

pub(crate) fn embedded(b: &[u8]) -> Embedded<'_> {
    let (segs, _) = segments(b);
    let mut e = Embedded::default();
    let mut icc_parts: Vec<(u8, &[u8])> = Vec::new();
    let mut icc_count = 0u8;
    for s in &segs {
        match s.marker {
            0xE1 if e.exif.is_none() && s.data.starts_with(b"Exif\0") => {
                e.exif = exif_payload(s.data).map(Cow::Borrowed);
            }
            0xE1 if e.xmp.is_none() && s.data.starts_with(XMP_SIG) => {
                e.xmp = s.data.get(XMP_SIG.len()..).map(Cow::Borrowed);
            }
            0xE2 if s.data.starts_with(ICC_SIG) => {
                if let (Some(&seq), Some(&count), Some(part)) =
                    (s.data.get(12), s.data.get(13), s.data.get(14..))
                {
                    icc_count = count;
                    icc_parts.push((seq, part));
                }
            }
            0xED if e.iptc.is_none() && s.data.starts_with(PS_SIG) => {
                let irb = s.data.get(PS_SIG.len()..).unwrap_or(&[]);
                e.iptc = photoshop_resources(irb)
                    .into_iter()
                    .find(|(id, _)| *id == 0x0404)
                    .map(|(_, d)| Cow::Borrowed(d));
            }
            _ => {}
        }
    }
    e.icc = assemble_icc(icc_parts, icc_count);
    e
}

/// Join ICC chunks in sequence order; refuse a set with missing or repeated chunks.
fn assemble_icc(mut parts: Vec<(u8, &[u8])>, count: u8) -> Option<Cow<'_, [u8]>> {
    if parts.is_empty() || count == 0 || parts.len() != usize::from(count) {
        return None;
    }
    parts.sort_by_key(|(seq, _)| *seq);
    if parts
        .iter()
        .enumerate()
        .any(|(i, (seq, _))| usize::from(*seq) != i + 1)
    {
        return None;
    }
    if parts.len() == 1 {
        return parts.first().map(|(_, d)| Cow::Borrowed(*d));
    }
    let total: usize = parts.iter().map(|(_, d)| d.len()).sum();
    if total > MAX_BLOCK {
        return None;
    }
    let mut out = Vec::with_capacity(total);
    for (_, d) in parts {
        out.extend_from_slice(d);
    }
    Some(Cow::Owned(out))
}

/// Encode one marker segment (`FF`, marker, length, payload); `None` past the 65533-byte limit.
#[cfg(any(feature = "jpeg", test))]
pub(crate) fn segment(marker: u8, payload: &[u8]) -> Option<Vec<u8>> {
    let len = u16::try_from(payload.len() + 2).ok()?;
    let mut s = vec![0xFF, marker];
    s.extend_from_slice(&len.to_be_bytes());
    s.extend_from_slice(payload);
    Some(s)
}

#[cfg(test)]
pub(crate) mod testutil {
    /// A minimal JPEG byte layout: SOI, the given segments, SOF0 for `w×h`, SOS, EOI. Not
    /// decodable; enough for header and metadata tests.
    pub fn skeleton(w: u16, h: u16, segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        for (m, data) in segments {
            b.extend_from_slice(&super::segment(*m, data).unwrap());
        }
        let mut sof = vec![8];
        sof.extend_from_slice(&h.to_be_bytes());
        sof.extend_from_slice(&w.to_be_bytes());
        sof.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        b.extend_from_slice(&super::segment(0xC0, &sof).unwrap());
        b.extend_from_slice(&super::segment(0xDA, &[1, 1, 0, 0, 63, 0]).unwrap());
        b.extend_from_slice(&[0xFF, 0xD9]);
        b
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::skeleton;
    use super::*;

    #[test]
    fn header_reads_the_frame_after_app_segments() {
        let b = skeleton(
            640,
            480,
            &[(0xE0, b"JFIF\0\x01\x02\0\0\x01\0\x01\0\0".to_vec())],
        );
        let h = header(&b).unwrap();
        assert_eq!((h.width, h.height, h.bit_depth), (640, 480, Some(8)));
    }

    #[test]
    fn truncated_and_sizeless_frames_are_errors() {
        let b = skeleton(640, 480, &[]);
        assert!(header(&b[..10]).is_err());
        assert!(header(&skeleton(0, 480, &[])).is_err());
    }

    #[test]
    fn multi_chunk_icc_is_reassembled_in_order() {
        let chunk = |seq: u8, d: &[u8]| {
            let mut v = ICC_SIG.to_vec();
            v.extend_from_slice(&[seq, 2]);
            v.extend_from_slice(d);
            (0xE2, v)
        };
        let b = skeleton(1, 1, &[chunk(2, b"world"), chunk(1, b"hello ")]);
        assert_eq!(embedded(&b).icc.as_deref(), Some(&b"hello world"[..]));
        // A missing chunk drops the profile instead of returning half of it.
        let b = skeleton(1, 1, &[chunk(2, b"world")]);
        assert_eq!(embedded(&b).icc, None);
    }

    #[test]
    fn finds_exif_xmp_and_iptc() {
        let mut xmp = XMP_SIG.to_vec();
        xmp.extend_from_slice(b"<x:xmpmeta/>");
        let mut ps = PS_SIG.to_vec();
        ps.extend_from_slice(b"8BIM\x04\x04\0\0\0\0\0\x04\x1c\x02\x05\0");
        let b = skeleton(
            1,
            1,
            &[
                (0xE1, b"Exif\0\0II*\0\x08\0\0\0".to_vec()),
                (0xE1, xmp),
                (0xED, ps),
            ],
        );
        let e = embedded(&b);
        assert_eq!(e.exif.as_deref(), Some(&b"II*\0\x08\0\0\0"[..]));
        assert_eq!(e.xmp.as_deref(), Some(&b"<x:xmpmeta/>"[..]));
        assert_eq!(e.iptc.as_deref(), Some(&b"\x1c\x02\x05\0"[..]));
    }
}
