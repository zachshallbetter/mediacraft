//! HEIF (ISO/IEC 23008-12) and AVIF (AV1 Image File Format) stills: the item structure (`pitm`,
//! `iinf`, `iloc`, `iref`, `iprp`, `idat`), coded images decoded with `craft-hevc` and
//! `craft-av1`, `grid` derived images assembled from their tiles, alpha from auxiliary images,
//! and the EXIF, XMP and ICC metadata items. Transformative properties (`irot`, `imir`) are
//! reported by the header as the orientation and are not applied here.

use crate::bytes::{be_u16, be_u32};
use crate::container::{Embedded, exif_payload};
use crate::formats::bmff::{boxes, ipma_for};
use crate::yuv::{self, Plane};
use crate::{Error, Format, Limits, Result, Rgba8Image};
use std::borrow::Cow;

/// Largest single item payload we read.
const MAX_ITEM_BYTES: u64 = 256 << 20;
/// Most tiles a grid may name.
const MAX_TILES: usize = 4096;

struct Item {
    id: u32,
    ty: [u8; 4],
    content_type: String,
}

struct Location {
    /// 0: file offsets, 1: offsets into `idat`.
    method: u16,
    base: u64,
    extents: Vec<(u64, u64)>,
}

struct Reference {
    ty: [u8; 4],
    from: u32,
    to: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Nclx {
    matrix: u8,
    full_range: bool,
}

enum Prop<'a> {
    Hvcc(&'a [u8]),
    Av1c(&'a [u8]),
    Nclx(Nclx),
    Icc(&'a [u8]),
    AuxC(&'a [u8]),
    Other,
}

pub(crate) struct Heif<'a> {
    data: &'a [u8],
    format: Format,
    primary: u32,
    items: Vec<Item>,
    locations: Vec<(u32, Location)>,
    references: Vec<Reference>,
    idat: &'a [u8],
    props: Vec<Prop<'a>>,
    ipma: Vec<&'a [u8]>,
}

/// An unsigned big-endian integer of `size` bytes (0, 4 or 8; 0 reads as 0).
fn read_sized(b: &[u8], p: &mut usize, size: u8) -> Option<u64> {
    let v = match size {
        0 => 0,
        4 => u64::from(be_u32(b, *p)?),
        8 => (u64::from(be_u32(b, *p)?) << 32) | u64::from(be_u32(b, *p + 4)?),
        _ => return None,
    };
    *p += usize::from(size);
    Some(v)
}

fn cstr(b: &[u8]) -> (&[u8], &[u8]) {
    match b.iter().position(|&c| c == 0) {
        Some(n) => (&b[..n], b.get(n + 1..).unwrap_or(&[])),
        None => (b, &[]),
    }
}

impl<'a> Heif<'a> {
    pub(crate) fn parse(b: &'a [u8], format: Format) -> Result<Self> {
        let bad = |why: &str| Error::Malformed(format, why.into());
        let meta = boxes(b)
            .into_iter()
            .find(|(t, _)| t == b"meta")
            .map(|(_, d)| d)
            .ok_or_else(|| bad("no meta box"))?;
        let meta = meta.get(4..).ok_or_else(|| bad("meta box is cut short"))?;
        let mut h = Heif {
            data: b,
            format,
            primary: 0,
            items: Vec::new(),
            locations: Vec::new(),
            references: Vec::new(),
            idat: &[],
            props: Vec::new(),
            ipma: Vec::new(),
        };
        for (ty, d) in boxes(meta) {
            match &ty {
                b"pitm" => {
                    h.primary = match d.first() {
                        Some(0) => be_u16(d, 4).map(u32::from),
                        _ => be_u32(d, 4),
                    }
                    .ok_or_else(|| bad("pitm is cut short"))?;
                }
                b"iinf" => h.items = parse_iinf(d),
                b"iloc" => h.locations = parse_iloc(d).ok_or_else(|| bad("iloc is damaged"))?,
                b"iref" => h.references = parse_iref(d),
                b"idat" => h.idat = d,
                b"iprp" => {
                    for (t, p) in boxes(d) {
                        match &t {
                            b"ipco" => {
                                h.props = boxes(p)
                                    .into_iter()
                                    .map(|(t, p)| parse_prop(t, p))
                                    .collect()
                            }
                            b"ipma" => h.ipma.push(p),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if h.item(h.primary).is_none() {
            return Err(bad("the primary item is not described"));
        }
        Ok(h)
    }

    fn item(&self, id: u32) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    fn props(&self, id: u32) -> Vec<&Prop<'a>> {
        self.ipma
            .iter()
            .find_map(|d| ipma_for(d, id))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| i.checked_sub(1).and_then(|i| self.props.get(i)))
            .collect()
    }

    fn nclx(&self, id: u32) -> Option<Nclx> {
        self.props(id).into_iter().find_map(|p| {
            if let Prop::Nclx(n) = p {
                Some(*n)
            } else {
                None
            }
        })
    }

    fn icc(&self, id: u32) -> Option<&'a [u8]> {
        self.props(id)
            .into_iter()
            .find_map(|p| if let Prop::Icc(d) = p { Some(*d) } else { None })
    }

    /// Items `from` refers to with a reference of type `ty`, in order.
    fn refs_from(&self, ty: &[u8; 4], from: u32) -> Vec<u32> {
        self.references
            .iter()
            .filter(|r| &r.ty == ty && r.from == from)
            .flat_map(|r| r.to.iter().copied())
            .collect()
    }

    /// Items that refer to `to` with a reference of type `ty`.
    fn refs_to(&self, ty: &[u8; 4], to: u32) -> Vec<u32> {
        self.references
            .iter()
            .filter(|r| &r.ty == ty && r.to.contains(&to))
            .map(|r| r.from)
            .collect()
    }

    fn item_data(&self, id: u32) -> Result<Cow<'a, [u8]>> {
        let bad = |why: String| Error::Malformed(self.format, why);
        let loc = self
            .locations
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, l)| l)
            .ok_or_else(|| bad(format!("item {id} has no location")))?;
        let src = match loc.method {
            0 => self.data,
            1 => self.idat,
            m => {
                return Err(Error::Unsupported(
                    self.format,
                    if m == 2 {
                        "item-offset construction"
                    } else {
                        "unknown construction method"
                    },
                ));
            }
        };
        let mut parts: Vec<&'a [u8]> = Vec::new();
        let mut total = 0u64;
        for &(off, len) in &loc.extents {
            let start = loc
                .base
                .checked_add(off)
                .ok_or_else(|| bad("extent offset overflows".into()))?;
            let start =
                usize::try_from(start).map_err(|_| bad("extent offset overflows".into()))?;
            // A zero length means "to the end of the source".
            let end = if len == 0 {
                src.len()
            } else {
                start.saturating_add(usize::try_from(len).unwrap_or(usize::MAX))
            };
            let part = src
                .get(start..end)
                .ok_or_else(|| bad(format!("item {id} lies past the end of the file")))?;
            total += part.len() as u64;
            if total > MAX_ITEM_BYTES {
                return Err(bad(format!("item {id} is over {MAX_ITEM_BYTES} bytes")));
            }
            parts.push(part);
        }
        Ok(match parts.as_slice() {
            [one] => Cow::Borrowed(*one),
            _ => Cow::Owned(parts.concat()),
        })
    }

    /// The primary image as RGBA in stored orientation, and its ICC profile.
    pub(crate) fn decode(&self, limits: &Limits) -> Result<(Rgba8Image, Option<Vec<u8>>)> {
        let inherited = self.nclx(self.primary);
        let mut img = self.decode_item(self.primary, inherited, limits, 0)?;
        // Alpha: an auxiliary image of the primary whose type URN names alpha.
        for aux in self.refs_to(b"auxl", self.primary) {
            let is_alpha = self.props(aux).iter().any(|p| matches!(p, Prop::AuxC(urn) if cstr(urn).0.ends_with(b"auxid:1") || cstr(urn).0.ends_with(b"auxiliary:alpha")));
            if !is_alpha {
                continue;
            }
            if let Ok(a) = self.decode_item(aux, None, limits, 0)
                && (a.width(), a.height()) == (img.width(), img.height())
            {
                let alpha: Vec<u8> = a.data().as_chunks::<4>().0.iter().map(|p| p[0]).collect();
                for (px, a) in img.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(alpha) {
                    px[3] = a;
                }
            }
        }
        Ok((img, self.icc(self.primary).map(<[u8]>::to_vec)))
    }

    fn decode_item(
        &self,
        id: u32,
        inherited: Option<Nclx>,
        limits: &Limits,
        depth: u8,
    ) -> Result<Rgba8Image> {
        let item = self
            .item(id)
            .ok_or_else(|| Error::Malformed(self.format, format!("item {id} is not described")))?;
        let nclx = self.nclx(id).or(inherited);
        match &item.ty {
            b"grid" if depth == 0 => self.decode_grid(id, nclx, limits),
            b"hvc1" => {
                let hvcc = self.props(id).into_iter().find_map(|p| {
                    if let Prop::Hvcc(d) = p {
                        Some(*d)
                    } else {
                        None
                    }
                });
                let hvcc = hvcc.ok_or_else(|| {
                    Error::Malformed(self.format, "HEVC item without hvcC".into())
                })?;
                let data = self.item_data(id)?;
                let err = |e: craft_hevc::Error| Error::Decode(Format::Heif, e.to_string());
                let mut dec = craft_hevc::Decoder::from_hvcc(hvcc).map_err(err)?;
                let mut pics = dec.decode(&data, 0).map_err(err)?;
                pics.extend(dec.flush());
                let pic = pics
                    .first()
                    .ok_or_else(|| Error::Decode(Format::Heif, "no picture in the item".into()))?;
                limits.check_size(pic.width, pic.height)?;
                hevc_rgba(pic, nclx).ok_or_else(|| {
                    Error::Decode(Format::Heif, "inconsistent picture planes".into())
                })
            }
            b"av01" => {
                let av1c = self.props(id).into_iter().find_map(|p| {
                    if let Prop::Av1c(d) = p {
                        Some(*d)
                    } else {
                        None
                    }
                });
                let data = self.item_data(id)?;
                let err = |e: craft_av1::Error| Error::Decode(Format::Avif, e.to_string());
                let mut dec = craft_av1::Decoder::new();
                // av1C: four header bytes, then configuration OBUs (a sequence header).
                if let Some(obus) = av1c.and_then(|c| c.get(4..)).filter(|o| !o.is_empty()) {
                    dec.decode(obus).map_err(err)?;
                }
                let mut pics = dec.decode(&data).map_err(err)?;
                pics.extend(dec.flush());
                let pic = pics
                    .first()
                    .ok_or_else(|| Error::Decode(Format::Avif, "no picture in the item".into()))?;
                limits.check_size(pic.width, pic.height)?;
                av1_rgba(pic, nclx).ok_or_else(|| {
                    Error::Decode(Format::Avif, "inconsistent picture planes".into())
                })
            }
            b"grid" => Err(Error::Unsupported(self.format, "nested grids")),
            _ => Err(Error::Unsupported(
                self.format,
                "this item type is not decoded",
            )),
        }
    }

    /// A `grid` derived image: tiles (its `dimg` references, row-major) pasted onto a canvas of
    /// the grid's output size, clipped at the right and bottom edges.
    fn decode_grid(&self, id: u32, nclx: Option<Nclx>, limits: &Limits) -> Result<Rgba8Image> {
        let bad = |why: &str| Error::Malformed(self.format, why.into());
        let d = self.item_data(id)?;
        let (Some(&flags), Some(&rows), Some(&cols)) = (d.get(1), d.get(2), d.get(3)) else {
            return Err(bad("grid description is cut short"));
        };
        let (rows, cols) = (usize::from(rows) + 1, usize::from(cols) + 1);
        let (w, h) = if flags & 1 == 1 {
            (
                be_u32(&d, 4).ok_or_else(|| bad("grid size is cut short"))?,
                be_u32(&d, 8).ok_or_else(|| bad("grid size is cut short"))?,
            )
        } else {
            (
                u32::from(be_u16(&d, 4).ok_or_else(|| bad("grid size is cut short"))?),
                u32::from(be_u16(&d, 6).ok_or_else(|| bad("grid size is cut short"))?),
            )
        };
        limits.check_size(w, h)?;
        let tiles = self.refs_from(b"dimg", id);
        if tiles.len() != rows * cols || tiles.len() > MAX_TILES {
            return Err(bad("grid tile count does not match its rows and columns"));
        }
        let mut canvas = Rgba8Image::filled(w, h, [0, 0, 0, 255])?;
        let stride = w as usize * 4;
        for (k, tile) in tiles.into_iter().enumerate() {
            let t = self.decode_item(tile, nclx, limits, 1)?;
            let (x0, y0) = (
                (k % cols) as u64 * u64::from(t.width()),
                (k / cols) as u64 * u64::from(t.height()),
            );
            for ty in 0..t.height() as u64 {
                let (y, x0) = (y0 + ty, x0);
                if y >= u64::from(h) || x0 >= u64::from(w) {
                    continue;
                }
                let n = (u64::from(t.width())).min(u64::from(w) - x0) as usize * 4;
                let src = t
                    .data()
                    .get(ty as usize * t.width() as usize * 4..)
                    .and_then(|s| s.get(..n));
                let dst_at = y as usize * stride + x0 as usize * 4;
                if let (Some(src), Some(dst)) = (src, canvas.data_mut().get_mut(dst_at..dst_at + n))
                {
                    dst.copy_from_slice(src);
                }
            }
        }
        Ok(canvas)
    }

    /// EXIF and XMP items describing the primary image, and its ICC profile.
    pub(crate) fn embedded(&self) -> Embedded<'a> {
        let mut e = Embedded::default();
        let describes_primary = |id: u32| {
            let r = self.refs_from(b"cdsc", id);
            r.is_empty() || r.contains(&self.primary)
        };
        for item in self.items.iter().filter(|i| describes_primary(i.id)) {
            match &item.ty {
                // EXIF item: a 4-byte offset to the TIFF header, then the EXIF data.
                b"Exif" if e.exif.is_none() => {
                    if let Ok(d) = self.item_data(item.id)
                        && let Some(off) = be_u32(&d, 0)
                        && let Some(p) = d
                            .get(4usize.saturating_add(off as usize)..)
                            .and_then(exif_payload)
                    {
                        e.exif = Some(Cow::Owned(p.to_vec()));
                    }
                }
                b"mime" if e.xmp.is_none() && item.content_type == "application/rdf+xml" => {
                    e.xmp = self.item_data(item.id).ok();
                }
                _ => {}
            }
        }
        e.icc = self.icc(self.primary).map(Cow::Borrowed);
        e
    }
}

fn parse_iinf(d: &[u8]) -> Vec<Item> {
    let start = if d.first() == Some(&0) { 6 } else { 8 };
    let mut out = Vec::new();
    for (ty, e) in boxes(d.get(start..).unwrap_or(&[])) {
        if &ty != b"infe" {
            continue;
        }
        let version = e.first().copied().unwrap_or(0);
        let (id, rest) = match version {
            2 => (be_u16(e, 4).map(u32::from), e.get(8..)),
            3 => (be_u32(e, 4), e.get(10..)),
            _ => continue,
        };
        let (Some(id), Some(rest)) = (id, rest) else {
            continue;
        };
        let Some(item_ty) = rest.get(..4) else {
            continue;
        };
        let item_ty = [item_ty[0], item_ty[1], item_ty[2], item_ty[3]];
        let (_name, after) = cstr(rest.get(4..).unwrap_or(&[]));
        let content_type = if &item_ty == b"mime" {
            String::from_utf8_lossy(cstr(after).0).into_owned()
        } else {
            String::new()
        };
        out.push(Item {
            id,
            ty: item_ty,
            content_type,
        });
    }
    out
}

fn parse_iloc(d: &[u8]) -> Option<Vec<(u32, Location)>> {
    let version = *d.first()?;
    let (offset_size, length_size) = (d.get(4)? >> 4, d.get(4)? & 0x0F);
    let (base_size, index_size) = (
        d.get(5)? >> 4,
        if version >= 1 { d.get(5)? & 0x0F } else { 0 },
    );
    let mut p = 6usize;
    let count = if version < 2 {
        let v = u32::from(be_u16(d, p)?);
        p += 2;
        v
    } else {
        let v = be_u32(d, p)?;
        p += 4;
        v
    };
    let mut out = Vec::new();
    for _ in 0..count.min(1 << 16) {
        let id = if version < 2 {
            let v = u32::from(be_u16(d, p)?);
            p += 2;
            v
        } else {
            let v = be_u32(d, p)?;
            p += 4;
            v
        };
        let method = if version >= 1 {
            let v = be_u16(d, p)? & 0x0F;
            p += 2;
            v
        } else {
            0
        };
        p += 2; // data_reference_index
        let base = read_sized(d, &mut p, base_size)?;
        let n = be_u16(d, p)?;
        p += 2;
        let mut extents = Vec::with_capacity(usize::from(n).min(256));
        for _ in 0..n {
            if index_size > 0 {
                read_sized(d, &mut p, index_size)?;
            }
            let off = read_sized(d, &mut p, offset_size)?;
            let len = read_sized(d, &mut p, length_size)?;
            extents.push((off, len));
        }
        out.push((
            id,
            Location {
                method,
                base,
                extents,
            },
        ));
    }
    Some(out)
}

fn parse_iref(d: &[u8]) -> Vec<Reference> {
    let wide = d.first().is_some_and(|&v| v >= 1);
    let mut out = Vec::new();
    for (ty, r) in boxes(d.get(4..).unwrap_or(&[])) {
        let id = |p: &mut usize| -> Option<u32> {
            if wide {
                let v = be_u32(r, *p)?;
                *p += 4;
                Some(v)
            } else {
                let v = u32::from(be_u16(r, *p)?);
                *p += 2;
                Some(v)
            }
        };
        let mut p = 0usize;
        let Some(from) = id(&mut p) else { continue };
        let Some(n) = be_u16(r, p) else { continue };
        p += 2;
        let to: Vec<u32> = (0..n).map_while(|_| id(&mut p)).collect();
        out.push(Reference { ty, from, to });
    }
    out
}

fn parse_prop(ty: [u8; 4], d: &[u8]) -> Prop<'_> {
    match &ty {
        b"hvcC" => Prop::Hvcc(d),
        b"av1C" => Prop::Av1c(d),
        b"auxC" => Prop::AuxC(d.get(4..).unwrap_or(&[])),
        b"colr" => match d.get(..4) {
            Some(b"nclx") => match (be_u16(d, 8), d.get(10)) {
                (Some(m), Some(f)) => Prop::Nclx(Nclx {
                    matrix: u8::try_from(m).unwrap_or(2),
                    full_range: f & 0x80 != 0,
                }),
                _ => Prop::Other,
            },
            Some(b"prof" | b"rICC") => Prop::Icc(d.get(4..).unwrap_or(&[])),
            _ => Prop::Other,
        },
        _ => Prop::Other,
    }
}

