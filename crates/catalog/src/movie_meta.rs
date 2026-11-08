//! Capture metadata of MP4 and QuickTime movies: the camera, when, and where.
//!
//! Phones and cameras write it in three places, read here in order of preference:
//! * Apple's `moov/meta` keys (`com.apple.quicktime.creationdate`, `.make`, `.model`,
//!   `.location.ISO6709`), which iPhones write with the local UTC offset;
//! * QuickTime `udta` atoms (`©day`, `©mak`, `©mod`, `©xyz`), which Android and many cameras
//!   write;
//! * the movie header's creation time (`mvhd`), in UTC, when nothing else gives a date.

use chrono::{DateTime, FixedOffset, NaiveDateTime, SecondsFormat, TimeZone, Utc};

/// What a movie says about its capture.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MovieTags {
    pub make: Option<String>,
    pub model: Option<String>,
    /// RFC 3339, with the recorded UTC offset when there is one.
    pub created: Option<String>,
    /// Latitude and longitude in degrees.
    pub location: Option<(f64, f64)>,
}

/// Seconds from 1904-01-01 (the QuickTime epoch) to 1970-01-01.
const QUICKTIME_EPOCH_OFFSET: i64 = 2_082_844_800;

pub(crate) fn from_mp4(m: &craft_isobmff::Mp4File) -> MovieTags {
    let get = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| m.metadata.get(k))
            .map(|v| v.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let created = get(&["com.apple.quicktime.creationdate", "©day"])
        .as_deref()
        .and_then(parse_date)
        .or_else(|| movie_header_time(m.creation_time));
    let location = get(&["com.apple.quicktime.location.ISO6709", "©xyz"])
        .as_deref()
        .and_then(parse_iso6709);
    MovieTags {
        make: get(&["com.apple.quicktime.make", "©mak"]),
        model: get(&["com.apple.quicktime.model", "©mod"]),
        created,
        location,
    }
}

