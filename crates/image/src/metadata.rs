//! One merged view of a file's EXIF, IPTC and XMP, with the sources kept alongside.
//!
//! Precedence follows the Metadata Working Group's guidance in its simplest form: descriptive
//! and rights fields prefer XMP, then IPTC, then EXIF; camera and exposure fields come from
//! EXIF; the capture time prefers EXIF `DateTimeOriginal`, then XMP `photoshop:DateCreated`,
//! `xmp:CreateDate`, then IPTC.

use crate::container::Embedded;
use crate::exif::{Exif, Gps, parse_exif};
use crate::iptc::{Iptc, parse_iptc};
use crate::xmp::{Xmp, parse_xmp};
use serde::{Deserialize, Serialize};

/// Everything a catalogue shows or searches about a still image.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    /// Camera maker.
    pub camera_make: Option<String>,
    /// Camera model.
    pub camera_model: Option<String>,
    /// Lens model.
    pub lens: Option<String>,
    /// Software that wrote the file.
    pub software: Option<String>,
    /// When the photo was taken, ISO 8601.
    pub capture_time: Option<String>,
    /// Seconds.
    pub exposure_time: Option<f64>,
    /// F-number.
    pub f_number: Option<f64>,
    /// ISO speed.
    pub iso: Option<u32>,
    /// Millimetres.
    pub focal_length: Option<f64>,
    /// 35 mm-equivalent millimetres.
    pub focal_length_35mm: Option<u32>,
    /// EV.
    pub exposure_bias: Option<f64>,
    /// Whether the flash fired.
    pub flash_fired: Option<bool>,
    /// Where the photo was taken.
    pub gps: Option<Gps>,

    /// Title.
    pub title: Option<String>,
    /// Caption / description.
    pub caption: Option<String>,
    /// Short accessibility text.
    pub alt_text: Option<String>,
    /// Long accessibility description.
    pub extended_description: Option<String>,
    /// Headline.
    pub headline: Option<String>,
    /// Keywords, de-duplicated, first spelling kept.
    pub keywords: Vec<String>,
    /// Creators.
    pub creators: Vec<String>,
    /// Credit line.
    pub credit: Option<String>,
    /// Source.
    pub source: Option<String>,
    /// Copyright notice.
    pub copyright: Option<String>,
    /// Rights usage terms.
    pub usage_terms: Option<String>,
    /// Rights web statement (licence URL).
    pub web_statement: Option<String>,
    /// `true` copyrighted, `false` public domain, `None` unknown.
    pub copyright_marked: Option<bool>,
    /// Star rating, −1 (rejected) to 5.
    pub rating: Option<i8>,

    /// Place within the city.
    pub sublocation: Option<String>,
    /// City.
    pub city: Option<String>,
    /// Province or state.
    pub state: Option<String>,
    /// Country name.
    pub country: Option<String>,
    /// ISO country code.
    pub country_code: Option<String>,

    /// The decoded EXIF, when present.
    pub exif: Option<Exif>,
    /// The decoded XMP, when present.
    pub xmp: Option<Xmp>,
    /// The decoded IPTC-IIM, when present.
    pub iptc: Option<Iptc>,
}

impl Metadata {
    /// Decode and merge the blocks. Problems with a block are returned as warnings; the other
    /// blocks are still read.
    pub fn from_embedded(e: &Embedded<'_>) -> (Metadata, Vec<String>) {
        let mut warnings = Vec::new();
        let exif = e.exif.as_deref().and_then(|b| {
            let x = parse_exif(b);
            if x.is_none() {
                warnings.push("EXIF block is not a readable TIFF structure".to_string());
            }
            x
        });
        let xmp = e.xmp.as_deref().and_then(|b| match parse_xmp(b) {
            Ok(x) => Some(x),
            Err(err) => {
                warnings.push(format!("XMP: {err}"));
                None
            }
        });
        let iptc = e.iptc.as_deref().map(parse_iptc);
        (Self::merge(exif, xmp, iptc), warnings)
    }

