//! Gain-map HDR stills: an SDR base image plus a (usually smaller) gain map that says how much
//! brighter each part of the picture is on an HDR display.
//!
//! Three families are read, each from its public description:
//!
//! * **Ultra HDR / Android JPEG.** The gain map is the second image of the file's Multi-Picture
//!   Format (CIPA DC-007) index, a JPEG of its own whose XMP carries the `hdrgm:` properties
//!   (Ultra HDR Image Format: `GainMapMin`/`Max`, `Gamma`, `OffsetSDR`/`HDR`, `HDRCapacityMin`/
//!   `Max`, `BaseRenditionIsHDR`, each a single value or a three-item `rdf:Seq`).
//! * **ISO 21496-1.** The same model as a binary structure: in a JPEG, an APP2 segment tagged
//!   `urn:iso:std:iso:ts:21496:-1` in the gain-map image; in HEIF and AVIF, the payload of a
//!   `tmap` derived item whose inputs are the base and the gain-map image. Headrooms and gains
//!   are log2 values stored as numerator/denominator pairs.
//! * **Apple.** A HEIC whose primary image has an auxiliary image with the `auxC` type
//!   `urn:com:apple:photo:2020:aux:hdrgainmap`. Its XMP (`HDRGainMap:` namespace) holds a version
//!   and sometimes the headroom; otherwise the headroom comes from the EXIF maker note, by
//!   Apple's published sample formula. Newer files add an ISO 21496-1 `tmap`, which wins.
//!
//! Everything here is opt-in: [`decode`](crate::decode), [`thumbnail`](crate::thumbnail) and
//! [`render`](crate::render) still produce the SDR base. [`apply_gain_map`] is for HDR
//! renditions, which are not wired in yet.
//!
//! The model, as ISO 21496-1 states it: a gain-map sample `g` in `0..=1` becomes a log2 gain
//! `min + (max − min)·g^(1/gamma)`; at a display headroom `H` (linear, 1 = SDR white) the
//! weight is `w = clamp((log2 H − lo) / (hi − lo), 0, 1)` between the two capacities `lo` and
//! `hi`, and `out = (base + offset_base)·2^(gain·w) − offset_alternate`. When the base is
//! already HDR the weight runs the other way. Apple's older sample curve is linear:
//! `out = base·(1 + (min(H, Hmax) − 1)·g)`.

use crate::bytes::{be_u16, be_u32};
use crate::formats::jpeg::segments;
use crate::heif::Heif;
use crate::tiff::Tiff;
use crate::{DecodeOptions, Error, Format, Limits, Result, Rgba8Image, sniff, xmp};
use serde::{Deserialize, Serialize};

const HDRGM_NS: &str = "http://ns.adobe.com/hdr-gain-map/1.0/";
const APPLE_NS: &str = "http://ns.apple.com/HDRGainMap/1.0/";
const APPLE_URN: &[u8] = b"urn:com:apple:photo:2020:aux:hdrgainmap";
const ISO_URN: &[u8] = b"urn:iso:std:iso:ts:21496:-1\0";
const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
/// Secondary images of an MPF index that are looked at.
const MAX_MPF_IMAGES: usize = 16;
/// Headrooms past this many stops (2^24) are not an image.
const MAX_STOPS: f32 = 24.0;
/// Headroom assumed for an Apple gain map that states none (3 stops, what recent iPhones top
/// out at).
const APPLE_DEFAULT_STOPS: f32 = 3.0;

/// Which family of gain map a file carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GainMapSource {
    /// Ultra HDR (Android JPEG): `hdrgm:` XMP on the MPF secondary image.
    UltraHdr,
    /// ISO 21496-1 metadata: a JPEG APP2 segment or a HEIF/AVIF `tmap` item.
    Iso21496,
    /// Apple's `hdrgainmap` auxiliary image of a HEIC.
    Apple,
}

/// The shape of the gain curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GainCurve {
    /// Log2 gain between `min` and `max`, weighted by the display headroom (Ultra HDR, ISO).
    Log2,
    /// `1 + (headroom − 1)·g`, Apple's sample formula for gain maps without ISO metadata.
    AppleLinear,
}

