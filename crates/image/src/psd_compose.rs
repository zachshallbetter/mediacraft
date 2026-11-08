//! Interim layer compositor for PSD/PSB (`psd` feature), for files whose saved composite is
//! missing or a placeholder (saved without "maximise compatibility").
//!
//! Clean-room, over [`craft_psd`]'s layer records. Blend formulas follow the W3C "Compositing
//! and Blending Level 1" specification and public descriptions of Photoshop's extra modes. It is
//! a stop-gap until compositing moves to a shared craft-libs crate.
//!
//! # What is drawn
//! Each visible pixel layer's stored raster, placed at its bounds, with opacity, fill opacity
//! (multiplied into opacity: no layer effects are drawn, so the two coincide for the pixels we
//! draw), a user layer mask (density, default colour, invert, disabled flag), clipping groups,
//! and all 27 blend modes. Groups are pass-through or isolated, with group opacity and mask.
//! Text and smart-object layers draw their stored raster.
//!
//! # What is not drawn (counted in [`CompositeReport`])
//! Vector masks, adjustment layers, fill layers (solid, gradient, pattern) and layer effects.
//! Mask feather is skipped. Dissolve is treated as normal (it is a random-dither mode).
//! Unknown blend keys fall back to normal.
//!
//! # Colour
//! RGB, grayscale and CMYK at 8, 16 and 32 bits, converted per layer by `craft-psd` exactly as
//! for the saved composite (CMYK is the same naive ink conversion, with no ICC transform, so the
//! two paths agree). Arithmetic is `f32` on straight (non-premultiplied) RGBA in the file's own
//! colour space. Bitmap, indexed, Lab, multichannel and duotone are refused with
//! [`Error::Unsupported`], and the caller falls back to the saved composite.
//!
//! # Soft light
//! Uses Photoshop's published form: `(1-2s)·b² + 2s·b` for `s <= 0.5`, else
//! `2b(1-s) + sqrt(b)(2s-1)`. W3C is the same except that for a backdrop `b <= 0.25` it replaces
//! `sqrt(b)` with the polynomial `((16b-12)b+4)b`, so dark backdrops differ slightly.
//!
//! # Limits
//! The canvas, and every group surface alive at once, is charged against
//! [`Limits::max_alloc`] (16 bytes per pixel) before it is allocated. Layer bounds may be huge or
//! negative: a layer outside the canvas is skipped without decoding, and one whose stored
//! raster alone would exceed the budget is skipped and counted. Group nesting is capped.

use crate::{Error, Format, Limits, Result, Rgba8Image};
use craft_psd::layer::{LayerMask, LayerRecord, Rect};
use craft_psd::tagged::SectionType;
use craft_psd::{BlendMode, ColorMode, GrayImage, PsdFile};

const MAX_DEPTH: usize = 48;
const BYTES_PER_PX: u64 = 16;

/// What the layer compositor drew and what it left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompositeReport {
    /// Pixel layers drawn.
    pub layers_drawn: u32,
    /// Groups composited.
    pub groups: u32,
    /// Layers and groups skipped because they (or an enclosing group) are hidden.
    pub hidden_skipped: u32,
    /// Layers clipped to the layer below.
    pub clipped_layers: u32,
    /// Layer masks applied.
    pub masks_applied: u32,
    /// Layer masks present but disabled, so not applied.
    pub masks_disabled: u32,
    /// Masks that could not be decoded and were ignored.
    pub masks_unreadable: u32,
    /// Dissolve layers drawn as normal.
    pub dissolve_as_normal: u32,
    /// Layers with a blend key this compositor does not know, drawn as normal.
    pub unknown_blend_modes: u32,
    /// Vector masks present and not applied.
    pub vector_masks_ignored: u32,
    /// Adjustment layers not drawn.
    pub adjustment_layers_ignored: u32,
    /// Solid, gradient and pattern fill layers not drawn.
    pub fill_layers_ignored: u32,
    /// Layers with layer effects (shadows, strokes, overlays...) drawn without them.
    pub layer_effects_ignored: u32,
    /// Text layers drawn from their stored raster.
    pub text_layers_raster: u32,
    /// Smart-object layers drawn from their stored raster.
    pub smart_objects_raster: u32,
    /// Layers whose pixels could not be decoded and were skipped.
    pub layers_unreadable: u32,
    /// Layers skipped because their stored raster would exceed the allocation limit.
    pub layers_oversized: u32,
}

