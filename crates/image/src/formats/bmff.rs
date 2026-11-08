//! HEIF and AVIF (ISO/IEC 23008-12, ISO/IEC 14496-12): the primary item's spatial extent
//! (`ispe`), rotation (`irot`), mirroring (`imir`) and bit depth (`pixi`), read from the `meta`
//! box without decoding the coded image.

use super::RawHeader;
use crate::bytes::{be_u16, be_u32};
use crate::{Error, Format, Orientation, Result};

/// Most boxes visited inside `meta`; real files have dozens.
const MAX_BOXES: usize = 4096;

/// `(type, payload)` for each box in `b`.
fn boxes(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while out.len() < MAX_BOXES {
        let (Some(size32), Some(ty)) = (be_u32(b, p), b.get(p + 4..p + 8)) else {
            break;
        };
        let (header, size) = match size32 {
            0 => (8, b.len() - p),
            1 => {
                let Some(hi) = be_u32(b, p + 8) else { break };
                let Some(lo) = be_u32(b, p + 12) else { break };
                let Ok(size) = usize::try_from((u64::from(hi) << 32) | u64::from(lo)) else {
                    break;
                };
                (16, size)
            }
            n => (8, n as usize),
        };
        if size < header {
            break;
        }
        let Some(end) = p.checked_add(size) else {
            break;
        };
        let Some(payload) = b.get(p + header..end) else {
            break;
        };
        out.push(([ty[0], ty[1], ty[2], ty[3]], payload));
        p = end;
    }
    out
}

/// A property from `ipco`, by 1-based index.
#[derive(Clone, Copy)]
enum Prop {
    Ispe(u32, u32),
    Irot(u8),
    Imir(u8),
    Pixi(u8),
    Other,
}

pub(crate) fn header(b: &[u8], format: Format) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(format, why.into());
    let top = boxes(b);
    let meta = top
        .iter()
        .find(|(t, _)| t == b"meta")
        .map(|(_, d)| *d)
        .ok_or_else(|| bad("no meta box"))?;
    let meta = meta.get(4..).ok_or_else(|| bad("meta box is cut short"))?; // FullBox header
    let children = boxes(meta);

    let primary = children
        .iter()
        .find(|(t, _)| t == b"pitm")
        .and_then(|(_, d)| match d.first() {
            Some(0) => be_u16(d, 4).map(u32::from),
            Some(_) => be_u32(d, 4),
            None => None,
        })
        .ok_or_else(|| bad("no primary item"))?;

    let iprp = children
        .iter()
        .find(|(t, _)| t == b"iprp")
        .map(|(_, d)| *d)
        .ok_or_else(|| bad("no item properties"))?;
    let iprp = boxes(iprp);
    let ipco = iprp
        .iter()
        .find(|(t, _)| t == b"ipco")
        .map(|(_, d)| *d)
        .ok_or_else(|| bad("no property container"))?;
    let props: Vec<Prop> = boxes(ipco)
        .into_iter()
        .map(|(t, d)| match &t {
            b"ispe" => match (be_u32(d, 4), be_u32(d, 8)) {
                (Some(w), Some(h)) => Prop::Ispe(w, h),
                _ => Prop::Other,
            },
            b"irot" => d.first().map_or(Prop::Other, |a| Prop::Irot(a & 3)),
            b"imir" => d.first().map_or(Prop::Other, |a| Prop::Imir(a & 1)),
            b"pixi" => d.get(5).map_or(Prop::Other, |&bits| Prop::Pixi(bits)),
            _ => Prop::Other,
        })
        .collect();

    let assoc = iprp
        .iter()
        .filter(|(t, _)| t == b"ipma")
        .find_map(|(_, d)| ipma_for(d, primary))
        .ok_or_else(|| bad("primary item has no properties"))?;

    let mut size = None;
    let mut bits = None;
    let mut orientation = Orientation::Identity;
    // Transformative properties apply in association order (ISO/IEC 23008-12 §6.5.1).
    for idx in assoc {
        match idx.checked_sub(1).and_then(|i| props.get(i)) {
            Some(Prop::Ispe(w, h)) => size = Some((*w, *h)),
            Some(Prop::Pixi(b)) => bits = Some(*b),
            // irot turns anti-clockwise by 90° steps.
            Some(Prop::Irot(a)) => {
                let step = match a {
                    1 => Orientation::Rotate270,
                    2 => Orientation::Rotate180,
                    3 => Orientation::Rotate90,
                    _ => Orientation::Identity,
                };
                orientation = orientation.then(step);
            }
            // axis 0: mirror about the vertical axis (left↔right); 1: about the horizontal axis.
            Some(Prop::Imir(axis)) => {
                let step = if *axis == 0 {
                    Orientation::MirrorHorizontal
                } else {
                    Orientation::MirrorVertical
                };
                orientation = orientation.then(step);
            }
            _ => {}
        }
    }
    let (width, height) = size
        .filter(|(w, h)| *w > 0 && *h > 0)
        .ok_or_else(|| bad("primary item has no spatial extent"))?;
    Ok(RawHeader {
        width,
        height,
        bit_depth: bits,
        has_alpha: None,
        animated: false,
        orientation: Some(orientation),
    })
}