/// The numbers that turn a gain map into brightness. Gains and capacities are log2 (stops);
/// per-channel values are R, G, B (a single-channel map repeats its value).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GainMapParams {
    /// Log2 gain where the map is 0.
    pub min_log2_gain: [f32; 3],
    /// Log2 gain where the map is 1.
    pub max_log2_gain: [f32; 3],
    /// Encoding gamma of the map: the stored sample is `value^gamma`.
    pub gamma: [f32; 3],
    /// Offset added to the SDR rendition before the gain (and removed after, when HDR is made).
    pub offset_sdr: [f32; 3],
    /// Offset of the HDR rendition.
    pub offset_hdr: [f32; 3],
    /// Log2 headroom of the less bright of the two renditions (0 for an SDR one).
    pub hdr_capacity_min: f32,
    /// Log2 headroom of the brighter rendition: the most the map can add.
    pub hdr_capacity_max: f32,
    /// Whether the base image is the HDR rendition (the map then leads back to SDR).
    pub base_is_hdr: bool,
    /// How the map is applied.
    pub curve: GainCurve,
}

impl GainMapParams {
    /// The largest display headroom the map reaches (linear, 1 = SDR white).
    pub fn max_headroom(&self) -> f32 {
        self.hdr_capacity_max.clamp(0.0, MAX_STOPS).exp2()
    }

    /// Refuse values that would make the maths meaningless (NaN, infinity, non-positive gamma,
    /// absurd stops, inverted ranges).
    pub fn validate(&self) -> Result<()> {
        let bad = |why: &str| Error::InvalidArgument(format!("gain map parameters: {why}"));
        let all: [&[f32]; 5] = [
            &self.min_log2_gain,
            &self.max_log2_gain,
            &self.gamma,
            &self.offset_sdr,
            &self.offset_hdr,
        ];
        if all.iter().any(|a| a.iter().any(|v| !v.is_finite()))
            || !self.hdr_capacity_min.is_finite()
            || !self.hdr_capacity_max.is_finite()
        {
            return Err(bad("a value is not finite"));
        }
        if self.gamma.iter().any(|&g| !(1e-3..=1e3).contains(&g)) {
            return Err(bad("gamma is out of range"));
        }
        if self
            .min_log2_gain
            .iter()
            .chain(&self.max_log2_gain)
            .chain(&[self.hdr_capacity_min, self.hdr_capacity_max])
            .any(|v| v.abs() > MAX_STOPS)
        {
            return Err(bad("a gain or capacity is past 24 stops"));
        }
        if self
            .min_log2_gain
            .iter()
            .zip(&self.max_log2_gain)
            .any(|(lo, hi)| lo > hi)
            || self.hdr_capacity_min > self.hdr_capacity_max
        {
            return Err(bad("a minimum is above its maximum"));
        }
        if self
            .offset_sdr
            .iter()
            .chain(&self.offset_hdr)
            .any(|o| o.abs() > 1.0)
        {
            return Err(bad("an offset is past 1"));
        }
        Ok(())
    }
}

/// A gain map: its pixels (when they can be decoded), its parameters and where they came from.
#[derive(Clone, Debug, PartialEq)]
pub struct GainMap {
    /// The map as RGB (a single-channel map repeats its value in all three), usually at a
    /// fraction of the base image's size and in the same orientation. `None` when this build
    /// cannot decode it; [`GainMap::image_unavailable`] says why.
    pub image: Option<Rgba8Image>,
    /// Why [`GainMap::image`] is `None`.
    pub image_unavailable: Option<String>,
    /// The parameters.
    pub params: GainMapParams,
    /// The family the file belongs to.
    pub source: GainMapSource,
    /// Assumptions made while reading (a headroom that had to be guessed, say).
    pub notes: Vec<String>,
}

/// What a catalogue wants to know without decoding the map: that there is one, and how bright
/// the HDR rendition gets.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GainMapSummary {
    /// The family.
    pub source: GainMapSource,
    /// The most the map brightens the picture, as a linear headroom (4 means two stops over
    /// SDR white).
    pub headroom: f32,
    /// Whether the base image is the HDR rendition.
    pub base_is_hdr: bool,
}

/// Linear-light RGB, three `f32` per pixel, rows top to bottom. Primaries are BT.709 (sRGB);
/// 1.0 is SDR reference white, so HDR highlights exceed 1.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearRgbF32 {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 3` values: R, G, B. Finite and never negative.
    pub data: Vec<f32>,
}

/// Where the gain-map pixels are.
enum ImageRef<'a> {
    Jpeg(&'a [u8]),
    Heif(Box<Heif<'a>>, u32),
}

struct Located<'a> {
    source: GainMapSource,
    params: GainMapParams,
    notes: Vec<String>,
    image: ImageRef<'a>,
}

/// Find the gain map of a file and decode it. `Ok(None)` when the file has none (or is a
/// format that cannot carry one); an error when it declares one that is damaged.
pub fn gain_map(bytes: &[u8]) -> Result<Option<GainMap>> {
    let Some(found) = locate(bytes)? else {
        return Ok(None);
    };
    let (image, image_unavailable) = match decode_map(&found.image) {
        Ok(img) => (Some(img), None),
        Err(e) => (None, Some(reason(&e))),
    };
    Ok(Some(GainMap {
        image,
        image_unavailable,
        params: found.params,
        source: found.source,
        notes: found.notes,
    }))
}

