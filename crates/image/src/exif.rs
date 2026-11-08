//! EXIF (CIPA DC-008 / JEITA CP-3451): IFD0, the Exif IFD, the GPS IFD and the IFD1 thumbnail,
//! decoded into the fields asset catalogues show and search. Also writes the small EXIF block
//! renditions carry.

use crate::tiff::{Ifd, Tiff};
use crate::{Orientation, bytes::clean_text};
use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};

const EXIF_IFD: u16 = 0x8769;
const GPS_IFD: u16 = 0x8825;
const INTEROP_IFD: u16 = 0xA005;

/// GPS position (WGS 84): signed decimal degrees, north and east positive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gps {
    /// Degrees, −90..=90.
    pub latitude: f64,
    /// Degrees, −180..=180.
    pub longitude: f64,
    /// Metres above sea level, negative below.
    pub altitude: Option<f64>,
}

/// The EXIF fields this crate decodes. Everything is optional; absent or damaged is `None`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Exif {
    /// Camera maker (tag 271).
    pub make: Option<String>,
    /// Camera model (272).
    pub model: Option<String>,
    /// Body serial number (0xA431).
    pub body_serial: Option<String>,
    /// Software that wrote the file (305).
    pub software: Option<String>,
    /// Lens maker (0xA433).
    pub lens_make: Option<String>,
    /// Lens model (0xA434).
    pub lens_model: Option<String>,
    /// Photographer (315).
    pub artist: Option<String>,
    /// Copyright notice (33432), photographer part.
    pub copyright: Option<String>,
    /// Image description (270).
    pub description: Option<String>,
    /// Orientation (274).
    pub orientation: Option<Orientation>,
    /// When the photo was taken: DateTimeOriginal (or DateTime) as ISO 8601, with the UTC
    /// offset when OffsetTimeOriginal records one.
    pub capture_time: Option<String>,
    /// Exposure time in seconds (0x829A).
    pub exposure_time: Option<f64>,
    /// F-number (0x829D).
    pub f_number: Option<f64>,
    /// ISO speed (0x8827).
    pub iso: Option<u32>,
    /// Focal length in millimetres (0x920A).
    pub focal_length: Option<f64>,
    /// 35 mm-equivalent focal length in millimetres (0xA405).
    pub focal_length_35mm: Option<u32>,
    /// Exposure bias in EV (0x9204).
    pub exposure_bias: Option<f64>,
    /// Whether the flash fired (0x9209 bit 0).
    pub flash_fired: Option<bool>,
    /// ColorSpace (0xA001): 1 sRGB, 0xFFFF uncalibrated.
    pub color_space: Option<u16>,
    /// Interoperability index (`R98` sRGB, `R03` Adobe RGB).
    pub interop_index: Option<String>,
    /// Pixel dimensions as recorded (0xA002, 0xA003).
    pub pixel_width: Option<u32>,
    /// See [`Exif::pixel_width`].
    pub pixel_height: Option<u32>,
    /// GPS position.
    pub gps: Option<Gps>,
    /// Byte range of the IFD1 JPEG thumbnail inside the EXIF blob.
    #[serde(skip)]
    pub thumbnail: Option<(usize, usize)>,
}

impl Exif {
    /// Whether the EXIF says "Adobe RGB" without an ICC profile (DCF option file convention).
    pub fn adobe_rgb_hint(&self) -> bool {
        self.color_space == Some(0xFFFF) && self.interop_index.as_deref() == Some("R03")
    }
}

/// Decode an EXIF TIFF structure. `None` when the blob is not one.
pub fn parse_exif(blob: &[u8]) -> Option<Exif> {
    let t = Tiff::new(blob)?;
    let first = t.first_ifd()?;
    let ifd0 = t.ifd(first)?;
    let mut x = Exif::default();
    let text = |ifd: &Ifd, tag| ifd.get(tag).and_then(|e| t.text(e));
    x.make = text(&ifd0, 0x010F);
    x.model = text(&ifd0, 0x0110);
    x.software = text(&ifd0, 0x0131);
    x.artist = text(&ifd0, 0x013B);
    x.copyright = text(&ifd0, 0x8298);
    x.description = text(&ifd0, 0x010E);
    x.orientation = ifd0
        .get(0x0112)
        .and_then(|e| t.uint(e))
        .and_then(|v| Orientation::from_exif(v as u16));
    let mut date = text(&ifd0, 0x0132);
    let mut offset = None;

    let sub = |ifd: &Ifd, tag| {
        ifd.get(tag)
            .and_then(|e| t.uint(e))
            .and_then(|p| t.ifd(p as usize))
    };
    if let Some(ex) = sub(&ifd0, EXIF_IFD) {
        x.body_serial = text(&ex, 0xA431);
        x.lens_make = text(&ex, 0xA433);
        x.lens_model = text(&ex, 0xA434);
        if let Some(d) = text(&ex, 0x9003) {
            date = Some(d);
            offset = text(&ex, 0x9011);
        } else {
            offset = text(&ex, 0x9010);
        }
        let rational = |tag| ex.get(tag).and_then(|e| t.rational(e, 0));
        x.exposure_time = rational(0x829A).filter(|v| *v > 0.0);
        x.f_number = rational(0x829D).filter(|v| *v > 0.0);
        x.focal_length = rational(0x920A).filter(|v| *v > 0.0);
        x.exposure_bias = rational(0x9204);
        let uint = |tag| ex.get(tag).and_then(|e| t.uint(e));
        x.iso = uint(0x8827).or_else(|| uint(0x8833)).filter(|v| *v > 0);
        x.focal_length_35mm = uint(0xA405).filter(|v| *v > 0);
        x.flash_fired = uint(0x9209).map(|v| v & 1 == 1);
        x.color_space = uint(0xA001).and_then(|v| u16::try_from(v).ok());
        x.pixel_width = uint(0xA002).filter(|v| *v > 0);
        x.pixel_height = uint(0xA003).filter(|v| *v > 0);
        if let Some(interop) = sub(&ex, INTEROP_IFD) {
            x.interop_index = interop
                .get(0x0001)
                .and_then(|e| t.bytes(e))
                .and_then(clean_text);
        }
    }
    x.capture_time = date
        .as_deref()
        .and_then(|d| iso_datetime(d, offset.as_deref()));
    x.gps = sub(&ifd0, GPS_IFD).and_then(|g| gps(&t, &g));
    x.thumbnail = thumbnail(&t, &ifd0, first);
    Some(x)
}

