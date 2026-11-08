//! Gain-map HDR stills: a synthetic Ultra HDR / ISO 21496-1 JPEG built with the crate's own
//! encoder, iPhone HEICs from a local photo library when there is one, and damage.

use craft_image::{
    DecodeOptions, GainMapSource, OutputFormat, RenditionSpec, Rgba8Image, apply_gain_map, decode,
    encode_plain, gain_map, gain_map_summary, probe, render,
};
use std::path::{Path, PathBuf};

const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const ISO_URN: &[u8] = b"urn:iso:std:iso:ts:21496:-1\0";

fn seg(marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut s = vec![0xFF, marker];
    s.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
    s.extend_from_slice(payload);
    s
}

fn xmp_seg(description_attrs: &str) -> Vec<u8> {
    let xml = format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
         xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description \
         rdf:about=\"\" {description_attrs}/></rdf:RDF></x:xmpmeta>"
    );
    seg(0xE1, &[XMP_SIG, xml.as_bytes()].concat())
}

const GAINMAP_XMP: &str = "xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\" \
    hdrgm:Version=\"1.0\" hdrgm:GainMapMin=\"0\" hdrgm:GainMapMax=\"2\" hdrgm:Gamma=\"1\" \
    hdrgm:OffsetSDR=\"0.015625\" hdrgm:OffsetHDR=\"0.015625\" hdrgm:HDRCapacityMin=\"0\" \
    hdrgm:HDRCapacityMax=\"2\" hdrgm:BaseRenditionIsHDR=\"False\"";

const PRIMARY_XMP: &str = "xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\" \
    xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" hdrgm:Version=\"1.0\"";

fn rat(n: i32, d: u32) -> Vec<u8> {
    [n.to_be_bytes(), d.to_be_bytes()].concat()
}

/// ISO 21496-1 metadata: SDR base, 2 stops of headroom, gain 0..2 stops, offsets 1/64.
fn iso_metadata() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 0, 0];
    for r in [
        rat(0, 1),
        rat(2, 1),
        rat(0, 1),
        rat(2, 1),
        rat(1, 1),
        rat(1, 64),
        rat(1, 64),
    ] {
        b.extend(r);
    }
    b
}