/// Whether the file has a gain map and how bright it gets, without decoding the map. This is
/// what [`probe`](crate::probe) reports.
pub fn gain_map_summary(bytes: &[u8]) -> Result<Option<GainMapSummary>> {
    Ok(locate(bytes)?.map(|f| GainMapSummary {
        source: f.source,
        headroom: f.params.max_headroom(),
        base_is_hdr: f.params.base_is_hdr,
    }))
}

fn reason(e: &Error) -> String {
    let text = e.to_string();
    // craft-hevc decodes 4:2:0 only: Apple's map (like HEIF alpha) is monochrome HEVC.
    if text.contains("4:2:0") || text.contains("chroma") {
        format!(
            "the gain-map image is monochrome HEVC, which craft-hevc cannot decode yet ({text})"
        )
    } else {
        format!("the gain-map image could not be decoded: {text}")
    }
}

fn decode_map(image: &ImageRef<'_>) -> Result<Rgba8Image> {
    match image {
        ImageRef::Jpeg(b) => Ok(crate::decode(b, &DecodeOptions::default())?.image),
        ImageRef::Heif(h, id) => {
            crate::decode::guarded(Format::Heif, || h.decode_coded(*id, &Limits::default()))
        }
    }
}

fn locate(bytes: &[u8]) -> Result<Option<Located<'_>>> {
    Limits::default().check_input(bytes.len())?;
    match sniff(bytes) {
        Some(Format::Jpeg) => Ok(locate_jpeg(bytes)),
        Some(f @ (Format::Heif | Format::Avif)) => locate_heif(bytes, f),
        _ => Ok(None),
    }
}

// ---- JPEG: MPF, hdrgm XMP, ISO APP2 -------------------------------------------------------

/// The secondary images of a JPEG's MPF index (CIPA DC-007): the primary is image 0 and is
/// skipped. Offsets are from the MPF byte-order mark; a range outside the data is dropped.
fn mpf_secondary_images(bytes: &[u8]) -> Vec<&[u8]> {
    let (segs, _) = segments(bytes);
    let Some(seg) = segs
        .iter()
        .find(|s| s.marker == 0xE2 && s.data.starts_with(b"MPF\0"))
    else {
        return Vec::new();
    };
    let Some(base) = (seg.data.as_ptr() as usize)
        .checked_sub(bytes.as_ptr() as usize)
        .and_then(|p| p.checked_add(4))
    else {
        return Vec::new();
    };
    let Some(t) = seg.data.get(4..).and_then(Tiff::new) else {
        return Vec::new();
    };
    let Some(ifd) = t.first_ifd().and_then(|p| t.ifd(p)) else {
        return Vec::new();
    };
    let Some(entry) = ifd.get(0xB002) else {
        return Vec::new();
    };
    let count = (entry.count as usize / 16).min(MAX_MPF_IMAGES);
    let Some(start) = t.u32_at(entry.value_pos).map(|v| v as usize) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 1..count {
        let at = start.saturating_add(i * 16);
        let (Some(size), Some(off)) = (
            t.u32_at(at.saturating_add(4)),
            t.u32_at(at.saturating_add(8)),
        ) else {
            break;
        };
        let (Some(from), size) = (base.checked_add(off as usize), size as usize) else {
            continue;
        };
        if let Some(img) = from
            .checked_add(size)
            .and_then(|to| bytes.get(from..to))
            .filter(|img| img.starts_with(&[0xFF, 0xD8]))
        {
            out.push(img);
        }
    }
    out
}

fn locate_jpeg(bytes: &[u8]) -> Option<Located<'_>> {
    for sec in mpf_secondary_images(bytes) {
        let (segs, _) = segments(sec);
        let xmp_params = segs
            .iter()
            .find(|s| s.marker == 0xE1 && s.data.starts_with(XMP_SIG))
            .and_then(|s| s.data.get(XMP_SIG.len()..))
            .and_then(ultrahdr_params);
        let iso_params = segs
            .iter()
            .find(|s| s.marker == 0xE2 && s.data.starts_with(ISO_URN))
            .and_then(|s| s.data.get(ISO_URN.len()..))
            .and_then(iso_params);
        let (source, params) = match (xmp_params, iso_params) {
            (Some(p), _) => (GainMapSource::UltraHdr, p),
            (None, Some(p)) => (GainMapSource::Iso21496, p),
            (None, None) => continue,
        };
        if params.validate().is_err() {
            continue;
        }
        return Some(Located {
            source,
            params,
            notes: Vec::new(),
            image: ImageRef::Jpeg(sec),
        });
    }
    None
}