/// Property indices (1-based) associated with `item` in an `ipma` box.
fn ipma_for(d: &[u8], item: u32) -> Option<Vec<usize>> {
    let version = *d.first()?;
    let flags = be_u32(d, 0)? & 0x00FF_FFFF;
    let count = be_u32(d, 4)?;
    let mut p = 8usize;
    for _ in 0..count.min(100_000) {
        let id = if version < 1 {
            let v = u32::from(be_u16(d, p)?);
            p += 2;
            v
        } else {
            let v = be_u32(d, p)?;
            p += 4;
            v
        };
        let n = usize::from(*d.get(p)?);
        p += 1;
        let mut idx = Vec::with_capacity(n);
        for _ in 0..n {
            if flags & 1 == 1 {
                idx.push(usize::from(be_u16(d, p)? & 0x7FFF));
                p += 2;
            } else {
                idx.push(usize::from(*d.get(p)? & 0x7F));
                p += 1;
            }
        }
        if id == item {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(ty: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(ty);
        b.extend_from_slice(payload);
        b
    }

    fn heif(props: Vec<Vec<u8>>, assoc: &[u8]) -> Vec<u8> {
        let mut ipco = Vec::new();
        for p in props {
            ipco.extend_from_slice(&p);
        }
        let mut ipma = vec![0, 0, 0, 0];
        ipma.extend_from_slice(&1u32.to_be_bytes());
        ipma.extend_from_slice(&1u16.to_be_bytes()); // item 1
        ipma.push(assoc.len() as u8);
        ipma.extend_from_slice(assoc);
        let mut iprp = bx(b"ipco", &ipco);
        iprp.extend_from_slice(&bx(b"ipma", &ipma));
        let mut meta = vec![0, 0, 0, 0];
        meta.extend_from_slice(&bx(b"pitm", &[0, 0, 0, 0, 0, 1]));
        meta.extend_from_slice(&bx(b"iprp", &iprp));
        let mut b = bx(b"ftyp", b"heic\0\0\0\0mif1heic");
        b.extend_from_slice(&bx(b"meta", &meta));
        b
    }

    fn ispe(w: u32, h: u32) -> Vec<u8> {
        let mut d = vec![0, 0, 0, 0];
        d.extend_from_slice(&w.to_be_bytes());
        d.extend_from_slice(&h.to_be_bytes());
        bx(b"ispe", &d)
    }

    #[test]
    fn primary_extent_and_rotation() {
        // A tile-sized ispe first, the grid's full size second, and irot = 3 (270° anti-clockwise).
        let b = heif(
            vec![ispe(512, 512), ispe(4032, 3024), bx(b"irot", &[3])],
            &[0x82, 0x83],
        );
        let h = header(&b, Format::Heif).unwrap();
        assert_eq!((h.width, h.height), (4032, 3024));
        assert_eq!(h.orientation, Some(Orientation::Rotate90));
    }

    #[test]
    fn missing_pieces_are_errors() {
        let b = heif(vec![bx(b"irot", &[1])], &[0x81]);
        assert!(header(&b, Format::Heif).is_err());
        assert!(header(b"\0\0\0\x10ftypheic\0\0\0\0", Format::Heif).is_err());
    }
}
