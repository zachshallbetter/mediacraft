//! IPTC-IIM (IPTC Information Interchange Model 4.2): the record-2 datasets photo tools still
//! write alongside XMP.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// IPTC-IIM fields this crate reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Iptc {
    /// 2:05 Object Name (title).
    pub object_name: Option<String>,
    /// 2:25 Keywords.
    pub keywords: Vec<String>,
    /// 2:80 By-line (creators).
    pub bylines: Vec<String>,
    /// 2:90 City.
    pub city: Option<String>,
    /// 2:92 Sub-location.
    pub sublocation: Option<String>,
    /// 2:95 Province/State.
    pub state: Option<String>,
    /// 2:100 Country code.
    pub country_code: Option<String>,
    /// 2:101 Country name.
    pub country: Option<String>,
    /// 2:105 Headline.
    pub headline: Option<String>,
    /// 2:110 Credit.
    pub credit: Option<String>,
    /// 2:115 Source.
    pub source: Option<String>,
    /// 2:116 Copyright notice.
    pub copyright: Option<String>,
    /// 2:120 Caption/Abstract.
    pub caption: Option<String>,
    /// 2:55 Date Created as `YYYY-MM-DD`.
    pub date_created: Option<String>,
}

/// Read IIM records. Damage ends the walk; what was read before it is kept.
pub fn parse_iptc(d: &[u8]) -> Iptc {
    let mut x = Iptc::default();
    let mut utf8 = false;
    let mut p = 0usize;
    let mut seen = 0;
    while seen < 10_000 {
        seen += 1;
        let (Some(&0x1C), Some(&record), Some(&dataset)) = (d.get(p), d.get(p + 1), d.get(p + 2))
        else {
            break;
        };
        let Some(len) = crate::bytes::be_u16(d, p + 3) else {
            break;
        };
        let mut start = p + 5;
        let len = if len & 0x8000 != 0 {
            // Extended dataset: the low bits say how many bytes hold the real length.
            let n = usize::from(len & 0x7FFF);
            if n == 0 || n > 4 {
                break;
            }
            let Some(bytes) = d.get(start..start + n) else {
                break;
            };
            start += n;
            bytes
                .iter()
                .fold(0usize, |acc, &b| (acc << 8) | usize::from(b))
        } else {
            usize::from(len)
        };
        let Some(end) = start.checked_add(len) else {
            break;
        };
        let Some(value) = d.get(start..end) else {
            break;
        };
        p = end;
        // 1:90 Coded Character Set: ESC % G declares UTF-8.
        if record == 1 && dataset == 90 {
            utf8 = value == b"\x1b%G";
            continue;
        }
        if record != 2 {
            continue;
        }
        let Some(text) = decode(value, utf8) else {
            continue;
        };
        let set = |f: &mut Option<String>| {
            if f.is_none() {
                *f = Some(text.clone());
            }
        };
        match dataset {
            5 => set(&mut x.object_name),
            25 => x.keywords.push(text),
            80 => x.bylines.push(text),
            90 => set(&mut x.city),
            92 => set(&mut x.sublocation),
            95 => set(&mut x.state),
            100 => set(&mut x.country_code),
            101 => set(&mut x.country),
            105 => set(&mut x.headline),
            110 => set(&mut x.credit),
            115 => set(&mut x.source),
            116 => set(&mut x.copyright),
            120 => set(&mut x.caption),
            55 if x.date_created.is_none() => {
                x.date_created = NaiveDate::parse_from_str(&text, "%Y%m%d")
                    .ok()
                    .map(|d| d.format("%Y-%m-%d").to_string());
            }
            _ => {}
        }
    }
    x
}

/// UTF-8 when declared or valid, otherwise Latin-1 (the IIM default for Western tools).
fn decode(v: &[u8], declared_utf8: bool) -> Option<String> {
    let s = match std::str::from_utf8(v) {
        Ok(s) => s.to_string(),
        Err(_) if declared_utf8 => String::from_utf8_lossy(v).into_owned(),
        Err(_) => v.iter().map(|&b| char::from(b)).collect(),
    };
    let t = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn record(dataset: u8, value: &[u8]) -> Vec<u8> {
        let mut b = vec![0x1C, 2, dataset];
        b.extend_from_slice(&(value.len() as u16).to_be_bytes());
        b.extend_from_slice(value);
        b
    }

    #[test]
    fn reads_record_two_and_repeatable_fields() {
        let mut d = vec![0x1C, 1, 90, 0, 3, 0x1B, b'%', b'G'];
        for (ds, v) in [
            (5, &b"Title"[..]),
            (25, b"one"),
            (25, b"two"),
            (80, b"Ada"),
            (116, "© Ada".as_bytes()),
            (55, b"20240501"),
            (120, b"A caption"),
        ] {
            d.extend_from_slice(&record(ds, v));
        }
        let x = parse_iptc(&d);
        assert_eq!(x.object_name.as_deref(), Some("Title"));
        assert_eq!(x.keywords, vec!["one", "two"]);
        assert_eq!(x.bylines, vec!["Ada"]);
        assert_eq!(x.copyright.as_deref(), Some("© Ada"));
        assert_eq!(x.date_created.as_deref(), Some("2024-05-01"));
        assert_eq!(x.caption.as_deref(), Some("A caption"));
    }

    #[test]
    fn latin1_fallback_and_damage() {
        let mut d = record(90, b"M\xFCnchen");
        d.extend_from_slice(&[0x1C, 2, 101, 0x00, 0x50, b'x']); // length runs past the end
        let x = parse_iptc(&d);
        assert_eq!(x.city.as_deref(), Some("München"));
        assert_eq!(x.country, None);
        assert_eq!(parse_iptc(&record(55, b"20231301")).date_created, None);
    }
}