/// A property's values as numbers: one value or one per channel.
fn numbers(props: &[(String, String, Vec<xmp::Item>)], ns: &str, local: &str) -> Option<Vec<f32>> {
    let (_, _, items) = props.iter().find(|(n, l, _)| n == ns && l == local)?;
    items
        .iter()
        .map(|(_, t)| t.trim().parse::<f32>().ok().filter(|v| v.is_finite()))
        .collect()
}

fn channels(v: Option<Vec<f32>>, default: f32) -> [f32; 3] {
    match v.as_deref() {
        Some([a]) => [*a; 3],
        Some([r, g, b]) => [*r, *g, *b],
        _ => [default; 3],
    }
}

/// Ultra HDR parameters from the gain-map image's XMP packet. `None` when it has no
/// `hdrgm:Version`, or lacks the two required properties.
fn ultrahdr_params(packet: &[u8]) -> Option<GainMapParams> {
    let props = xmp::properties(packet).ok()?;
    let text = |local: &str| {
        props
            .iter()
            .find(|(n, l, _)| n == HDRGM_NS && l == local)
            .and_then(|(_, _, i)| i.first())
            .map(|(_, t)| t.trim().to_string())
    };
    text("Version")?;
    let n = |local: &str| numbers(&props, HDRGM_NS, local);
    let max = n("GainMapMax")?;
    let cap_max = n("HDRCapacityMax")?;
    Some(GainMapParams {
        min_log2_gain: channels(n("GainMapMin"), 0.0),
        max_log2_gain: channels(Some(max), 0.0),
        gamma: channels(n("Gamma"), 1.0),
        offset_sdr: channels(n("OffsetSDR"), 1.0 / 64.0),
        offset_hdr: channels(n("OffsetHDR"), 1.0 / 64.0),
        hdr_capacity_min: n("HDRCapacityMin")
            .and_then(|v| v.first().copied())
            .unwrap_or(0.0),
        hdr_capacity_max: *cap_max.first()?,
        base_is_hdr: text("BaseRenditionIsHDR").is_some_and(|t| t.eq_ignore_ascii_case("true")),
        curve: GainCurve::Log2,
    })
}

// ---- ISO 21496-1 ------------------------------------------------------------------------

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl Rd<'_> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.d.get(self.p)?;
        self.p += 1;
        Some(v)
    }

    fn u16(&mut self) -> Option<u16> {
        let v = be_u16(self.d, self.p)?;
        self.p += 2;
        Some(v)
    }

    fn u32(&mut self) -> Option<u32> {
        let v = be_u32(self.d, self.p)?;
        self.p += 4;
        Some(v)
    }

    /// A signed numerator over an unsigned denominator.
    fn signed(&mut self) -> Option<f32> {
        let n = self.u32()? as i32;
        let d = self.u32()?;
        ratio(f64::from(n), d)
    }

    fn unsigned(&mut self) -> Option<f32> {
        let n = self.u32()?;
        let d = self.u32()?;
        ratio(f64::from(n), d)
    }
}

fn ratio(n: f64, d: u32) -> Option<f32> {
    (d != 0)
        .then(|| (n / f64::from(d)) as f32)
        .filter(|v| v.is_finite())
}

/// ISO 21496-1 metadata as it follows the JPEG URN: versions, a flag byte (multi-channel in the
/// top bit), the two headrooms, then per channel the min and max gain, gamma and the two
/// offsets. `None` for a newer minimum version, a short structure, or a zero denominator.
fn iso_params(d: &[u8]) -> Option<GainMapParams> {
    let mut r = Rd { d, p: 0 };
    if r.u16()? != 0 {
        return None; // minimum_version: a reader of version 0 must refuse anything newer
    }
    let _writer_version = r.u16()?;
    let flags = r.u8()?;
    let n_channels = if flags & 0x80 != 0 { 3 } else { 1 };
    let base_headroom = r.unsigned()?;
    let alt_headroom = r.unsigned()?;
    let mut p = GainMapParams {
        min_log2_gain: [0.0; 3],
        max_log2_gain: [0.0; 3],
        gamma: [1.0; 3],
        offset_sdr: [0.0; 3],
        offset_hdr: [0.0; 3],
        hdr_capacity_min: base_headroom.min(alt_headroom),
        hdr_capacity_max: base_headroom.max(alt_headroom),
        base_is_hdr: base_headroom > alt_headroom,
        curve: GainCurve::Log2,
    };
    for c in 0..n_channels {
        let min = r.signed()?;
        let max = r.signed()?;
        let gamma = r.unsigned()?;
        let base_offset = r.signed()?;
        let alt_offset = r.signed()?;
        p.min_log2_gain[c] = min;
        p.max_log2_gain[c] = max;
        p.gamma[c] = gamma;
        let (sdr, hdr) = if p.base_is_hdr {
            (alt_offset, base_offset)
        } else {
            (base_offset, alt_offset)
        };
        p.offset_sdr[c] = sdr;
        p.offset_hdr[c] = hdr;
    }
    if n_channels == 1 {
        for c in 1..3 {
            p.min_log2_gain[c] = p.min_log2_gain[0];
            p.max_log2_gain[c] = p.max_log2_gain[0];
            p.gamma[c] = p.gamma[0];
            p.offset_sdr[c] = p.offset_sdr[0];
            p.offset_hdr[c] = p.offset_hdr[0];
        }
    }
    p.validate().ok()?;
    Some(p)
}