fn plane<T>(data: &[T], stride: usize, width: u32, height: u32) -> Plane<'_, T> {
    Plane {
        data,
        stride,
        width,
        height,
    }
}

fn hevc_rgba(p: &craft_hevc::Picture, nclx: Option<Nclx>) -> Option<Rgba8Image> {
    let (matrix, full) = nclx.map_or((p.color.matrix, p.color.full_range), |n| {
        (n.matrix, n.full_range)
    });
    let bits = p.bit_depth;
    // Monochrome (4:0:0, as alpha images are): neutral chroma at half resolution.
    if p.chroma_width == 0 || p.u.is_empty() {
        let (cw, ch) = (p.width.div_ceil(2), p.height.div_ceil(2));
        let mid = 1u16 << (bits.clamp(1, 16) - 1);
        let neutral = vec![mid; cw as usize * ch as usize];
        let c = || plane(&neutral[..], cw as usize, cw, ch);
        return match (p.y.as_u8(), p.y.as_u16()) {
            (Some(y), _) => {
                let y16: Vec<u16> = y.iter().map(|&v| u16::from(v)).collect();
                yuv::to_rgba(
                    plane(&y16[..], p.y_stride, p.width, p.height),
                    c(),
                    c(),
                    None,
                    bits,
                    matrix,
                    full,
                )
            }
            (None, Some(y)) => yuv::to_rgba(
                plane(y, p.y_stride, p.width, p.height),
                c(),
                c(),
                None,
                bits,
                matrix,
                full,
            ),
            _ => None,
        };
    }
    let (cw, ch) = (p.chroma_width, p.chroma_height);
    match (p.y.as_u8(), p.u.as_u8(), p.v.as_u8()) {
        (Some(y), Some(u), Some(v)) => yuv::to_rgba(
            plane(y, p.y_stride, p.width, p.height),
            plane(u, p.uv_stride, cw, ch),
            plane(v, p.uv_stride, cw, ch),
            None,
            bits,
            matrix,
            full,
        ),
        _ => yuv::to_rgba(
            plane(p.y.as_u16()?, p.y_stride, p.width, p.height),
            plane(p.u.as_u16()?, p.uv_stride, cw, ch),
            plane(p.v.as_u16()?, p.uv_stride, cw, ch),
            None,
            bits,
            matrix,
            full,
        ),
    }
}

fn av1_rgba(p: &craft_av1::Picture, nclx: Option<Nclx>) -> Option<Rgba8Image> {
    let (matrix, full) = nclx.map_or((p.matrix_coefficients, p.full_range), |n| {
        (n.matrix, n.full_range)
    });
    let (w1, h1) = (p.plane_width(1), p.plane_height(1));
    let neutral;
    let (u, v): (&[u16], &[u16]) = if p.mono_chrome || p.planes[1].is_empty() {
        neutral = vec![1u16 << (p.bit_depth.max(1) - 1); w1 as usize * h1 as usize];
        (&neutral, &neutral)
    } else {
        (&p.planes[1], &p.planes[2])
    };
    let y = plane(&p.planes[0][..], p.width as usize, p.width, p.height);
    let bits = u32::from(p.bit_depth);
    if matrix == 0 && !p.mono_chrome {
        yuv::rgb_to_rgba(
            y,
            plane(u, w1 as usize, w1, h1),
            plane(v, w1 as usize, w1, h1),
            None,
            bits,
            full,
        )
    } else {
        yuv::to_rgba(
            y,
            plane(u, w1 as usize, w1, h1),
            plane(v, w1 as usize, w1, h1),
            None,
            bits,
            matrix,
            full,
        )
    }
}