/// `mvhd` creation time as UTC. Zero means unset; dates before 1990 are a camera's unset clock.
fn movie_header_time(secs_since_1904: u64) -> Option<String> {
    let unix = i64::try_from(secs_since_1904)
        .ok()?
        .checked_sub(QUICKTIME_EPOCH_OFFSET)?;
    let t = Utc.timestamp_opt(unix, 0).single()?;
    (t.timestamp() >= 631_152_000).then(|| t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// The date forms movies carry: `2023-07-04T18:22:10-0700` (Apple), RFC 3339 with or without
/// fractional seconds, `20230704T182210.000Z` (Android), or a bare date-time taken as UTC.
fn parse_date(s: &str) -> Option<String> {
    let offset_forms = [
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%.f%:z",
    ];
    let with_offset = DateTime::parse_from_rfc3339(s).ok().or_else(|| {
        offset_forms
            .iter()
            .find_map(|f| DateTime::parse_from_str(s, f).ok())
    });
    if let Some(t) = with_offset {
        return Some(t.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    let utc_forms = [
        "%Y%m%dT%H%M%S%.fZ",
        "%Y%m%dT%H%M%SZ",
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];
    let naive = utc_forms
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())?;
    let t: DateTime<FixedOffset> = Utc.from_utc_datetime(&naive).fixed_offset();
    Some(t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// ISO 6709 point: `+47.6062-122.3321+050.000/`. Each coordinate's integer part may also be
/// degrees and minutes (`±DDMM.M`, `±DDDMM.M`) or degrees, minutes and seconds.
fn parse_iso6709(s: &str) -> Option<(f64, f64)> {
    let s = s.trim().trim_end_matches('/');
    let starts: Vec<usize> = s
        .char_indices()
        .filter(|(_, c)| matches!(c, '+' | '-'))
        .map(|(i, _)| i)
        .collect();
    let (&a, &b) = (starts.first()?, starts.get(1)?);
    if a != 0 {
        return None;
    }
    let end = starts.get(2).copied().unwrap_or(s.len());
    let lat = coordinate(s.get(a..b)?, 2)?;
    let lon = coordinate(s.get(b..end)?, 3)?;
    ((-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
        && !(lat == 0.0 && lon == 0.0))
        .then_some((lat, lon))
}

/// One signed coordinate whose degrees take `deg_digits` digits.
fn coordinate(s: &str, deg_digits: usize) -> Option<f64> {
    let (sign, body) = match s.as_bytes().first()? {
        b'+' => (1.0, s.get(1..)?),
        b'-' => (-1.0, s.get(1..)?),
        _ => return None,
    };
    if body.is_empty() || !body.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return None;
    }
    let int_len = body.find('.').unwrap_or(body.len());
    let v: f64 = body.parse().ok()?;
    let degrees = match int_len.checked_sub(deg_digits)? {
        0 => v,
        // DDMM.M: minutes are the last two integer digits.
        2 => (v / 100.0).trunc() + (v % 100.0) / 60.0,
        // DDMMSS.S
        4 => (v / 10_000.0).trunc() + ((v / 100.0).trunc() % 100.0) / 60.0 + (v % 100.0) / 3600.0,
        _ => return None,
    };
    degrees.is_finite().then_some(sign * degrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso6709_points_in_every_form() {
        assert_eq!(
            parse_iso6709("+47.6062-122.3321+050.000/"),
            Some((47.6062, -122.3321))
        );
        assert_eq!(
            parse_iso6709("+35.6895+139.6917/"),
            Some((35.6895, 139.6917))
        );
        assert_eq!(
            parse_iso6709("-33.8688+151.2093"),
            Some((-33.8688, 151.2093))
        );
        let (lat, lon) = parse_iso6709("+4736.372-12219.926/").unwrap_or_default();
        assert!(
            (lat - 47.6062).abs() < 1e-4 && (lon + 122.3321).abs() < 1e-4,
            "{lat} {lon}"
        );
        let (lat, lon) = parse_iso6709("+403630-0735730/").unwrap_or_default();
        assert!(
            (lat - 40.6083).abs() < 1e-3 && (lon + 73.9583).abs() < 1e-3,
            "{lat} {lon}"
        );
        for bad in [
            "",
            "/",
            "47.6-122.3",
            "+91.0+10.0/",
            "+10.0+181.0/",
            "+0.0000+0.0000/",
            "+ab-cd/",
            "+1-",
        ] {
            assert_eq!(parse_iso6709(bad), None, "{bad}");
        }
    }

    #[test]
    fn dates_keep_their_offset_or_are_utc() {
        assert_eq!(
            parse_date("2023-07-04T18:22:10-0700").as_deref(),
            Some("2023-07-04T18:22:10-07:00")
        );
        assert_eq!(
            parse_date("2023-07-04T18:22:10+02:00").as_deref(),
            Some("2023-07-04T18:22:10+02:00")
        );
        assert_eq!(
            parse_date("20230704T182210.000Z").as_deref(),
            Some("2023-07-04T18:22:10Z")
        );
        assert_eq!(
            parse_date("2023-07-04T18:22:10.123Z").as_deref(),
            Some("2023-07-04T18:22:10Z")
        );
        assert_eq!(
            parse_date("2023-07-04 18:22:10").as_deref(),
            Some("2023-07-04T18:22:10Z")
        );
        assert_eq!(parse_date("yesterday"), None);
    }

    #[test]
    fn movie_header_time_is_utc_and_ignores_unset_clocks() {
        // 2024-01-01T00:00:00Z.
        assert_eq!(
            movie_header_time(1_704_067_200 + 2_082_844_800).as_deref(),
            Some("2024-01-01T00:00:00Z")
        );
        assert_eq!(movie_header_time(0), None);
        assert_eq!(movie_header_time(2_082_844_800), None);
        assert_eq!(movie_header_time(u64::MAX), None);
    }
}