// ---- HEIF / AVIF: tmap and Apple auxiliary images -----------------------------------------

fn locate_heif(bytes: &[u8], format: Format) -> Result<Option<Located<'_>>> {
    let h = Heif::parse(bytes, format)?;
    let primary = h.primary_id();
    let apple = h.aux_with_urn(primary, APPLE_URN).first().copied();

    // ISO: the first `tmap` whose payload (a version byte, then ISO 21496-1) reads.
    let mut iso: Option<(GainMapParams, Option<u32>)> = None;
    for id in h.items_of_type(b"tmap") {
        let Ok(payload) = h.payload(id) else { continue };
        if payload.first() != Some(&0) {
            continue;
        }
        if let Some(p) = payload.get(1..).and_then(iso_params) {
            let map = h.derived_inputs(id).get(1).copied();
            iso = Some((p, map));
            break;
        }
    }

    let mut notes = Vec::new();
    let (source, params, map_item) = match (apple, iso) {
        (Some(a), Some((p, _))) => (GainMapSource::Apple, p, a),
        (Some(a), None) => (
            GainMapSource::Apple,
            apple_params(&h, a, bytes, &mut notes),
            a,
        ),
        (None, Some((p, Some(m)))) => (GainMapSource::Iso21496, p, m),
        (None, Some((_, None))) => {
            return Err(Error::Malformed(
                format,
                "tmap item does not name a gain-map image".into(),
            ));
        }
        (None, None) => return Ok(None),
    };
    params.validate()?;
    Ok(Some(Located {
        source,
        params,
        notes,
        image: ImageRef::Heif(Box::new(h), map_item),
    }))
}

/// Apple's parameters: version and (sometimes) headroom from the auxiliary image's XMP, else
/// the headroom from the maker note, else an assumed 3 stops.
fn apple_params(h: &Heif<'_>, aux: u32, file: &[u8], notes: &mut Vec<String>) -> GainMapParams {
    let from_xmp = h.xmp_describing(aux).iter().find_map(|packet| {
        let props = xmp::properties(packet).ok()?;
        numbers(&props, APPLE_NS, "HDRGainMapHeadroom")?
            .first()
            .copied()
    });
    let stops = match from_xmp.filter(|h| *h >= 1.0) {
        Some(headroom) => headroom.log2(),
        None => match maker_note_stops(file) {
            Some(s) => s,
            None => {
                notes.push(format!(
                    "no headroom in the XMP or maker note; assumed {APPLE_DEFAULT_STOPS} stops"
                ));
                APPLE_DEFAULT_STOPS
            }
        },
    };
    let stops = stops.clamp(0.0, MAX_STOPS);
    GainMapParams {
        min_log2_gain: [0.0; 3],
        max_log2_gain: [stops; 3],
        gamma: [1.0; 3],
        offset_sdr: [0.0; 3],
        offset_hdr: [0.0; 3],
        hdr_capacity_min: 0.0,
        hdr_capacity_max: stops,
        base_is_hdr: false,
        curve: GainCurve::AppleLinear,
    }
}