const ADJUSTMENT_KEYS: [&[u8; 4]; 17] = [
    b"brit", b"levl", b"curv", b"expA", b"vibA", b"hue ", b"hue2", b"blnc", b"blwh", b"phfl",
    b"mixr", b"clrL", b"thrs", b"nvrt", b"post", b"selc", b"grdm",
];
const FILL_KEYS: [&[u8; 4]; 3] = [b"SoCo", b"GdFl", b"PtFl"];
const EFFECT_KEYS: [&[u8; 4]; 3] = [b"lrFX", b"lfx2", b"lmfx"];
const VECTOR_MASK_KEYS: [&[u8; 4]; 2] = [b"vmsk", b"vsms"];
const TEXT_KEYS: [&[u8; 4]; 2] = [b"TySh", b"tySh"];
const SMART_KEYS: [&[u8; 4]; 3] = [b"SoLd", b"PlLd", b"SoLE"];

/// Composite a PSD/PSB from its layers, ignoring the saved composite.
///
/// Returns straight-alpha RGBA8 at the document size and a report of what was left out.
pub fn composite_layers(bytes: &[u8], limits: &Limits) -> Result<(Rgba8Image, CompositeReport)> {
    limits.check_input(bytes.len())?;
    let file =
        PsdFile::from_bytes(bytes).map_err(|e| Error::Malformed(Format::Psd, e.to_string()))?;
    composite_file(&file, limits)
}

pub(crate) fn composite_file(
    file: &PsdFile,
    limits: &Limits,
) -> Result<(Rgba8Image, CompositeReport)> {
    let h = &file.header;
    limits.check_size(h.width, h.height)?;
    match h.color_mode {
        ColorMode::Rgb | ColorMode::Grayscale | ColorMode::Cmyk => {}
        _ => {
            return Err(Error::Unsupported(
                Format::Psd,
                "layer compositing needs RGB, grayscale or CMYK",
            ));
        }
    }
    if file.layers().is_empty() {
        return Err(Error::Unsupported(Format::Psd, "the file has no layers"));
    }
    let (w, hh) = (h.width as usize, h.height as usize);
    let mut ctx = Ctx {
        file,
        limits,
        live: 0,
        canvas: (w, hh),
        report: CompositeReport::default(),
    };
    let tree = build_tree(file.layers())?;
    let mut canvas = ctx.surface(Rect::from_xywh(0, 0, h.width, h.height))?;
    ctx.render(&tree, &mut canvas, 0)?;
    let mut out = Vec::new();
    out.try_reserve_exact(w * hh * 4)
        .map_err(|_| Error::Decode(Format::Psd, "out of memory".into()))?;
    for p in &canvas.px {
        let a = p[3];
        if a <= 0.0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            out.extend_from_slice(&[to_u8(p[0]), to_u8(p[1]), to_u8(p[2]), to_u8(a)]);
        }
    }
    let report = ctx.report;
    Ok((Rgba8Image::new(h.width, h.height, out)?, report))
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

enum Node {
    Layer(usize),
    Group { folder: usize, kids: Vec<Node> },
}