/// 64×48: gray 128 on the left, 230 on the right.
fn base_image() -> Rgba8Image {
    let mut d = Vec::new();
    for _y in 0..48 {
        for x in 0..64 {
            let v = if x < 32 { 128 } else { 230 };
            d.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Rgba8Image::new(64, 48, d).unwrap()
}

/// 16×12: no gain on the left, full gain on the right.
fn map_image() -> Rgba8Image {
    let mut d = Vec::new();
    for _y in 0..12 {
        for x in 0..16 {
            let v = if x < 8 { 0 } else { 255 };
            d.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Rgba8Image::new(16, 12, d).unwrap()
}

fn jpeg(img: &Rgba8Image) -> Vec<u8> {
    encode_plain(img, OutputFormat::Jpeg { quality: 95 }).unwrap()
}

/// An MPF index segment for a primary of `primary_len` bytes followed by one image.
fn mpf_segment(primary_len: u32, second_len: u32, second_offset: u32) -> Vec<u8> {
    let mut t = b"MM\0*".to_vec();
    t.extend_from_slice(&8u32.to_be_bytes());
    t.extend_from_slice(&3u16.to_be_bytes());
    let entry = |tag: u16, ty: u16, count: u32, value: [u8; 4]| {
        let mut e = tag.to_be_bytes().to_vec();
        e.extend_from_slice(&ty.to_be_bytes());
        e.extend_from_slice(&count.to_be_bytes());
        e.extend_from_slice(&value);
        e
    };
    t.extend(entry(0xB000, 7, 4, *b"0100"));
    t.extend(entry(0xB001, 4, 1, 2u32.to_be_bytes()));
    t.extend(entry(0xB002, 7, 32, 50u32.to_be_bytes()));
    t.extend_from_slice(&0u32.to_be_bytes());
    for (attr, size, off) in [
        (0x2003_0000u32, primary_len, 0),
        (0, second_len, second_offset),
    ] {
        t.extend_from_slice(&attr.to_be_bytes());
        t.extend_from_slice(&size.to_be_bytes());
        t.extend_from_slice(&off.to_be_bytes());
        t.extend_from_slice(&[0; 4]);
    }
    seg(0xE2, &[b"MPF\0", &t[..]].concat())
}

/// A JPEG with a gain map as its MPF secondary image. `second_segments` go between the SOI and
/// the gain-map JPEG's own data. Returns the file and the length of its primary image.
fn with_gain_map(base: &[u8], map: &[u8], second_segments: &[Vec<u8>]) -> (Vec<u8>, usize) {
    let xmp = xmp_seg(PRIMARY_XMP);
    let mpf_len = mpf_segment(0, 0, 0).len();
    let primary_len = 2 + xmp.len() + mpf_len + base.len() - 2;
    let mpf_base = 2 + xmp.len() + 4 + 4; // marker and length, then "MPF\0"
    let mut second = vec![0xFF, 0xD8];
    for s in second_segments {
        second.extend_from_slice(s);
    }
    second.extend_from_slice(&map[2..]);
    let mpf = mpf_segment(
        u32::try_from(primary_len).unwrap(),
        u32::try_from(second.len()).unwrap(),
        u32::try_from(primary_len - mpf_base).unwrap(),
    );
    let mut file = vec![0xFF, 0xD8];
    file.extend_from_slice(&xmp);
    file.extend_from_slice(&mpf);
    file.extend_from_slice(&base[2..]);
    assert_eq!(file.len(), primary_len);
    file.extend_from_slice(&second);
    (file, primary_len)
}

fn ultra_hdr() -> (Vec<u8>, usize) {
    with_gain_map(
        &jpeg(&base_image()),
        &jpeg(&map_image()),
        &[xmp_seg(GAINMAP_XMP)],
    )
}

fn iso_jpeg() -> (Vec<u8>, usize) {
    let iso = seg(0xE2, &[ISO_URN, &iso_metadata()[..]].concat());
    with_gain_map(&jpeg(&base_image()), &jpeg(&map_image()), &[iso])
}

fn linear(code: u8) -> f32 {
    let e = f32::from(code) / 255.0;
    if e <= 0.040_45 {
        e / 12.92
    } else {
        ((e + 0.055) / 1.055).powf(2.4)
    }
}

#[test]
fn ultra_hdr_round_trip() {
    let (file, _) = ultra_hdr();
    let gm = gain_map(&file).unwrap().expect("gain map");
    assert_eq!(gm.source, GainMapSource::UltraHdr);
    assert_eq!(gm.params.max_log2_gain, [2.0; 3]);
    assert_eq!(gm.params.hdr_capacity_max, 2.0);
    assert!(!gm.params.base_is_hdr);
    assert_eq!(gm.params.offset_sdr, [0.015_625; 3]);
    let img = gm.image.as_ref().expect("map pixels");
    assert_eq!((img.width(), img.height()), (16, 12));
    assert!(gm.image_unavailable.is_none());

    // The catalogue view.
    let info = probe(&file).unwrap();
    let s = info.gain_map.expect("probe reports the gain map");
    assert_eq!(s.source, GainMapSource::UltraHdr);
    assert!((s.headroom - 4.0).abs() < 1e-5 && !s.base_is_hdr);
    assert_eq!(gain_map_summary(&file).unwrap(), Some(s));

    let base = decode(&file, &DecodeOptions::default()).unwrap().image;
    assert_eq!((base.width(), base.height()), (64, 48));
    let at = |lin: &craft_image::LinearRgbF32, x: usize, y: usize| lin.data[(y * 64 + x) * 3];
    let (left_lin, right_lin) = (linear(128), linear(230));

    // Headroom 1 is the SDR picture.
    let sdr = apply_gain_map(&base, None, &gm, 1.0).unwrap();
    assert_eq!((sdr.width, sdr.height), (64, 48));
    assert!((at(&sdr, 8, 24) - left_lin).abs() < 0.02);
    assert!((at(&sdr, 56, 24) - right_lin).abs() < 0.03);

    // At the maximum the highlights are four times brighter, the unmapped half is untouched.
    let hdr = apply_gain_map(&base, None, &gm, 4.0).unwrap();
    let want = (right_lin + 1.0 / 64.0) * 4.0 - 1.0 / 64.0;
    assert!(
        (at(&hdr, 56, 24) - want).abs() < 0.15,
        "{}",
        at(&hdr, 56, 24)
    );
    assert!(at(&hdr, 56, 24) > 3.0);
    assert!((at(&hdr, 8, 24) - left_lin).abs() < 0.03);

    // In between is in between, and more headroom than the map has changes nothing.
    let mid = at(&apply_gain_map(&base, None, &gm, 2.0).unwrap(), 56, 24);
    assert!(mid > right_lin * 1.5 && mid < at(&hdr, 56, 24));
    let over = apply_gain_map(&base, None, &gm, 100.0).unwrap();
    assert_eq!(over.data, hdr.data);
    assert!(hdr.data.iter().all(|v| v.is_finite() && *v >= 0.0));
}

#[test]
fn iso_21496_jpeg_is_read() {
    let (file, _) = iso_jpeg();
    let gm = gain_map(&file).unwrap().expect("gain map");
    assert_eq!(gm.source, GainMapSource::Iso21496);
    assert_eq!(gm.params.max_log2_gain, [2.0; 3]);
    assert_eq!(gm.params.offset_hdr, [1.0 / 64.0; 3]);
    let base = decode(&file, &DecodeOptions::default()).unwrap().image;
    let hdr = apply_gain_map(&base, None, &gm, 4.0).unwrap();
    assert!(hdr.data[(24 * 64 + 56) * 3] > 3.0);
}

#[test]
fn ordinary_files_have_no_gain_map() {
    let plain = jpeg(&base_image());
    assert!(gain_map(&plain).unwrap().is_none());
    assert!(probe(&plain).unwrap().gain_map.is_none());
    // An MPF secondary that is not a gain map (no hdrgm, no ISO) is not mistaken for one.
    let (file, _) = with_gain_map(&plain, &jpeg(&map_image()), &[]);
    assert!(gain_map(&file).unwrap().is_none());
    let png = encode_plain(&base_image(), OutputFormat::Png).unwrap();
    assert!(gain_map(&png).unwrap().is_none());
}

/// Renditions stay SDR: the gain map changes nothing about the pixels, the bytes, or the decode.
#[test]
fn renditions_ignore_the_gain_map() {
    let plain = jpeg(&base_image());
    let (file, primary_len) = ultra_hdr();
    for format in [
        OutputFormat::Jpeg { quality: 80 },
        OutputFormat::Png,
        OutputFormat::WebpLossless,
    ] {
        let spec = RenditionSpec::new(40, 40, format);
        let with = render(&file, &spec).unwrap();
        let without = render(&file[..primary_len], &spec).unwrap();
        let reference = render(&plain, &spec).unwrap();
        assert_eq!(with.bytes, without.bytes, "{format:?}");
        assert_eq!(with.bytes, reference.bytes, "{format:?}");
    }
    let a = decode(&file, &DecodeOptions::default()).unwrap();
    let b = decode(&plain, &DecodeOptions::default()).unwrap();
    assert_eq!(a.image, b.image);
}

fn heic_corpus() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 8 {
                walk(&p, out, depth + 1);
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("heic"))
            {
                out.push(p);
            }
        }
    }
    let root = std::env::var_os("MEDIACRAFT_HEIC_CORPUS")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                PathBuf::from(h).join("Pictures/Photos Library.photoslibrary/scopes/syndication")
            })
        });
    let mut out = Vec::new();
    if let Some(r) = root {
        walk(&r, &mut out, 0);
    }
    out.sort();
    out
}