/// Headroom in stops from Apple's maker-note tags 33 and 48, by the formula in Apple's
/// "Applying Apple HDR effect to your photos".
fn maker_note_stops(file: &[u8]) -> Option<f32> {
    let blocks = crate::embedded(file);
    let exif = blocks.exif.as_deref()?;
    let t = Tiff::new(exif)?;
    let ifd0 = t.ifd(t.first_ifd()?)?;
    let exif_ifd = t.ifd(t.uint(ifd0.get(0x8769)?)? as usize)?;
    let note = t.bytes(exif_ifd.get(0x927C)?)?;
    if !note.starts_with(b"Apple iOS\0") {
        return None;
    }
    // "Apple iOS\0", a 2-byte version, a byte-order mark, then an IFD whose offsets count from
    // the start of the note.
    let big_endian = note.get(12..14)? == b"MM";
    let nt = Tiff::with_order(note, !big_endian);
    let ifd = nt.ifd(14)?;
    let value = |tag| {
        let e = ifd.get(tag)?;
        nt.rational(e, 0).map(|v| v as f32)
    };
    let (m33, m48) = (value(33)?, value(48)?);
    let stops = if m33 < 1.0 {
        if m48 <= 0.01 {
            -20.0 * m48 + 1.8
        } else {
            -0.101 * m48 + 1.601
        }
    } else if m48 <= 0.01 {
        -70.0 * m48 + 3.0
    } else {
        -0.303 * m48 + 2.303
    };
    stops.is_finite().then_some(stops.max(0.0))
}

// ---- Applying a gain map ------------------------------------------------------------------