/// Layers are stored bottom-first: a group is a bounding divider, its children, then the folder
/// record that carries its name, mode and opacity.
fn build_tree(layers: &[LayerRecord]) -> Result<Vec<Node>> {
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    for (i, rec) in layers.iter().enumerate() {
        match rec.section_type() {
            SectionType::BoundingDivider => {
                if stack.len() > MAX_DEPTH {
                    return Err(Error::Malformed(
                        Format::Psd,
                        "layer groups nest too deeply".into(),
                    ));
                }
                stack.push(Vec::new());
            }
            SectionType::OpenFolder | SectionType::ClosedFolder if stack.len() > 1 => {
                let kids = stack.pop().unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    top.push(Node::Group { folder: i, kids });
                }
            }
            _ => {
                if let Some(top) = stack.last_mut() {
                    top.push(Node::Layer(i));
                }
            }
        }
    }
    // A divider with no folder record: keep its children in the parent.
    while stack.len() > 1 {
        let kids = stack.pop().unwrap_or_default();
        if let Some(top) = stack.last_mut() {
            top.extend(kids);
        }
    }
    Ok(stack.pop().unwrap_or_default())
}

/// A rectangle of straight-alpha `f32` RGBA.
struct Surf {
    left: i32,
    top: i32,
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Surf {
    fn bytes(&self) -> u64 {
        self.px.len() as u64 * BYTES_PER_PX
    }
}

struct Ctx<'a> {
    file: &'a PsdFile,
    limits: &'a Limits,
    /// Bytes of live surfaces.
    live: u64,
    canvas: (usize, usize),
    report: CompositeReport,
}

impl<'a> Ctx<'a> {
    /// A transparent surface for `rect`, charged against the allocation limit first.
    fn surface(&mut self, rect: Rect) -> Result<Surf> {
        let (w, h) = rect
            .size()
            .map_err(|e| Error::Malformed(Format::Psd, e.to_string()))?;
        let bytes = (w as u64)
            .saturating_mul(h as u64)
            .saturating_mul(BYTES_PER_PX);
        if self.live.saturating_add(bytes) > self.limits.max_alloc {
            return Err(Error::TooLarge {
                width: self.file.header.width,
                height: self.file.header.height,
                max_pixels: self.limits.max_pixels,
            });
        }
        let mut px = Vec::new();
        px.try_reserve_exact(w * h)
            .map_err(|_| Error::Decode(Format::Psd, "out of memory".into()))?;
        px.resize(w * h, [0.0; 4]);
        self.live += bytes;
        Ok(Surf {
            left: rect.left,
            top: rect.top,
            w,
            h,
            px,
        })
    }

    fn free(&mut self, s: Surf) {
        self.live = self.live.saturating_sub(s.bytes());
    }

    fn canvas_rect(&self) -> Rect {
        Rect::from_xywh(0, 0, self.canvas.0 as u32, self.canvas.1 as u32)
    }

    /// Walk siblings bottom to top, handling clipping runs.
    fn render(&mut self, nodes: &[Node], dst: &mut Surf, depth: usize) -> Result<()> {
        let mut i = 0;
        while i < nodes.len() {
            let mut j = i + 1;
            while j < nodes.len() && self.clipping(&nodes[j]) != 0 {
                j += 1;
            }
            self.render_unit(&nodes[i], &nodes[i + 1..j], dst, depth)?;
            i = j;
        }
        Ok(())
    }

    fn idx(n: &Node) -> usize {
        match n {
            Node::Layer(i) | Node::Group { folder: i, .. } => *i,
        }
    }

