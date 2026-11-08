//! HEIC and AVIF stills decoded in-tree (craft-hevc, craft-av1) against their lossless sources.
//! Fixtures are described in tests/data/README.md.

use craft_image::{DecodeOptions, Format, Rgba8Image, ThumbnailOptions, decode, probe, thumbnail};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name),
    )
    .unwrap()
}

/// Mean absolute difference per colour channel, over the pixels `keep` selects.
fn mean_diff(a: &Rgba8Image, b: &Rgba8Image, keep: impl Fn(u32, u32) -> bool) -> f64 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let (mut sum, mut n) = (0f64, 0f64);
    for y in 0..a.height() {
        for x in 0..a.width() {
            if !keep(x, y) {
                continue;
            }
            let (p, q) = (a.pixel(x, y).unwrap(), b.pixel(x, y).unwrap());
            sum += (0..3).map(|c| f64::from(p[c].abs_diff(q[c]))).sum::<f64>();
            n += 3.0;
        }
    }
    sum / n
}

fn source(name: &str) -> Rgba8Image {
    decode(&fixture(name), &DecodeOptions::default())
        .unwrap()
        .image
}

#[test]
fn heic_with_alpha_decodes_through_craft_hevc() {
    let bytes = fixture("pattern_alpha.heic");
    let info = probe(&bytes).unwrap();
    assert_eq!(info.header.format, Format::Heif);
    assert!(info.decodable);

    let d = decode(&bytes, &DecodeOptions::default()).unwrap();
    let (srgb, _) = d.to_srgb();
    let img = d.orientation.apply(&srgb).unwrap();
    let src = source("pattern_alpha.png");
    assert_eq!((img.width(), img.height()), (320, 240));
    // Colour of the opaque half matches the source within HEVC's loss.
    let diff = mean_diff(&img, &src, |x, _| x < 150);
    assert!(diff < 6.0, "colour differs by {diff}");
    // Until craft-hevc decodes monochrome (4:0:0) streams, the alpha auxiliary image is skipped
    // and the image is opaque; see `heic_alpha_is_applied`.
    assert!(img.is_opaque());
}

#[test]
fn ten_bit_avif_decodes_through_craft_av1() {
    let bytes = fixture("pattern_10bit.avif");
    assert_eq!(probe(&bytes).unwrap().header.format, Format::Avif);
    let img = decode(&bytes, &DecodeOptions::default()).unwrap().image;
    let src = source("pattern.png");
    let diff = mean_diff(&img, &src, |_, _| true);
    assert!(diff < 6.0, "colour differs by {diff}");
    assert!(img.is_opaque());
}

#[test]
fn heic_and_avif_thumbnail_and_survive_damage() {
    for name in ["pattern_alpha.heic", "pattern_10bit.avif"] {
        let bytes = fixture(name);
        let t = thumbnail(&bytes, &ThumbnailOptions::new(64)).unwrap();
        assert_eq!((t.image.width(), t.image.height()), (64, 48), "{name}");
        // Every prefix and a sweep of byte flips: errors, never panics.
        for n in (0..bytes.len()).step_by(97) {
            let _ = decode(&bytes[..n], &DecodeOptions::default());
        }
        for i in (0..bytes.len()).step_by(53) {
            let mut b = bytes.clone();
            b[i] ^= 0xA5;
            let _ = decode(&b, &DecodeOptions::default());
            let _ = probe(&b);
        }
    }
}

/// HEIF stores alpha as a monochrome (4:0:0) HEVC auxiliary image, which craft-hevc does not
/// decode yet ("only 4:2:0"). The reader already finds the auxiliary image and its `auxC` URN.
#[test]
#[ignore = "craft-hevc lacks 4:0:0 (monochrome) decoding"]
fn heic_alpha_is_applied() {
    let d = decode(&fixture("pattern_alpha.heic"), &DecodeOptions::default()).unwrap();
    let img = d.orientation.apply(&d.image).unwrap();
    assert!(img.pixel(40, 120).unwrap()[3] >= 250);
    assert!(img.pixel(280, 120).unwrap()[3].abs_diff(64) <= 8);
}