/// The 8-bit sRGB code value as linear light.
fn srgb_to_linear_table() -> [f32; 256] {
    let mut t = [0.0f32; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let e = i as f32 / 255.0;
        *v = if e <= 0.040_45 {
            e / 12.92
        } else {
            ((e + 0.055) / 1.055).powf(2.4)
        };
    }
    t
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

/// Make the HDR rendition for a display with `target_headroom` (linear; 1 is an SDR display,
/// about 4 to 8 for HDR ones; clamped to at least 1). `base` is the SDR image in its stored
/// orientation, `base_icc` its profile (converted to sRGB first; `None` means sRGB). The result
/// is linear BT.709 light relative to SDR white.
///
/// At headroom 1 the output is the base (up to the offsets' rounding); at the map's maximum the
/// brightest parts are boosted by the full gain. Errors when the map's pixels are unavailable
/// or its parameters are not finite and in range.
pub fn apply_gain_map(
    base: &Rgba8Image,
    base_icc: Option<&[u8]>,
    gm: &GainMap,
    target_headroom: f32,
) -> Result<LinearRgbF32> {
    gm.params.validate()?;
    let map = gm.image.as_ref().ok_or_else(|| {
        Error::InvalidArgument(
            gm.image_unavailable
                .clone()
                .unwrap_or_else(|| "the gain-map image is unavailable".into()),
        )
    })?;
    let converted;
    let base = match base_icc {
        Some(profile) => {
            let mut c = base.clone();
            crate::icc::to_srgb(&mut c, crate::icc::Source::Embedded(profile));
            converted = c;
            &converted
        }
        None => base,
    };
    let (w, h) = (base.width() as usize, base.height() as usize);
    let (mw, mh) = (map.width() as usize, map.height() as usize);
    let len = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| Error::InvalidArgument("image is too large".into()))?;
    let mut data = Vec::new();
    data.try_reserve_exact(len)
        .map_err(|e| Error::InvalidArgument(e.to_string()))?;

    let p = &gm.params;
    let target = finite_or(target_headroom, 1.0).clamp(1.0, 2f32.powf(MAX_STOPS));
    let stops = target.log2();
    let span = p.hdr_capacity_max - p.hdr_capacity_min;
    let toward_alternate = if span <= f32::EPSILON {
        // A degenerate range is all-or-nothing at its capacity.
        f32::from(stops >= p.hdr_capacity_max)
    } else if p.base_is_hdr {
        ((p.hdr_capacity_max - stops) / span).clamp(0.0, 1.0)
    } else {
        ((stops - p.hdr_capacity_min) / span).clamp(0.0, 1.0)
    };
    let (off_base, off_alt) = if p.base_is_hdr {
        (p.offset_hdr, p.offset_sdr)
    } else {
        (p.offset_sdr, p.offset_hdr)
    };
    let linear = srgb_to_linear_table();
    let map_px = map.data();
    let sample = |x: usize, y: usize, c: usize| {
        map_px
            .get((y * mw + x) * 4 + c)
            .map_or(0.0, |&v| f32::from(v) / 255.0)
    };
    let headroom_used = target.min(p.max_headroom().max(1.0));

    for y in 0..h {
        // Bilinear position of this row in the (smaller) map, pixel centres aligned.
        let fy = ((y as f32 + 0.5) * mh as f32 / h as f32 - 0.5).clamp(0.0, (mh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(mh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * mw as f32 / w as f32 - 0.5).clamp(0.0, (mw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(mw - 1);
            let i = (y * w + x) * 4;
            for c in 0..3 {
                let top = sample(x0, y0, c) * (1.0 - tx) + sample(x1, y0, c) * tx;
                let bottom = sample(x0, y1, c) * (1.0 - tx) + sample(x1, y1, c) * tx;
                let g = (top * (1.0 - ty) + bottom * ty).clamp(0.0, 1.0);
                let b = linear[usize::from(base.data().get(i + c).copied().unwrap_or(0))];
                let out = match p.curve {
                    GainCurve::AppleLinear => b * (1.0 + (headroom_used - 1.0) * g),
                    GainCurve::Log2 => {
                        let recovery = g.powf(1.0 / p.gamma[c]);
                        let log_gain = p.min_log2_gain[c]
                            + (p.max_log2_gain[c] - p.min_log2_gain[c]) * recovery;
                        let exponent = (log_gain * toward_alternate).clamp(-MAX_STOPS, MAX_STOPS);
                        (b + off_base[c]) * exponent.exp2() - off_alt[c]
                    }
                };
                data.push(finite_or(out, 0.0).max(0.0));
            }
        }
    }
    Ok(LinearRgbF32 {
        width: base.width(),
        height: base.height(),
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rat(n: i32, d: u32) -> Vec<u8> {
        let mut v = n.to_be_bytes().to_vec();
        v.extend_from_slice(&d.to_be_bytes());
        v
    }

    /// A single-channel ISO 21496-1 structure with an SDR base and 2 stops of headroom.
    fn iso_bytes() -> Vec<u8> {
        let mut b = vec![0, 0, 0, 0, 0]; // minimum_version, writer_version, flags
        b.extend(rat(0, 1)); // base headroom
        b.extend(rat(2, 1)); // alternate headroom
        b.extend(rat(-1, 2)); // min gain -0.5
        b.extend(rat(2, 1)); // max gain 2
        b.extend(rat(1, 1)); // gamma
        b.extend(rat(1, 64)); // base offset
        b.extend(rat(1, 64)); // alternate offset
        b
    }

    #[test]
    fn iso_metadata_is_read() {
        let p = iso_params(&iso_bytes()).unwrap();
        assert_eq!(p.min_log2_gain, [-0.5; 3]);
        assert_eq!(p.max_log2_gain, [2.0; 3]);
        assert_eq!((p.hdr_capacity_min, p.hdr_capacity_max), (0.0, 2.0));
        assert!(!p.base_is_hdr);
        assert!((p.max_headroom() - 4.0).abs() < 1e-6);
        assert_eq!(p.offset_sdr, [1.0 / 64.0; 3]);
    }

    #[test]
    fn iso_metadata_refuses_damage() {
        let b = iso_bytes();
        for n in 0..b.len() {
            assert!(iso_params(&b[..n]).is_none(), "prefix of {n}");
        }
        let mut newer = b.clone();
        newer[1] = 1; // minimum_version 1
        assert!(iso_params(&newer).is_none());
        let mut zero_den = b.clone();
        zero_den[8..13].copy_from_slice(&[0; 5]); // headroom denominator
        assert!(iso_params(&zero_den).is_none());
        let mut backwards = b;
        backwards[5..13].copy_from_slice(&rat(3, 1)); // base headroom above the alternate
        assert!(iso_params(&backwards).unwrap().base_is_hdr);
    }

    #[test]
    fn ultrahdr_xmp_attribute_and_seq_forms() {
        let attrs = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0" hdrgm:GainMapMax="3" hdrgm:HDRCapacityMax="3"/></rdf:RDF></x:xmpmeta>"#;
        let p = ultrahdr_params(attrs).unwrap();
        assert_eq!(p.max_log2_gain, [3.0; 3]);
        assert_eq!(p.gamma, [1.0; 3]);
        assert_eq!(p.offset_sdr, [1.0 / 64.0; 3]);
        let seq = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0" hdrgm:HDRCapacityMax="2" hdrgm:BaseRenditionIsHDR="True"><hdrgm:GainMapMax><rdf:Seq><rdf:li>1</rdf:li><rdf:li>2</rdf:li><rdf:li>3</rdf:li></rdf:Seq></hdrgm:GainMapMax></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let p = ultrahdr_params(seq).unwrap();
        assert_eq!(p.max_log2_gain, [1.0, 2.0, 3.0]);
        assert!(p.base_is_hdr);
        // No version, or a missing required property: not a gain map.
        assert!(ultrahdr_params(b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>").is_none());
        let no_max = attrs.to_vec();
        let no_max = String::from_utf8(no_max)
            .unwrap()
            .replace("hdrgm:GainMapMax=\"3\"", "");
        assert!(ultrahdr_params(no_max.as_bytes()).is_none());
    }

    #[test]
    fn validation_catches_nonsense() {
        let good = iso_params(&iso_bytes()).unwrap();
        assert!(good.validate().is_ok());
        for mutate in [
            |p: &mut GainMapParams| p.gamma[1] = 0.0,
            |p: &mut GainMapParams| p.gamma[0] = f32::NAN,
            |p: &mut GainMapParams| p.max_log2_gain[2] = f32::INFINITY,
            |p: &mut GainMapParams| p.hdr_capacity_max = 99.0,
            |p: &mut GainMapParams| p.min_log2_gain[0] = 5.0,
            |p: &mut GainMapParams| p.offset_hdr[0] = 4.0,
        ] {
            let mut p = good.clone();
            mutate(&mut p);
            assert!(p.validate().is_err(), "{p:?}");
        }
    }

    fn map(value: u8, params: GainMapParams) -> GainMap {
        GainMap {
            image: Some(Rgba8Image::filled(2, 2, [value, value, value, 255]).unwrap()),
            image_unavailable: None,
            params,
            source: GainMapSource::Iso21496,
            notes: Vec::new(),
        }
    }

    #[test]
    fn apply_scales_between_sdr_and_the_full_gain() {
        let params = iso_params(&iso_bytes()).unwrap(); // gain -0.5..2 stops, offsets 1/64
        let base = Rgba8Image::filled(4, 4, [188, 188, 188, 255]).unwrap(); // about 0.5 linear
        let lin = 0.5029f32;
        let sdr = apply_gain_map(&base, None, &map(255, params.clone()), 1.0).unwrap();
        assert!(
            sdr.data.iter().all(|v| (v - lin).abs() < 2e-3),
            "{:?}",
            &sdr.data[..3]
        );
        // Full gain (g = 1: 2 stops) at the map's maximum headroom.
        let hdr = apply_gain_map(&base, None, &map(255, params.clone()), 4.0).unwrap();
        let want = (lin + 1.0 / 64.0) * 4.0 - 1.0 / 64.0;
        assert!(
            hdr.data.iter().all(|v| (v - want).abs() < 2e-3),
            "{:?}",
            &hdr.data[..3]
        );
        // Headroom past the map's maximum adds nothing more.
        let more = apply_gain_map(&base, None, &map(255, params.clone()), 64.0).unwrap();
        assert_eq!(more.data, hdr.data);
        // Half way in stops is half the log gain.
        let half = apply_gain_map(&base, None, &map(255, params), 2.0).unwrap();
        let want = (lin + 1.0 / 64.0) * 2.0 - 1.0 / 64.0;
        assert!((half.data[0] - want).abs() < 2e-3);
    }

    #[test]
    fn apple_curve_is_linear_in_headroom() {
        let params = GainMapParams {
            min_log2_gain: [0.0; 3],
            max_log2_gain: [3.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.0; 3],
            offset_hdr: [0.0; 3],
            hdr_capacity_min: 0.0,
            hdr_capacity_max: 3.0,
            base_is_hdr: false,
            curve: GainCurve::AppleLinear,
        };
        let base = Rgba8Image::filled(2, 2, [255, 255, 255, 255]).unwrap();
        let out = apply_gain_map(&base, None, &map(255, params.clone()), 8.0).unwrap();
        assert!((out.data[0] - 8.0).abs() < 1e-4);
        let out = apply_gain_map(&base, None, &map(0, params), 8.0).unwrap();
        assert!((out.data[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn hostile_arguments_never_produce_non_finite_output() {
        let params = iso_params(&iso_bytes()).unwrap();
        let base = Rgba8Image::filled(3, 3, [200, 10, 90, 255]).unwrap();
        for h in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -3.0, 0.0, 1e30] {
            let out = apply_gain_map(&base, None, &map(128, params.clone()), h).unwrap();
            assert!(out.data.iter().all(|v| v.is_finite() && *v >= 0.0), "{h}");
        }
        let mut bad = params;
        bad.gamma[0] = f32::NAN;
        assert!(apply_gain_map(&base, None, &map(1, bad), 2.0).is_err());
        let gone = GainMap {
            image: None,
            image_unavailable: Some("no decoder".into()),
            ..map(0, iso_params(&iso_bytes()).unwrap())
        };
        let e = apply_gain_map(&base, None, &gone, 2.0).unwrap_err();
        assert!(e.to_string().contains("no decoder"));
    }

    #[test]
    fn files_without_gain_maps_give_none() {
        assert!(gain_map(b"").unwrap().is_none());
        assert!(gain_map(b"\xFF\xD8\xFF\xD9").unwrap().is_none());
        assert!(gain_map_summary(b"hello world").unwrap().is_none());
    }
}