/// Detection on real iPhone HEICs, checked against an independent oracle: the Apple auxiliary
/// image URN appears in the file bytes exactly when there is a gain map.
#[test]
fn iphone_heics_are_detected() {
    let files = heic_corpus();
    if files.is_empty() {
        eprintln!("no HEIC corpus here (set MEDIACRAFT_HEIC_CORPUS); skipping");
        return;
    }
    let needle = b"urn:com:apple:photo:2020:aux:hdrgainmap";
    let (mut with, mut without, mut with_pixels, mut assumed) = (0, 0, 0, 0);
    let mut reasons = std::collections::BTreeSet::new();
    let (mut lo, mut hi) = (f32::MAX, 0f32);
    for p in &files {
        let Ok(bytes) = std::fs::read(p) else {
            continue;
        };
        let expected = bytes.windows(needle.len()).any(|w| w == needle);
        let found = gain_map(&bytes);
        let summary = probe(&bytes).map(|i| i.gain_map);
        match (expected, found) {
            (true, Ok(Some(gm))) => {
                with += 1;
                assert_eq!(gm.source, GainMapSource::Apple, "{}", p.display());
                gm.params.validate().unwrap();
                let s = summary.unwrap().expect("probe flags it");
                assert!((s.headroom - gm.params.max_headroom()).abs() < 1e-4);
                lo = lo.min(s.headroom);
                hi = hi.max(s.headroom);
                assumed += usize::from(!gm.notes.is_empty());
                match (&gm.image, &gm.image_unavailable) {
                    (Some(_), None) => with_pixels += 1,
                    (None, Some(why)) => {
                        reasons.insert(why.clone());
                    }
                    other => panic!("{}: image and reason disagree: {other:?}", p.display()),
                }
            }
            (false, Ok(None)) => {
                without += 1;
                assert!(summary.unwrap().is_none());
            }
            (e, f) => panic!("{}: oracle {e}, got {f:?}", p.display()),
        }
    }
    eprintln!(
        "HEIC corpus: {} files, {with} with a gain map, {without} without; map pixels decoded for \
         {with_pixels}; headroom assumed for {assumed}; headroom {lo:.2}..{hi:.2}; reasons: {reasons:?}",
        files.len()
    );
    assert!(with + without > 0);
}