    fn rec(&self, n: &Node) -> Option<&'a LayerRecord> {
        self.file.layers().get(Self::idx(n))
    }

    fn clipping(&self, n: &Node) -> u8 {
        self.rec(n).map_or(0, |r| r.clipping)
    }

    /// Opacity × fill opacity.
    fn alpha_of(rec: &LayerRecord) -> f32 {
        f32::from(rec.opacity) / 255.0 * f32::from(rec.fill_opacity()) / 255.0
    }

    fn mode_of(&mut self, node: &Node) -> BlendMode {
        let Some(rec) = self.rec(node) else {
            return BlendMode::Normal;
        };
        let m = match node {
            Node::Group { .. } => rec
                .section_divider()
                .and_then(|d| d.blend_mode)
                .unwrap_or(rec.blend_mode),
            Node::Layer(_) => rec.blend_mode,
        };
        match m {
            BlendMode::Dissolve => {
                self.report.dissolve_as_normal += 1;
                BlendMode::Normal
            }
            BlendMode::Unknown(_) => {
                self.report.unknown_blend_modes += 1;
                BlendMode::Normal
            }
            m => m,
        }
    }

    /// Count the features of a layer that is about to be drawn.
    fn tally(&mut self, rec: &LayerRecord) {
        let has = |keys: &[&[u8; 4]]| keys.iter().any(|k| rec.block(k).is_some());
        let r = &mut self.report;
        r.vector_masks_ignored += u32::from(has(&VECTOR_MASK_KEYS));
        r.layer_effects_ignored += u32::from(has(&EFFECT_KEYS));
        r.text_layers_raster += u32::from(has(&TEXT_KEYS));
        r.smart_objects_raster += u32::from(has(&SMART_KEYS));
    }

    fn render_unit(
        &mut self,
        base: &Node,
        clipped: &[Node],
        dst: &mut Surf,
        depth: usize,
    ) -> Result<()> {
        let Some(rec) = self.rec(base) else {
            return Ok(());
        };
        if !rec.is_visible() {
            self.report.hidden_skipped += 1 + clipped.len() as u32;
            return Ok(());
        }
        let has = |keys: &[&[u8; 4]]| keys.iter().any(|k| rec.block(k).is_some());
        if matches!(base, Node::Layer(_)) {
            if has(&ADJUSTMENT_KEYS) {
                self.report.adjustment_layers_ignored += 1;
                return Ok(());
            }
            if has(&FILL_KEYS) {
                self.report.fill_layers_ignored += 1;
                return Ok(());
            }
        }
        self.tally(rec);
        let mode = self.mode_of(base);
        let opacity = Self::alpha_of(rec);
        let pass = matches!(base, Node::Group { .. }) && mode == BlendMode::PassThrough;

        // The unit's own surface: the layer raster, or an isolated render of the group.
        let mut unit = match base {
            Node::Layer(_) => match self.layer_surface(Self::idx(base))? {
                Some(s) => s,
                None => return Ok(()),
            },
            Node::Group { kids, .. } => {
                self.report.groups += 1;
                if pass && clipped.is_empty() {
                    return self.pass_through(Self::idx(base), kids, opacity, dst, depth);
                }
                if depth >= MAX_DEPTH {
                    return Err(Error::Malformed(
                        Format::Psd,
                        "layer groups nest too deeply".into(),
                    ));
                }
                let mut s = self.surface(self.canvas_rect())?;
                self.render(kids, &mut s, depth + 1)?;
                s
            }
        };
        self.apply_mask(Self::idx(base), &mut unit);
        if matches!(base, Node::Layer(_)) {
            self.report.layers_drawn += 1;
        }
        for c in clipped {
            let Some(crec) = self.rec(c) else { continue };
            if !crec.is_visible() {
                self.report.hidden_skipped += 1;
                continue;
            }
            let chas = |keys: &[&[u8; 4]]| keys.iter().any(|k| crec.block(k).is_some());
            if matches!(c, Node::Layer(_)) && (chas(&ADJUSTMENT_KEYS) || chas(&FILL_KEYS)) {
                if chas(&FILL_KEYS) {
                    self.report.fill_layers_ignored += 1;
                } else {
                    self.report.adjustment_layers_ignored += 1;
                }
                continue;
            }
            self.tally(crec);
            let cmode = match self.mode_of(c) {
                BlendMode::PassThrough => BlendMode::Normal,
                m => m,
            };
            let copacity = Self::alpha_of(crec);
            let csurf = match c {
                Node::Layer(_) => self.layer_surface(Self::idx(c))?,
                Node::Group { kids, .. } => {
                    self.report.groups += 1;
                    if depth >= MAX_DEPTH {
                        return Err(Error::Malformed(
                            Format::Psd,
                            "layer groups nest too deeply".into(),
                        ));
                    }
                    let mut s = self.surface(self.canvas_rect())?;
                    self.render(kids, &mut s, depth + 1)?;
                    Some(s)
                }
            };
            if let Some(mut cs) = csurf {
                self.apply_mask(Self::idx(c), &mut cs);
                blend_surf(&mut unit, &cs, cmode, copacity, true);
                self.report.clipped_layers += 1;
                if matches!(c, Node::Layer(_)) {
                    self.report.layers_drawn += 1;
                }
                self.free(cs);
            }
        }
        let mode = if pass { BlendMode::Normal } else { mode };
        blend_surf(dst, &unit, mode, opacity, false);
        self.free(unit);
        Ok(())
    }

    /// A pass-through group: children draw on the backdrop itself; opacity or a mask mixes the
    /// result back toward the untouched backdrop.
    fn pass_through(
        &mut self,
        idx: usize,
        kids: &[Node],
        opacity: f32,
        dst: &mut Surf,
        depth: usize,
    ) -> Result<()> {
        if depth >= MAX_DEPTH {
            return Err(Error::Malformed(
                Format::Psd,
                "layer groups nest too deeply".into(),
            ));
        }
        let mask = self.mask_plane(idx);
        if opacity >= 1.0 && mask.is_none() {
            return self.render(kids, dst, depth + 1);
        }
        let mut tmp = self.surface(Rect::from_xywh(
            dst.left,
            dst.top,
            dst.w as u32,
            dst.h as u32,
        ))?;
        tmp.px.copy_from_slice(&dst.px);
        self.render(kids, &mut tmp, depth + 1)?;
        for y in 0..dst.h {
            for x in 0..dst.w {
                let k = opacity
                    * mask
                        .as_ref()
                        .map_or(1.0, |m| m.at(dst.left + x as i32, dst.top + y as i32));
                let (Some(d), Some(t)) = (dst.px.get_mut(y * dst.w + x), tmp.px.get(y * dst.w + x))
                else {
                    continue;
                };
                let a = d[3] + (t[3] - d[3]) * k;
                if a <= 0.0 {
                    *d = [0.0; 4];
                    continue;
                }
                for c in 0..3 {
                    let pm = d[c] * d[3] + (t[c] * t[3] - d[c] * d[3]) * k;
                    d[c] = pm / a;
                }
                d[3] = a;
            }
        }
        self.free(tmp);
        Ok(())
    }

    /// The layer's stored raster, cropped to the canvas. `None` when it contributes nothing.
    fn layer_surface(&mut self, idx: usize) -> Result<Option<Surf>> {
        let Some(rec) = self.file.layers().get(idx) else {
            return Ok(None);
        };
        let r = rec.rect;
        if r.is_empty() || rec.channels.is_empty() {
            return Ok(None);
        }
        let (cw, ch) = (self.canvas.0 as i64, self.canvas.1 as i64);
        let vis = (
            i64::from(r.left).max(0),
            i64::from(r.top).max(0),
            i64::from(r.right).min(cw),
            i64::from(r.bottom).min(ch),
        );
        if vis.0 >= vis.2 || vis.1 >= vis.3 {
            return Ok(None);
        }
        // The decoder works on the whole stored rectangle: refuse before it allocates.
        let stored = (r.width().max(0) as u64).saturating_mul(r.height().max(0) as u64);
        if stored.saturating_mul(8) > self.limits.max_alloc {
            self.report.layers_oversized += 1;
            return Ok(None);
        }
        let Some(view) = self.file.layer(idx) else {
            return Ok(None);
        };
        let img = match view.rgba8() {
            Ok(i) => i,
            Err(_) => {
                self.report.layers_unreadable += 1;
                return Ok(None);
            }
        };
        let mut s = self.surface(Rect::from_xywh(
            vis.0 as i32,
            vis.1 as i32,
            (vis.2 - vis.0) as u32,
            (vis.3 - vis.1) as u32,
        ))?;
        for y in 0..s.h {
            let sy = (vis.1 - i64::from(r.top)) as usize + y;
            for x in 0..s.w {
                let sx = (vis.0 - i64::from(r.left)) as usize + x;
                let o = (sy * img.width as usize + sx) * 4;
                if let (Some(p), Some(d)) = (img.data.get(o..o + 4), s.px.get_mut(y * s.w + x)) {
                    *d = [
                        f32::from(p[0]) / 255.0,
                        f32::from(p[1]) / 255.0,
                        f32::from(p[2]) / 255.0,
                        f32::from(p[3]) / 255.0,
                    ];
                }
            }
        }
        Ok(Some(s))
    }

    fn mask_plane(&mut self, idx: usize) -> Option<MaskPlane> {
        let rec = self.file.layers().get(idx)?;
        let m: &LayerMask = rec.layer_mask()?;
        let view = self.file.layer(idx)?;
        let real = rec
            .channel(craft_psd::layer::CHANNEL_REAL_USER_MASK)
            .is_some();
        let flags = if real {
            m.real.map_or(m.flags, |r| r.flags)
        } else {
            m.flags
        };
        if flags & LayerMask::FLAG_DISABLED != 0 {
            self.report.masks_disabled += 1;
            return None;
        }
        let img = match view.user_mask()? {
            Ok(g) => g,
            Err(_) => {
                self.report.masks_unreadable += 1;
                return None;
            }
        };
        let density = m
            .parameters
            .and_then(|p| p.user_density)
            .map_or(1.0, |d| f32::from(d) / 255.0);
        self.report.masks_applied += 1;
        Some(MaskPlane {
            img,
            invert: flags & LayerMask::FLAG_INVERT != 0,
            density,
        })
    }

    fn apply_mask(&mut self, idx: usize, s: &mut Surf) {
        let Some(mask) = self.mask_plane(idx) else {
            return;
        };
        for y in 0..s.h {
            for x in 0..s.w {
                if let Some(p) = s.px.get_mut(y * s.w + x) {
                    p[3] *= mask.at(s.left + x as i32, s.top + y as i32);
                }
            }
        }
    }
}

