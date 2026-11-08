//! Hostile input: arbitrary bytes and damaged real files must give errors, never panics, and
//! must stay within the limits.

use craft_image::{
    DecodeOptions, EncodeOptions, Limits, OutputFormat, RenditionSpec, Rgba8Image,
    ThumbnailOptions, decode, embedded, encode, probe, read_header, render, sniff, thumbnail,
};
use proptest::prelude::*;

/// A small valid file of each writable format, with ICC and EXIF where the format carries them.
fn seeds() -> Vec<Vec<u8>> {
    let mut d = Vec::new();
    for y in 0..24u32 {
        for x in 0..32u32 {
            d.extend_from_slice(&[
                (x * 8) as u8,
                (y * 10) as u8,
                90,
                if x > 28 { 0 } else { 255 },
            ]);
        }
    }
    let img = Rgba8Image::new(32, 24, d).unwrap();
    let opts = EncodeOptions {
        icc: Some(craft_cms_srgb()),
        exif: Some(b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec()),
        ..Default::default()
    };
    [
        OutputFormat::Jpeg { quality: 70 },
        OutputFormat::Png,
        OutputFormat::WebpLossless,
    ]
    .into_iter()
    .map(|f| encode(&img, f, &opts).unwrap())
    .collect()
}

fn craft_cms_srgb() -> Vec<u8> {
    // Any profile the decoder accepts will do; use the one renditions embed.
    let png = render(
        &encode(
            &Rgba8Image::filled(2, 2, [0, 0, 0, 255]).unwrap(),
            OutputFormat::Png,
            &EncodeOptions::default(),
        )
        .unwrap(),
        &RenditionSpec {
            embed_srgb_profile: true,
            ..RenditionSpec::new(2, 2, OutputFormat::Png)
        },
    )
    .unwrap();
    embedded(&png.bytes).icc.unwrap().into_owned()
}

/// Every entry point, with small limits so a forged size cannot make a test slow.
fn exercise(bytes: &[u8]) {
    let limits = Limits {
        max_pixels: 1 << 20,
        max_alloc: 64 << 20,
        max_input_bytes: 1 << 20,
    };
    let _ = sniff(bytes);
    let _ = read_header(bytes);
    let _ = embedded(bytes);
    let _ = probe(bytes);
    let _ = decode(
        bytes,
        &DecodeOptions {
            limits,
            fit: Some((8, 8)),
        },
    );
    let _ = thumbnail(
        bytes,
        &ThumbnailOptions {
            limits,
            ..ThumbnailOptions::new(16)
        },
    );
    let _ = render(
        bytes,
        &RenditionSpec {
            limits,
            ..RenditionSpec::new(16, 16, OutputFormat::Jpeg { quality: 50 })
        },
    );
}

#[test]
fn seeds_are_valid() {
    for s in seeds() {
        let info = probe(&s).unwrap();
        assert_eq!((info.header.width, info.header.height), (32, 24));
        assert!(thumbnail(&s, &ThumbnailOptions::new(16)).is_ok());
    }
}

#[test]
fn every_prefix_of_every_seed() {
    for s in seeds() {
        for n in 0..s.len() {
            exercise(&s[..n]);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        exercise(&bytes);
    }

    #[test]
    fn arbitrary_bytes_behind_each_magic(magic in 0usize..10, tail in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let heads: [&[u8]; 10] = [
            b"\xFF\xD8\xFF", b"\x89PNG\r\n\x1a\n", b"GIF89a", b"RIFF\0\0\0\0WEBP", b"II*\0", b"MM\0*",
            b"8BPS\0\x01", b"BM", b"\0\0\0\x18ftypheic", b"\0\0\0\x18ftypavif",
        ];
        let mut b = heads[magic].to_vec();
        b.extend_from_slice(&tail);
        exercise(&b);
    }

    #[test]
    fn damaged_real_files(seed in 0usize..3, flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..12)) {
        let mut b = seeds().swap_remove(seed);
        for (at, v) in flips {
            let i = at % b.len();
            b[i] = v;
        }
        exercise(&b);
    }
}

#[test]
fn forged_huge_dimensions_are_refused_before_allocation() {
    // A PNG IHDR claiming 60000×60000 must hit the pixel limit, not try to allocate 14 GB.
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = 60_000u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&[0; 4]);
    assert!(matches!(
        decode(&png, &DecodeOptions::default()),
        Err(craft_image::Error::TooLarge { .. })
    ));
}