/// Renditions of gain-map HEICs are the ones the SDR path always made: the gain map is only
/// read on request.
#[test]
fn iphone_heic_decode_does_not_depend_on_the_gain_map() {
    let files = heic_corpus();
    let needle = b"urn:com:apple:photo:2020:aux:hdrgainmap";
    let Some(bytes) = files
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .find(|b| b.len() < 8 << 20 && b.windows(needle.len()).any(|w| w == needle))
    else {
        eprintln!("no gain-map HEIC here; skipping");
        return;
    };
    let spec = RenditionSpec::new(256, 256, OutputFormat::Jpeg { quality: 80 });
    let first = render(&bytes, &spec).unwrap();
    let _ = gain_map(&bytes);
    let second = render(&bytes, &spec).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

/// Every prefix and a sweep of flipped bytes: errors or nothing, never a panic.
fn survive(bytes: &[u8], prefix_step: usize, flip_step: usize) {
    for n in (0..bytes.len()).step_by(prefix_step) {
        let _ = gain_map(&bytes[..n]);
        let _ = gain_map_summary(&bytes[..n]);
        let _ = probe(&bytes[..n]);
    }
    for i in (0..bytes.len()).step_by(flip_step) {
        for mask in [0xFF, 0x80, 0x01] {
            let mut b = bytes.to_vec();
            b[i] ^= mask;
            if craft_image::sniff(&b) == Some(craft_image::Format::Jpeg)
                && let Ok(Some(gm)) = gain_map(&b)
                && let Ok(d) = decode(&b, &DecodeOptions::default())
                && let Ok(out) = apply_gain_map(&d.image, None, &gm, 4.0)
            {
                assert!(out.data.iter().all(|v| v.is_finite() && *v >= 0.0));
            }
            let _ = probe(&b);
        }
    }
}

#[test]
fn damaged_gain_map_jpegs_do_not_panic() {
    survive(&ultra_hdr().0, 1, 1);
    survive(&iso_jpeg().0, 1, 1);
}

#[test]
fn damaged_gain_map_heics_do_not_panic() {
    let needle = b"urn:com:apple:photo:2020:aux:hdrgainmap";
    let files = heic_corpus();
    let Some(bytes) = files
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .find(|b| b.len() < 4 << 20 && b.windows(needle.len()).any(|w| w == needle))
    else {
        return;
    };
    // The structure (boxes, item tables, XMP, EXIF) is in the first part of the file.
    let head = &bytes[..bytes.len().min(48 << 10)];
    survive(head, 1009, 389);
    survive(&bytes, 16_001, 24_007);
}