struct MaskPlane {
    img: GrayImage,
    invert: bool,
    density: f32,
}

impl MaskPlane {
    /// Mask value at a document coordinate, 0 (hide) to 1 (show). Feather is not applied.
    fn at(&self, x: i32, y: i32) -> f32 {
        let (lx, ly) = (
            i64::from(x) - i64::from(self.img.left),
            i64::from(y) - i64::from(self.img.top),
        );
        let raw = if lx >= 0
            && ly >= 0
            && lx < i64::from(self.img.width)
            && ly < i64::from(self.img.height)
        {
            self.img
                .data
                .get(ly as usize * self.img.width as usize + lx as usize)
                .copied()
                .unwrap_or(self.img.default_value)
        } else {
            self.img.default_value
        };
        let mut m = f32::from(raw) / 255.0;
        if self.invert {
            m = 1.0 - m;
        }
        1.0 - self.density * (1.0 - m)
    }
}

/// Blend `src` onto `dst`. With `clip`, `dst`'s alpha is kept and only its covered pixels change.
fn blend_surf(dst: &mut Surf, src: &Surf, mode: BlendMode, opacity: f32, clip: bool) {
    let x0 = dst.left.max(src.left);
    let y0 = dst.top.max(src.top);
    let x1 = (i64::from(dst.left) + dst.w as i64).min(i64::from(src.left) + src.w as i64);
    let y1 = (i64::from(dst.top) + dst.h as i64).min(i64::from(src.top) + src.h as i64);
    for y in i64::from(y0)..y1 {
        for x in i64::from(x0)..x1 {
            let si = (y - i64::from(src.top)) as usize * src.w + (x - i64::from(src.left)) as usize;
            let di = (y - i64::from(dst.top)) as usize * dst.w + (x - i64::from(dst.left)) as usize;
            if let (Some(s), Some(d)) = (src.px.get(si), dst.px.get_mut(di)) {
                composite_px(d, [s[0], s[1], s[2]], s[3] * opacity, mode, clip);
            }
        }
    }
}

