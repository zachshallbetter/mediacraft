//! Bounds-checked TIFF structure walking (TIFF 6.0 §2): the byte-order header, IFDs and their
//! entries. EXIF blobs, TIFF files and TIFF-structured camera raws all share this layout.
//! Classic TIFF only; BigTIFF headers are recognised by [`crate::sniff`] but not walked.

use crate::bytes::{be_u16, be_u32, clean_text, le_u16, le_u32};

/// Most entries one IFD may declare. Real files stay far below; a forged count would otherwise
/// make us walk megabytes of garbage.
const MAX_ENTRIES: usize = 4096;

/// A TIFF byte structure: the whole blob, offsets are relative to its start.
#[derive(Clone, Copy)]
pub(crate) struct Tiff<'a> {
    data: &'a [u8],
    le: bool,
}

/// One 12-byte IFD entry. `value_pos` is where the 4-byte value-or-offset field sits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    pub tag: u16,
    pub typ: u16,
    pub count: u32,
    pub value_pos: usize,
}

/// A parsed IFD and the offset of the next one (0 at the end of the chain).
pub(crate) struct Ifd {
    pub entries: Vec<Entry>,
    pub next: usize,
}

impl Ifd {
    pub fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|e| e.tag == tag)
    }
}

impl<'a> Tiff<'a> {
    /// A classic TIFF structure: `II*\0` or `MM\0*`.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        let le = match data.get(0..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let t = Tiff { data, le };
        (t.u16_at(2)? == 42).then_some(t)
    }

    /// A structure with an explicit byte order and no header check, for vendor blocks (Apple's
    /// maker note) that lay out IFDs like TIFF but start with their own signature.
    pub fn with_order(data: &'a [u8], little_endian: bool) -> Self {
        Tiff {
            data,
            le: little_endian,
        }
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn u16_at(&self, pos: usize) -> Option<u16> {
        if self.le {
            le_u16(self.data, pos)
        } else {
            be_u16(self.data, pos)
        }
    }

    pub fn u32_at(&self, pos: usize) -> Option<u32> {
        if self.le {
            le_u32(self.data, pos)
        } else {
            be_u32(self.data, pos)
        }
    }

    pub fn first_ifd(&self) -> Option<usize> {
        Some(self.u32_at(4)? as usize)
    }

    pub fn ifd(&self, pos: usize) -> Option<Ifd> {
        let n = self.u16_at(pos)? as usize;
        if n == 0 || n > MAX_ENTRIES {
            return None;
        }
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let e = pos.checked_add(2)?.checked_add(i.checked_mul(12)?)?;
            entries.push(Entry {
                tag: self.u16_at(e)?,
                typ: self.u16_at(e + 2)?,
                count: self.u32_at(e + 4)?,
                value_pos: e + 8,
            });
        }
        let next = self.u32_at(pos + 2 + n * 12).unwrap_or(0) as usize;
        Some(Ifd { entries, next })
    }

    /// The raw value bytes of an entry, inline or at its offset.
    pub fn bytes(&self, e: &Entry) -> Option<&'a [u8]> {
        let size: usize = match e.typ {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 => 8,
            _ => return None,
        };
        let len = size.checked_mul(e.count as usize)?;
        let start = if len <= 4 {
            e.value_pos
        } else {
            self.u32_at(e.value_pos)? as usize
        };
        self.data.get(start..start.checked_add(len)?)
    }

    /// The first value of an unsigned integer entry (BYTE, SHORT, LONG, IFD).
    pub fn uint(&self, e: &Entry) -> Option<u32> {
        if e.count == 0 {
            return None;
        }
        match e.typ {
            1 | 7 => self.data.get(e.value_pos).map(|&b| u32::from(b)),
            3 => self.u16_at(e.value_pos).map(u32::from),
            4 | 13 => self.u32_at(e.value_pos),
            _ => None,
        }
    }

    /// The `i`-th value of an unsigned or signed integer entry.
    pub fn uint_at(&self, e: &Entry, i: usize) -> Option<u32> {
        let b = self.bytes(e)?;
        match e.typ {
            1 | 7 => b.get(i).map(|&v| u32::from(v)),
            3 => {
                let off = i.checked_mul(2)?;
                let s = b.get(off..off + 2)?;
                Some(u32::from(if self.le {
                    u16::from_le_bytes([s[0], s[1]])
                } else {
                    u16::from_be_bytes([s[0], s[1]])
                }))
            }
            4 | 13 => {
                let off = i.checked_mul(4)?;
                let s = b.get(off..off + 4)?;
                let a = [s[0], s[1], s[2], s[3]];
                Some(if self.le {
                    u32::from_le_bytes(a)
                } else {
                    u32::from_be_bytes(a)
                })
            }
            _ => None,
        }
    }

    /// The `i`-th RATIONAL or SRATIONAL value; `None` for a zero denominator or a non-finite result.
    pub fn rational(&self, e: &Entry, i: usize) -> Option<f64> {
        let b = self.bytes(e)?;
        let off = i.checked_mul(8)?;
        let s = b.get(off..off + 8)?;
        let word = |w: &[u8]| {
            let a = [w[0], w[1], w[2], w[3]];
            if self.le {
                u32::from_le_bytes(a)
            } else {
                u32::from_be_bytes(a)
            }
        };
        let (n, d) = (word(&s[0..4]), word(&s[4..8]));
        let v = match e.typ {
            5 => f64::from(n) / f64::from(d),
            10 => f64::from(n as i32) / f64::from(d as i32),
            _ => return None,
        };
        (d != 0 && v.is_finite()).then_some(v)
    }

    /// An ASCII (or UNDEFINED text) entry as cleaned text.
    pub fn text(&self, e: &Entry) -> Option<String> {
        match e.typ {
            2 | 7 | 1 => clean_text(self.bytes(e)?),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! A tiny little-endian TIFF writer for building fixtures.

    /// One entry value.
    pub enum Val {
        Short(u16),
        Long(u32),
        Ascii(&'static str),
        Rational(Vec<(u32, u32)>),
        SRational(i32, i32),
        Bytes(Vec<u8>),
    }

    /// Build `II*\0` + IFD0 (`ifd0`) with optional sub-IFDs linked by pointer tags, laid out
    /// sequentially. Returns the blob. Pointer tags (0x8769 Exif, 0x8825 GPS) get their offset
    /// patched to the sub-IFD that follows.
    pub fn build(ifd0: Vec<(u16, Val)>, subs: Vec<(u16, Vec<(u16, Val)>)>) -> Vec<u8> {
        let mut out = b"II*\0\x08\0\0\0".to_vec();
        let mut ifds: Vec<Vec<(u16, Val)>> = vec![ifd0];
        let mut pointers = Vec::new();
        for (tag, entries) in subs {
            ifds[0].push((tag, Val::Long(0)));
            pointers.push(tag);
            ifds.push(entries);
        }
        ifds[0].sort_by_key(|(t, _)| *t);
        let mut ifd_offsets = Vec::new();
        let mut patch: Vec<(usize, usize)> = Vec::new(); // (where, ifd index)
        for (idx, ifd) in ifds.iter().enumerate() {
            let start = out.len();
            ifd_offsets.push(start);
            let n = ifd.len();
            out.extend_from_slice(&(n as u16).to_le_bytes());
            let data_start = start + 2 + n * 12 + 4;
            let mut data: Vec<u8> = Vec::new();
            for (tag, v) in ifd {
                let (typ, count, bytes): (u16, u32, Vec<u8>) = match v {
                    Val::Short(s) => (3, 1, s.to_le_bytes().to_vec()),
                    Val::Long(l) => (4, 1, l.to_le_bytes().to_vec()),
                    Val::Ascii(s) => {
                        let mut b = s.as_bytes().to_vec();
                        b.push(0);
                        (2, b.len() as u32, b)
                    }
                    Val::Rational(rs) => {
                        let mut b = Vec::new();
                        for (n, d) in rs {
                            b.extend_from_slice(&n.to_le_bytes());
                            b.extend_from_slice(&d.to_le_bytes());
                        }
                        (5, rs.len() as u32, b)
                    }
                    Val::SRational(n, d) => {
                        let mut b = n.to_le_bytes().to_vec();
                        b.extend_from_slice(&d.to_le_bytes());
                        (10, 1, b)
                    }
                    Val::Bytes(b) => (7, b.len() as u32, b.clone()),
                };
                out.extend_from_slice(&tag.to_le_bytes());
                out.extend_from_slice(&typ.to_le_bytes());
                out.extend_from_slice(&count.to_le_bytes());
                if idx == 0 && pointers.contains(tag) {
                    let sub = pointers.iter().position(|p| p == tag).unwrap_or(0) + 1;
                    patch.push((out.len(), sub));
                    out.extend_from_slice(&[0; 4]);
                } else if bytes.len() <= 4 {
                    let mut b = bytes.clone();
                    b.resize(4, 0);
                    out.extend_from_slice(&b);
                } else {
                    let off = (data_start + data.len()) as u32;
                    out.extend_from_slice(&off.to_le_bytes());
                    data.extend_from_slice(&bytes);
                    if data.len() % 2 == 1 {
                        data.push(0);
                    }
                }
            }
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&data);
        }
        for (at, sub) in patch {
            let off = ifd_offsets[sub] as u32;
            out[at..at + 4].copy_from_slice(&off.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::{Val, build};
    use super::*;

    #[test]
    fn walks_entries_values_and_sub_ifds() {
        let blob = build(
            vec![(0x010f, Val::Ascii("Maker")), (0x0112, Val::Short(6))],
            vec![(
                0x8769,
                vec![
                    (0x829a, Val::Rational(vec![(1, 250)])),
                    (0x9204, Val::SRational(-1, 3)),
                ],
            )],
        );
        let t = Tiff::new(&blob).unwrap();
        let ifd0 = t.ifd(t.first_ifd().unwrap()).unwrap();
        assert_eq!(t.text(ifd0.get(0x010f).unwrap()).as_deref(), Some("Maker"));
        assert_eq!(t.uint(ifd0.get(0x0112).unwrap()), Some(6));
        let exif = t
            .ifd(t.uint(ifd0.get(0x8769).unwrap()).unwrap() as usize)
            .unwrap();
        assert_eq!(t.rational(exif.get(0x829a).unwrap(), 0), Some(0.004));
        let bias = t.rational(exif.get(0x9204).unwrap(), 0).unwrap();
        assert!((bias + 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn refuses_bad_magic_and_wild_offsets() {
        assert!(Tiff::new(b"II+\0\x08\0\0\0").is_none());
        assert!(Tiff::new(b"XX*\0").is_none());
        let mut blob = build(vec![(0x010f, Val::Ascii("A long make string"))], vec![]);
        // Point the string's offset past the end.
        let len = blob.len();
        let t = Tiff::new(&blob).unwrap();
        let pos = t.ifd(8).unwrap().get(0x010f).unwrap().value_pos;
        blob[pos..pos + 4].copy_from_slice(&(len as u32 + 100).to_le_bytes());
        let t = Tiff::new(&blob).unwrap();
        assert_eq!(t.text(t.ifd(8).unwrap().get(0x010f).unwrap()), None);
    }

    #[test]
    fn zero_denominator_is_none() {
        let blob = build(vec![(0x829d, Val::Rational(vec![(28, 0)]))], vec![]);
        let t = Tiff::new(&blob).unwrap();
        assert_eq!(t.rational(t.ifd(8).unwrap().get(0x829d).unwrap(), 0), None);
    }
}