/// `YYYY:MM:DD HH:MM:SS` (+ optional `±HH:MM`) as ISO 8601. All-zero and invalid dates give `None`.
fn iso_datetime(date: &str, offset: Option<&str>) -> Option<String> {
    let naive = NaiveDateTime::parse_from_str(date.trim(), "%Y:%m:%d %H:%M:%S").ok()?;
    let offset = offset.and_then(|o| {
        let o = o.trim();
        let sign = match o.get(0..1)? {
            "+" => 1,
            "-" => -1,
            _ => return None,
        };
        let h: i32 = o.get(1..3)?.parse().ok()?;
        let m: i32 = o.get(4..6)?.parse().ok()?;
        FixedOffset::east_opt(sign * (h * 3600 + m * 60))
    });
    match offset {
        Some(off) => off
            .from_local_datetime(&naive)
            .single()
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()),
        None => Some(naive.format("%Y-%m-%dT%H:%M:%S").to_string()),
    }
}

fn gps(t: &Tiff<'_>, g: &Ifd) -> Option<Gps> {
    let dms = |tag| {
        let e = g.get(tag)?;
        let (d, m, s) = (
            t.rational(e, 0)?,
            t.rational(e, 1).unwrap_or(0.0),
            t.rational(e, 2).unwrap_or(0.0),
        );
        Some(d + m / 60.0 + s / 3600.0)
    };
    let reference = |tag| {
        g.get(tag)
            .and_then(|e| t.bytes(e))
            .and_then(|b| b.first().copied())
    };
    let mut lat = dms(0x0002)?;
    let mut lon = dms(0x0004)?;
    if reference(0x0001) == Some(b'S') {
        lat = -lat;
    }
    if reference(0x0003) == Some(b'W') {
        lon = -lon;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let altitude = g
        .get(0x0006)
        .and_then(|e| t.rational(e, 0))
        .map(|a| if reference(0x0005) == Some(1) { -a } else { a });
    Some(Gps {
        latitude: lat,
        longitude: lon,
        altitude,
    })
}

/// The IFD1 JPEG thumbnail (JPEGInterchangeFormat / Length), when it really starts a JPEG.
fn thumbnail(t: &Tiff<'_>, ifd0: &Ifd, first: usize) -> Option<(usize, usize)> {
    if ifd0.next == 0 || ifd0.next == first {
        return None;
    }
    let ifd1 = t.ifd(ifd0.next)?;
    let off = t.uint(ifd1.get(0x0201)?)? as usize;
    let len = t.uint(ifd1.get(0x0202)?)? as usize;
    let end = off.checked_add(len)?;
    (len > 4 && t.data().get(off..end)?.starts_with(&[0xFF, 0xD8])).then_some((off, len))
}

/// A little-endian EXIF block with only Orientation and, when given, Artist and Copyright.
/// Renditions use it to keep attribution without carrying camera, GPS or thumbnail data.
pub(crate) fn minimal_exif(
    orientation: Orientation,
    artist: Option<&str>,
    copyright: Option<&str>,
) -> Vec<u8> {
    let mut entries: Vec<(u16, u16, Vec<u8>)> =
        vec![(0x0112, 3, orientation.to_exif().to_le_bytes().to_vec())];
    for (tag, value) in [(0x013B, artist), (0x8298, copyright)] {
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            let mut b: Vec<u8> = v.bytes().filter(|&c| c != 0).collect();
            b.push(0);
            entries.push((tag, 2, b));
        }
    }
    let mut out = b"II*\0\x08\0\0\0".to_vec();
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut data_off = 8 + 2 + entries.len() * 12 + 4;
    let mut data = Vec::new();
    for (tag, typ, value) in &entries {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&typ.to_le_bytes());
        let count = if *typ == 3 { 1u32 } else { value.len() as u32 };
        out.extend_from_slice(&count.to_le_bytes());
        if value.len() <= 4 {
            let mut v = value.clone();
            v.resize(4, 0);
            out.extend_from_slice(&v);
        } else {
            out.extend_from_slice(&(data_off as u32).to_le_bytes());
            data.extend_from_slice(value);
            data_off += value.len();
            if value.len() % 2 == 1 {
                data.push(0);
                data_off += 1;
            }
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::tiff::testutil::{Val, build};

    pub(crate) fn camera_exif() -> Vec<u8> {
        build(
            vec![
                (0x010F, Val::Ascii("Canon")),
                (0x0110, Val::Ascii("Canon EOS R5")),
                (0x0112, Val::Short(6)),
                (0x013B, Val::Ascii("Ada Photographer")),
                (0x8298, Val::Ascii("(c) 2024 Ada")),
            ],
            vec![
                (
                    EXIF_IFD,
                    vec![
                        (0x829A, Val::Rational(vec![(1, 250)])),
                        (0x829D, Val::Rational(vec![(28, 10)])),
                        (0x8827, Val::Short(400)),
                        (0x9003, Val::Ascii("2024:05:01 14:30:15")),
                        (0x9011, Val::Ascii("+02:00")),
                        (0x9204, Val::SRational(-2, 3)),
                        (0x9209, Val::Short(0x19)),
                        (0x920A, Val::Rational(vec![(50, 1)])),
                        (0xA434, Val::Ascii("RF50mm F1.2 L USM")),
                    ],
                ),
                (
                    GPS_IFD,
                    vec![
                        (0x0001, Val::Ascii("N")),
                        (0x0002, Val::Rational(vec![(48, 1), (51, 1), (2964, 100)])),
                        (0x0003, Val::Ascii("W")),
                        (0x0004, Val::Rational(vec![(2, 1), (17, 1), (4020, 100)])),
                        (0x0005, Val::Bytes(vec![0])),
                        (0x0006, Val::Rational(vec![(35, 1)])),
                    ],
                ),
            ],
        )
    }

    #[test]
    fn decodes_camera_capture_and_gps() {
        let x = parse_exif(&camera_exif()).unwrap();
        assert_eq!(x.make.as_deref(), Some("Canon"));
        assert_eq!(x.model.as_deref(), Some("Canon EOS R5"));
        assert_eq!(x.lens_model.as_deref(), Some("RF50mm F1.2 L USM"));
        assert_eq!(x.orientation, Some(Orientation::Rotate90));
        assert_eq!(x.capture_time.as_deref(), Some("2024-05-01T14:30:15+02:00"));
        assert_eq!(x.exposure_time, Some(0.004));
        assert_eq!(x.f_number, Some(2.8));
        assert_eq!(x.iso, Some(400));
        assert_eq!(x.focal_length, Some(50.0));
        assert_eq!(x.flash_fired, Some(true));
        assert!((x.exposure_bias.unwrap() + 2.0 / 3.0).abs() < 1e-9);
        let g = x.gps.unwrap();
        assert!((g.latitude - 48.858_233).abs() < 1e-5, "{}", g.latitude);
        assert!((g.longitude + 2.294_5).abs() < 1e-4, "{}", g.longitude);
        assert_eq!(g.altitude, Some(35.0));
    }

    #[test]
    fn dates_are_validated_with_chrono() {
        assert_eq!(
            iso_datetime("2024:02:29 00:00:00", None).as_deref(),
            Some("2024-02-29T00:00:00")
        );
        assert_eq!(iso_datetime("2023:02:29 00:00:00", None), None);
        assert_eq!(iso_datetime("0000:00:00 00:00:00", None), None);
        assert_eq!(
            iso_datetime("2024:01:01 10:00:00", Some("-05:30")).as_deref(),
            Some("2024-01-01T10:00:00-05:30")
        );
        assert_eq!(
            iso_datetime("2024:01:01 10:00:00", Some("junk")).as_deref(),
            Some("2024-01-01T10:00:00")
        );
    }

    #[test]
    fn minimal_exif_round_trips() {
        let blob = minimal_exif(
            Orientation::Identity,
            Some("Ada"),
            Some("(c) 2024 Ada, all rights reserved"),
        );
        let x = parse_exif(&blob).unwrap();
        assert_eq!(x.orientation, Some(Orientation::Identity));
        assert_eq!(x.artist.as_deref(), Some("Ada"));
        assert_eq!(
            x.copyright.as_deref(),
            Some("(c) 2024 Ada, all rights reserved")
        );
        assert_eq!(x.gps, None);
        assert_eq!(x.make, None);
    }

    #[test]
    fn out_of_range_gps_is_dropped() {
        let blob = build(
            vec![],
            vec![(
                GPS_IFD,
                vec![
                    (0x0002, Val::Rational(vec![(95, 1), (0, 1), (0, 1)])),
                    (0x0004, Val::Rational(vec![(10, 1)])),
                ],
            )],
        );
        assert_eq!(parse_exif(&blob).unwrap().gps, None);
    }
}