/// W3C general compositing: `αr = αb + αs − αbαs`,
/// `Cr = (1 − αs/αr)·Cb + αs/αr·((1 − αb)·Cs + αb·B(Cb, Cs))`.
pub(crate) fn composite_px(dst: &mut [f32; 4], cs: [f32; 3], sa: f32, mode: BlendMode, clip: bool) {
    let sa = sa.clamp(0.0, 1.0);
    let ab = dst[3];
    if sa <= 0.0 || (clip && ab <= 0.0) {
        return;
    }
    let cb = [dst[0], dst[1], dst[2]];
    let blended = if ab > 0.0 {
        blend_rgb(mode, cb, cs)
    } else {
        cs
    };
    if clip {
        for c in 0..3 {
            dst[c] = cb[c] * (1.0 - sa) + blended[c] * sa;
        }
        return;
    }
    let ar = sa + ab - sa * ab;
    let k = sa / ar;
    for c in 0..3 {
        let mixed = (1.0 - ab) * cs[c] + ab * blended[c];
        dst[c] = (1.0 - k) * cb[c] + k * mixed;
    }
    dst[3] = ar;
}

/// The blend function `B(Cb, Cs)` of a whole pixel.
pub(crate) fn blend_rgb(mode: BlendMode, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
    match mode {
        BlendMode::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        BlendMode::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        BlendMode::Color => set_lum(cs, lum(cb)),
        BlendMode::Luminosity => set_lum(cb, lum(cs)),
        // Whole-pixel comparisons on the channel sum.
        BlendMode::DarkerColor => {
            if cs.iter().sum::<f32>() < cb.iter().sum::<f32>() {
                cs
            } else {
                cb
            }
        }
        BlendMode::LighterColor => {
            if cs.iter().sum::<f32>() > cb.iter().sum::<f32>() {
                cs
            } else {
                cb
            }
        }
        m => [
            sep(m, cb[0], cs[0]),
            sep(m, cb[1], cs[1]),
            sep(m, cb[2], cs[2]),
        ],
    }
}

fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        b + (2.0 * s - 1.0) - b * (2.0 * s - 1.0)
    }
}

/// Separable modes on one channel; `b` backdrop, `s` source.
fn sep(mode: BlendMode, b: f32, s: f32) -> f32 {
    let v = match mode {
        BlendMode::Darken => b.min(s),
        BlendMode::Multiply => b * s,
        BlendMode::ColorBurn => color_burn(b, s),
        BlendMode::LinearBurn => b + s - 1.0,
        BlendMode::Lighten => b.max(s),
        BlendMode::Screen => b + s - b * s,
        BlendMode::ColorDodge => color_dodge(b, s),
        BlendMode::LinearDodge => b + s,
        BlendMode::Overlay => hard_light(s, b),
        BlendMode::SoftLight => {
            if s <= 0.5 {
                (1.0 - 2.0 * s) * b * b + 2.0 * s * b
            } else {
                2.0 * b * (1.0 - s) + b.sqrt() * (2.0 * s - 1.0)
            }
        }
        BlendMode::HardLight => hard_light(b, s),
        BlendMode::VividLight => {
            if s < 0.5 {
                color_burn(b, 2.0 * s)
            } else {
                color_dodge(b, 2.0 * (s - 0.5))
            }
        }
        BlendMode::LinearLight => b + 2.0 * s - 1.0,
        BlendMode::PinLight => {
            if s < 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        BlendMode::HardMix => {
            if b + s >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        BlendMode::Subtract => b - s,
        BlendMode::Divide => {
            if s <= 0.0 {
                1.0
            } else {
                b / s
            }
        }
        // Normal, pass-through, dissolve and anything unknown.
        _ => s,
    };
    v.clamp(0.0, 1.0)
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut c = c;
    if n < 0.0 {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for v in &mut c {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    c
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    if mx <= mn {
        return [0.0; 3];
    }
    let f = |v: f32| (v - mn) * s / (mx - mn);
    [f(c[0]), f(c[1]), f(c[2])]
}

#[cfg(test)]
mod tests;