    /// Merge decoded blocks.
    pub fn merge(exif: Option<Exif>, xmp: Option<Xmp>, iptc: Option<Iptc>) -> Metadata {
        let (x, p, i) = (exif.as_ref(), xmp.as_ref(), iptc.as_ref());
        let pick =
            |a: Option<&Option<String>>, b: Option<&Option<String>>, c: Option<&Option<String>>| {
                [a, b, c]
                    .into_iter()
                    .flatten()
                    .flatten()
                    .find(|s| !s.is_empty())
                    .cloned()
            };
        let list = |a: Option<&Vec<String>>, b: Option<&Vec<String>>, c: Vec<String>| {
            let mut out: Vec<String> = Vec::new();
            for s in a
                .into_iter()
                .flatten()
                .chain(b.into_iter().flatten())
                .chain(c.iter())
            {
                if !out.iter().any(|o| o.eq_ignore_ascii_case(s)) {
                    out.push(s.clone());
                }
            }
            out
        };
        let creators_exif: Vec<String> = x.and_then(|x| x.artist.clone()).into_iter().collect();
        Metadata {
            camera_make: x.and_then(|x| x.make.clone()),
            camera_model: x.and_then(|x| x.model.clone()),
            lens: x.and_then(|x| x.lens_model.clone()),
            software: pick(x.map(|x| &x.software), p.map(|p| &p.creator_tool), None),
            capture_time: pick(
                x.map(|x| &x.capture_time),
                p.map(|p| &p.date_created),
                p.map(|p| &p.create_date),
            )
            .or_else(|| i.and_then(|i| i.date_created.clone())),
            exposure_time: x.and_then(|x| x.exposure_time),
            f_number: x.and_then(|x| x.f_number),
            iso: x.and_then(|x| x.iso),
            focal_length: x.and_then(|x| x.focal_length),
            focal_length_35mm: x.and_then(|x| x.focal_length_35mm),
            exposure_bias: x.and_then(|x| x.exposure_bias),
            flash_fired: x.and_then(|x| x.flash_fired),
            gps: x.and_then(|x| x.gps),
            title: pick(p.map(|p| &p.title), i.map(|i| &i.object_name), None),
            caption: pick(
                p.map(|p| &p.description),
                i.map(|i| &i.caption),
                x.map(|x| &x.description),
            ),
            alt_text: p.and_then(|p| p.alt_text.clone()),
            extended_description: p.and_then(|p| p.extended_description.clone()),
            headline: pick(p.map(|p| &p.headline), i.map(|i| &i.headline), None),
            keywords: list(p.map(|p| &p.keywords), i.map(|i| &i.keywords), Vec::new()),
            creators: list(p.map(|p| &p.creators), i.map(|i| &i.bylines), creators_exif),
            credit: pick(p.map(|p| &p.credit), i.map(|i| &i.credit), None),
            source: pick(p.map(|p| &p.source), i.map(|i| &i.source), None),
            copyright: pick(
                p.map(|p| &p.rights),
                i.map(|i| &i.copyright),
                x.map(|x| &x.copyright),
            ),
            usage_terms: p.and_then(|p| p.usage_terms.clone()),
            web_statement: p.and_then(|p| p.web_statement.clone()),
            copyright_marked: p.and_then(|p| p.copyright_marked),
            rating: p.and_then(|p| p.rating),
            sublocation: pick(p.map(|p| &p.sublocation), i.map(|i| &i.sublocation), None),
            city: pick(p.map(|p| &p.city), i.map(|i| &i.city), None),
            state: pick(p.map(|p| &p.state), i.map(|i| &i.state), None),
            country: pick(p.map(|p| &p.country), i.map(|i| &i.country), None),
            country_code: pick(p.map(|p| &p.country_code), i.map(|i| &i.country_code), None),
            exif,
            xmp,
            iptc,
        }
    }

    /// A copy without location: GPS and the place names. For public renditions and exports.
    pub fn without_location(&self) -> Metadata {
        let mut m = self.clone();
        m.gps = None;
        m.sublocation = None;
        m.city = None;
        m.state = None;
        m.country = None;
        m.country_code = None;
        if let Some(x) = m.exif.as_mut() {
            x.gps = None;
        }
        if let Some(p) = m.xmp.as_mut() {
            p.sublocation = None;
            p.city = None;
            p.state = None;
            p.country = None;
            p.country_code = None;
        }
        if let Some(i) = m.iptc.as_mut() {
            i.sublocation = None;
            i.city = None;
            i.state = None;
            i.country = None;
            i.country_code = None;
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn xmp_wins_for_descriptive_fields_and_exif_for_camera() {
        let exif_blob = crate::exif::tests::camera_exif();
        let mut iptc = crate::iptc::tests::record(5, b"IIM title");
        iptc.extend_from_slice(&crate::iptc::tests::record(25, b"Paris"));
        iptc.extend_from_slice(&crate::iptc::tests::record(25, b"night"));
        let e = Embedded {
            exif: Some(Cow::Owned(exif_blob)),
            xmp: Some(Cow::Borrowed(crate::xmp::tests::PACKET.as_bytes())),
            iptc: Some(Cow::Owned(iptc)),
            icc: None,
        };
        let (m, warnings) = Metadata::from_embedded(&e);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(m.camera_model.as_deref(), Some("Canon EOS R5"));
        assert_eq!(m.title.as_deref(), Some("Eiffel Tower"));
        assert_eq!(m.copyright.as_deref(), Some("© 2024 Ada"));
        assert_eq!(m.capture_time.as_deref(), Some("2024-05-01T14:30:15+02:00"));
        assert_eq!(
            m.keywords,
            vec!["paris", "tower", "dusk", "night"],
            "case-insensitive de-duplication"
        );
        assert_eq!(m.creators, vec!["Ada", "Grace", "Ada Photographer"]);
        assert_eq!(
            m.alt_text.as_deref(),
            Some("The Eiffel Tower lit up at dusk")
        );
        assert!(m.gps.is_some());

        let public = m.without_location();
        assert!(public.gps.is_none() && public.city.is_none());
        assert!(public.exif.as_ref().is_some_and(|x| x.gps.is_none()));
    }

    #[test]
    fn broken_blocks_become_warnings() {
        let e = Embedded {
            exif: Some(Cow::Borrowed(b"II*\0\xFF\xFF\xFF\xFF")),
            xmp: Some(Cow::Borrowed(b"<a><b></a>")),
            ..Default::default()
        };
        let (m, warnings) = Metadata::from_embedded(&e);
        assert_eq!(m.title, None);
        assert!(!warnings.is_empty());
    }
}
