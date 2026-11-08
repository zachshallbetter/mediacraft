//! EXR, Radiance HDR, DDS, ICO/CUR, PNM, QOI, TGA and JPEG XL: sniffing, headers and pixels
//! round-tripped through fixtures generated here (encoded with the `image` crate, or built by
//! hand where it has no encoder), plus damaged-input robustness.

use craft_image::{
    DecodeOptions, Error, Format, Limits, Rgba8Image, decode, probe, read_header, sniff,
};
use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, Rgba};
use std::io::Cursor;

const W: u32 = 12;
const H: u32 = 9;

/// Distinct, fully-opaque-or-not RGBA test pattern.
fn pattern() -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    ImageBuffer::from_fn(W, H, |x, y| {
        Rgba([
            (x * 20) as u8,
            (y * 25) as u8,
            ((x + y) * 9) as u8,
            255 - (x * 7) as u8,
        ])
    })
}

fn encode_with(img: DynamicImage, fmt: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, fmt).unwrap();
    out.into_inner()
}

fn opts() -> DecodeOptions {
    DecodeOptions::default()
}

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Probe says `format` and `W`×`H`; decoding gives back `expected` (alpha compared only when
/// `alpha` is set).
fn round_trip(bytes: &[u8], format: Format, expected: &Rgba8Image, alpha: bool) {
    assert_eq!(sniff(bytes), Some(format));
    let info = probe(bytes).unwrap();
    assert_eq!(info.header.format, format);
    assert_eq!(
        (info.header.width, info.header.height),
        (expected.width(), expected.height())
    );
    let d = decode(bytes, &opts()).unwrap();
    assert_eq!(d.format, format);
    assert_eq!(
        (d.image.width(), d.image.height()),
        (expected.width(), expected.height())
    );
    for (a, b) in d.image.data().chunks(4).zip(expected.data().chunks(4)) {
        assert_eq!(&a[..3], &b[..3], "{format}");
        if alpha {
            assert_eq!(a[3], b[3], "{format} alpha");
        }
    }
}

fn expected() -> Rgba8Image {
    Rgba8Image::new(W, H, pattern().into_raw()).unwrap()
}

#[test]
fn qoi_round_trip() {
    let bytes = encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Qoi);
    let h = read_header(&bytes).unwrap();
    assert_eq!((h.bit_depth, h.has_alpha), (Some(8), Some(true)));
    round_trip(&bytes, Format::Qoi, &expected(), true);
    let rgb = DynamicImage::ImageRgba8(pattern()).into_rgb8();
    let bytes = encode_with(DynamicImage::ImageRgb8(rgb), ImageFormat::Qoi);
    assert_eq!(read_header(&bytes).unwrap().has_alpha, Some(false));
}

fn pnm_binary<P: image::Pixel<Subpixel = u8>>(
    img: &ImageBuffer<P, Vec<u8>>,
    subtype: PnmSubtype,
    color: ExtendedColorType,
) -> Vec<u8> {
    let mut out = Vec::new();
    PnmEncoder::new(&mut out)
        .with_subtype(subtype)
        .write_image(img.as_raw(), img.width(), img.height(), color)
        .unwrap();
    out
}

#[test]
fn pnm_round_trip() {
    // The default writer always emits PAM (P7); P6 and P5 need an explicit subtype.
    let pam = encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Pnm);
    assert!(pam.starts_with(b"P7"));
    assert_eq!(read_header(&pam).unwrap().has_alpha, Some(true));
    round_trip(&pam, Format::Pnm, &expected(), true);

    let rgb = DynamicImage::ImageRgba8(pattern()).into_rgb8();
    let ppm = pnm_binary(
        &rgb,
        PnmSubtype::Pixmap(SampleEncoding::Binary),
        ExtendedColorType::Rgb8,
    );
    assert!(ppm.starts_with(b"P6"));
    assert_eq!(read_header(&ppm).unwrap().has_alpha, Some(false));
    round_trip(&ppm, Format::Pnm, &expected(), false);

    let grey = DynamicImage::ImageRgba8(pattern()).into_luma8();
    let pgm = pnm_binary(
        &grey,
        PnmSubtype::Graymap(SampleEncoding::Binary),
        ExtendedColorType::L8,
    );
    assert!(pgm.starts_with(b"P5"));
    let d = decode(&pgm, &opts()).unwrap();
    assert_eq!(d.image.pixel(3, 2).unwrap()[0], grey.get_pixel(3, 2).0[0]);
}

#[test]
fn plain_text_pnm_with_comments() {
    let pbm = b"P1\n# a 3x2 bitmap\n3 2\n1 0 1\n0 1 0\n";
    assert_eq!(sniff(pbm), Some(Format::Pnm));
    let h = read_header(pbm).unwrap();
    assert_eq!((h.width, h.height, h.bit_depth), (3, 2, Some(1)));
    let d = decode(pbm, &opts()).unwrap();
    // In PBM a 1 is black.
    assert_eq!(d.image.pixel(0, 0), Some([0, 0, 0, 255]));
    assert_eq!(d.image.pixel(1, 0), Some([255, 255, 255, 255]));
}

#[test]
fn tga_round_trip_and_footer() {
    let bytes = encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Tga);
    round_trip(&bytes, Format::Tga, &expected(), false);
    // Detected from the header alone, and from the v2 footer when present.
    let mut with_footer = bytes.clone();
    if !with_footer.ends_with(b"TRUEVISION-XFILE.\0") {
        with_footer.extend_from_slice(&[0; 8]);
        with_footer.extend_from_slice(b"TRUEVISION-XFILE.\0");
    }
    assert_eq!(sniff(&with_footer), Some(Format::Tga));
    // A footer-less prefix that still holds the header is recognised by the strict check.
    assert_eq!(sniff(&bytes[..64.min(bytes.len())]), Some(Format::Tga));
    let rgb = DynamicImage::ImageRgba8(pattern()).into_rgb8();
    let bytes = encode_with(DynamicImage::ImageRgb8(rgb), ImageFormat::Tga);
    assert_eq!(read_header(&bytes).unwrap().has_alpha, Some(false));
    round_trip(&bytes, Format::Tga, &expected(), false);
}

#[test]
fn ico_round_trip() {
    let bytes = encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Ico);
    let h = read_header(&bytes).unwrap();
    assert_eq!((h.width, h.height), (W, H));
    round_trip(&bytes, Format::Ico, &expected(), true);
    // The same directory marked as a cursor is a cursor.
    let mut cur = bytes.clone();
    cur[2] = 2;
    assert_eq!(sniff(&cur), Some(Format::Ico));
    assert_eq!(read_header(&cur).unwrap().width, W);
}

#[test]
fn hdr_is_clamped_and_srgb_encoded() {
    // Linear values: black, half, one, and well over one.
    let px = [[0.0f32; 3], [0.5; 3], [1.0; 3], [4.0, 0.25, 0.0]];
    let mut pixels = Vec::new();
    for _ in 0..2 {
        for p in px.iter().cycle().take(8) {
            pixels.push(Rgb(*p));
        }
    }
    let mut bytes = Vec::new();
    image::codecs::hdr::HdrEncoder::new(&mut bytes)
        .encode(&pixels, 8, 2)
        .unwrap();
    assert_eq!(sniff(&bytes), Some(Format::Hdr));
    let info = probe(&bytes).unwrap();
    assert_eq!((info.header.width, info.header.height), (8, 2));
    let d = decode(&bytes, &opts()).unwrap();
    let at = |x| d.image.pixel(x, 0).unwrap();
    assert_eq!(at(0), [0, 0, 0, 255]);
    assert_eq!(at(1), [188, 188, 188, 255]);
    assert_eq!(at(2), [255, 255, 255, 255]);
    let p = at(3);
    assert_eq!(
        (p[0], p[2]),
        (255, 0),
        "over-range clamps to white, zero stays black"
    );
    assert!(
        (i32::from(p[1]) - 137).abs() <= 2,
        "linear 0.25 is 137 in sRGB: {p:?}"
    );
}

#[test]
fn exr_is_clamped_and_srgb_encoded() {
    let img: ImageBuffer<Rgba<f32>, Vec<f32>> = ImageBuffer::from_fn(5, 3, |x, _| match x {
        0 => Rgba([0.0, 0.0, 0.0, 1.0]),
        1 => Rgba([0.5, 0.5, 0.5, 0.5]),
        2 => Rgba([1.0, 1.0, 1.0, 1.0]),
        3 => Rgba([8.0, -1.0, 0.25, 1.0]),
        _ => Rgba([f32::NAN, 0.5, 0.5, 1.0]),
    });
    let bytes = encode_with(DynamicImage::ImageRgba32F(img), ImageFormat::OpenExr);
    assert_eq!(sniff(&bytes), Some(Format::Exr));
    let h = read_header(&bytes).unwrap();
    assert_eq!((h.width, h.height, h.has_alpha), (5, 3, Some(true)));
    assert!(matches!(h.bit_depth, Some(16 | 32)));
    let d = decode(&bytes, &opts()).unwrap();
    assert_eq!(d.image.pixel(0, 0), Some([0, 0, 0, 255]));
    assert_eq!(d.image.pixel(1, 1), Some([188, 188, 188, 128]));
    assert_eq!(d.image.pixel(2, 2), Some([255, 255, 255, 255]));
    let p = d.image.pixel(3, 0).unwrap();
    assert_eq!((p[0], p[1]), (255, 0));
    assert_eq!(d.image.pixel(4, 0).unwrap()[0], 0, "NaN becomes 0");
}

/// Probe the bytes, decode them through the public API, and return the pixels.
fn probe_and_decode(bytes: &[u8], format: Format, w: u32, h: u32) -> Rgba8Image {
    assert_eq!(sniff(bytes), Some(format));
    assert!(format.can_decode());
    let info = probe(bytes).unwrap();
    assert_eq!(
        (info.header.format, info.header.width, info.header.height),
        (format, w, h)
    );
    let d = decode(bytes, &opts()).unwrap();
    assert_eq!(
        (d.format, d.image.width(), d.image.height()),
        (format, w, h)
    );
    d.image
}

#[test]
fn dpx_rgb8() {
    let mut f = vec![0u8; 2048];
    f[..4].copy_from_slice(b"SDPX");
    f[4..8].copy_from_slice(&2048u32.to_be_bytes());
    f[770..772].copy_from_slice(&1u16.to_be_bytes());
    f[772..776].copy_from_slice(&2u32.to_be_bytes());
    f[776..780].copy_from_slice(&1u32.to_be_bytes());
    f[780 + 20] = 50; // RGB
    f[780 + 23] = 8;
    f[780 + 28..780 + 32].copy_from_slice(&2048u32.to_be_bytes());
    f[780 + 32..780 + 36].copy_from_slice(&u32::MAX.to_be_bytes());
    f.extend_from_slice(&[255, 0, 0, 0, 0, 255, 0, 0]); // two pixels, line padded to a word
    let img = probe_and_decode(&f, Format::Dpx, 2, 1);
    assert_eq!(img.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(img.pixel(1, 0), Some([0, 0, 255, 255]));
    assert_eq!(read_header(&f).unwrap().bit_depth, Some(8));
}

#[test]
fn pcx_truecolour() {
    let mut f = vec![0u8; 128];
    f[0] = 0x0A;
    f[1] = 5;
    f[2] = 1; // RLE
    f[3] = 8;
    f[8..10].copy_from_slice(&1u16.to_le_bytes()); // xmax: 2 pixels
    f[65] = 3; // planes
    f[66..68].copy_from_slice(&2u16.to_le_bytes()); // bytes per plane line
    // One scanline: the R plane, then G, then B. Every byte is below 0xC0, so all are literals.
    f.extend_from_slice(&[10, 100, 20, 110, 30, 120]);
    let img = probe_and_decode(&f, Format::Pcx, 2, 1);
    assert_eq!(img.pixel(0, 0), Some([10, 20, 30, 255]));
    assert_eq!(img.pixel(1, 0), Some([100, 110, 120, 255]));
}

#[test]
fn fits_is_flipped_and_stretched() {
    let card = |t: &str| {
        let mut c = t.as_bytes().to_vec();
        c.resize(80, b' ');
        c
    };
    let mut f = Vec::new();
    for t in [
        "SIMPLE  =                    T",
        "BITPIX  =                    8",
        "NAXIS   =                    2",
        "NAXIS1  =                    2",
        "NAXIS2  =                    2",
        "END",
    ] {
        f.extend(card(t));
    }
    f.resize(2880, b' ');
    f.extend_from_slice(&[0, 85, 170, 255]); // bottom row first
    f.resize(2880 * 2, 0);
    let img = probe_and_decode(&f, Format::Fits, 2, 2);
    assert_eq!(img.pixel(0, 0), Some([170, 170, 170, 255]));
    assert_eq!(img.pixel(1, 1), Some([85, 85, 85, 255]));
}

#[test]
fn dicom_uncompressed_grey() {
    // Explicit VR little endian. Short elements: tag, VR, u16 length, value.
    fn el(out: &mut Vec<u8>, g: u16, e: u16, vr: &[u8; 2], v: &[u8]) {
        out.extend_from_slice(&g.to_le_bytes());
        out.extend_from_slice(&e.to_le_bytes());
        out.extend_from_slice(vr);
        if matches!(vr, b"OB" | b"OW") {
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&(v.len() as u16).to_le_bytes());
        }
        out.extend_from_slice(v);
    }
    let mut f = vec![0u8; 128];
    f.extend_from_slice(b"DICM");
    el(&mut f, 2, 0x10, b"UI", b"1.2.840.10008.1.2.1\0");
    el(&mut f, 0x28, 0x02, b"US", &1u16.to_le_bytes());
    el(&mut f, 0x28, 0x04, b"CS", b"MONOCHROME2 ");
    el(&mut f, 0x28, 0x10, b"US", &2u16.to_le_bytes());
    el(&mut f, 0x28, 0x11, b"US", &2u16.to_le_bytes());
    el(&mut f, 0x28, 0x100, b"US", &8u16.to_le_bytes());
    el(&mut f, 0x28, 0x101, b"US", &8u16.to_le_bytes());
    el(&mut f, 0x28, 0x102, b"US", &7u16.to_le_bytes());
    el(&mut f, 0x28, 0x103, b"US", &0u16.to_le_bytes());
    el(&mut f, 0x7FE0, 0x10, b"OB", &[0, 85, 170, 255]);
    let img = probe_and_decode(&f, Format::Dicom, 2, 2);
    // DICOM rows run top to bottom.
    assert_eq!(img.pixel(0, 0), Some([0, 0, 0, 255]));
    assert_eq!(img.pixel(1, 1), Some([255, 255, 255, 255]));
}

/// A 4×4 DDS with one block of the given FourCC: colour endpoints red and blue, all indices 0.
fn dds(fourcc: &[u8; 4]) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..4].copy_from_slice(b"DDS ");
    b[4..8].copy_from_slice(&124u32.to_le_bytes());
    b[8..12].copy_from_slice(&0x1007u32.to_le_bytes());
    b[12..16].copy_from_slice(&4u32.to_le_bytes());
    b[16..20].copy_from_slice(&4u32.to_le_bytes());
    b[76..80].copy_from_slice(&32u32.to_le_bytes());
    b[80..84].copy_from_slice(&4u32.to_le_bytes());
    b[84..88].copy_from_slice(fourcc);
    if fourcc != b"DXT1" {
        b.extend_from_slice(&[200, 100, 0, 0, 0, 0, 0, 0]); // alpha endpoints 200 and 100
    }
    b.extend_from_slice(&0xF800u16.to_le_bytes());
    b.extend_from_slice(&0x001Fu16.to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b
}

#[test]
fn dds_block_compressed() {
    let b = dds(b"DXT1");
    assert_eq!(sniff(&b), Some(Format::Dds));
    let h = read_header(&b).unwrap();
    assert_eq!((h.width, h.height, h.bit_depth), (4, 4, Some(8)));
    let d = decode(&b, &opts()).unwrap();
    assert_eq!(d.image.pixel(2, 2), Some([255, 0, 0, 255]));

    let b = dds(b"DXT5");
    assert_eq!(read_header(&b).unwrap().has_alpha, Some(true));
    let d = decode(&b, &opts()).unwrap();
    assert_eq!(d.image.pixel(0, 0), Some([255, 0, 0, 200]));
}

#[test]
fn jxl_lossless_matches_the_png_it_came_from() {
    let bytes = data("pattern_alpha.jxl");
    assert!(bytes.len() < 20 * 1024);
    assert_eq!(sniff(&bytes), Some(Format::Jxl));
    assert!(Format::Jxl.can_decode());
    let h = read_header(&bytes).unwrap();
    assert_eq!(
        (h.format, h.width, h.height, h.has_alpha),
        (Format::Jxl, 320, 240, Some(true))
    );
    assert!(!h.animated);
    let d = decode(&bytes, &opts()).unwrap();
    let png = image::load_from_memory(&data("pattern_alpha.png"))
        .unwrap()
        .into_rgba8();
    assert_eq!((d.image.width(), d.image.height()), (320, 240));
    assert_eq!(d.image.data(), png.as_raw().as_slice());
}

#[test]
fn jxl_container_and_colour_profile() {
    // The same codestream inside the ISO container decodes the same.
    let bare = data("pattern_alpha.jxl");
    let mut wrapped = vec![
        0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    wrapped.extend_from_slice(&[
        0, 0, 0, 0x14, b'f', b't', b'y', b'p', b'j', b'x', b'l', b' ',
    ]);
    wrapped.extend_from_slice(&[0; 4]);
    wrapped.extend_from_slice(b"jxl ");
    wrapped.extend_from_slice(&((bare.len() + 8) as u32).to_be_bytes());
    wrapped.extend_from_slice(b"jxlc");
    wrapped.extend_from_slice(&bare);
    assert_eq!(sniff(&wrapped), Some(Format::Jxl));
    let a = decode(&bare, &opts()).unwrap();
    let b = decode(&wrapped, &opts()).unwrap();
    assert_eq!(a.image, b.image);

    // A linear-sRGB encoding with no embedded profile is described by a generated one.
    let lin = data("pattern_linear_small.jxl");
    let d = decode(&lin, &opts()).unwrap();
    assert_eq!((d.image.width(), d.image.height()), (64, 48));
    assert!(d.icc.is_some());
}

#[test]
fn limits_refuse_before_allocating() {
    let small = Limits {
        max_pixels: 10,
        ..Limits::default()
    };
    let tiny_alloc = Limits {
        max_alloc: 64,
        ..Limits::default()
    };
    let exr = encode_with(
        DynamicImage::ImageRgba32F(ImageBuffer::from_pixel(8, 8, Rgba([0.1, 0.2, 0.3, 1.0]))),
        ImageFormat::OpenExr,
    );
    let files = [
        encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Qoi),
        encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Pnm),
        encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Tga),
        encode_with(DynamicImage::ImageRgba8(pattern()), ImageFormat::Ico),
        dds(b"DXT1"),
        exr,
        data("pattern_alpha.jxl"),
    ];
    for f in &files {
        let o = DecodeOptions {
            limits: small,
            ..opts()
        };
        assert!(
            matches!(decode(f, &o), Err(Error::TooLarge { .. })),
            "{:?}",
            sniff(f)
        );
    }
    // Float formats hold 16 bytes a pixel while converting, so max_alloc is checked up front.
    let hdr = encode_hdr(8, 8);
    for f in [&files[5], &hdr, &files[6]] {
        let o = DecodeOptions {
            limits: tiny_alloc,
            ..opts()
        };
        assert!(
            matches!(decode(f, &o), Err(Error::TooLarge { .. })),
            "{:?}",
            sniff(f)
        );
    }
}

fn encode_hdr(w: usize, h: usize) -> Vec<u8> {
    let px = vec![Rgb([0.2f32, 0.4, 0.6]); w * h];
    let mut out = Vec::new();
    image::codecs::hdr::HdrEncoder::new(&mut out)
        .encode(&px, w, h)
        .unwrap();
    out
}

/// Every fixture, small enough to truncate at every length.
fn fixtures() -> Vec<Vec<u8>> {
    let exr = encode_with(
        DynamicImage::ImageRgba32F(ImageBuffer::from_fn(6, 4, |x, y| {
            Rgba([x as f32 / 5.0, y as f32 / 3.0, 2.0, 1.0])
        })),
        ImageFormat::OpenExr,
    );
    let small = DynamicImage::ImageRgba8(pattern());
    vec![
        encode_with(small.clone(), ImageFormat::Qoi),
        encode_with(small.clone(), ImageFormat::Pnm),
        encode_with(small.clone().into_rgb8().into(), ImageFormat::Pnm),
        b"P1\n# c\n3 2\n1 0 1\n0 1 0\n".to_vec(),
        encode_with(small.clone(), ImageFormat::Tga),
        encode_with(small, ImageFormat::Ico),
        encode_hdr(16, 4),
        exr,
        dds(b"DXT1"),
        dds(b"DXT5"),
        data("pattern_linear_small.jxl"),
    ]
}

/// Run every entry point; the only requirement is that none panics.
fn exercise(b: &[u8]) {
    let limits = Limits {
        max_pixels: 1 << 20,
        max_alloc: 64 << 20,
        max_input_bytes: 1 << 20,
    };
    let _ = sniff(b);
    let _ = read_header(b);
    let _ = probe(b);
    let _ = decode(b, &DecodeOptions { limits, ..opts() });
}

/// A small deterministic generator, so a failure reproduces.
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

#[test]
fn fixtures_are_valid() {
    for f in fixtures() {
        let info = probe(&f).unwrap();
        assert!(
            info.header.width > 0 && decode(&f, &opts()).is_ok(),
            "{:?}",
            info.header.format
        );
    }
}

#[test]
fn every_prefix_never_panics() {
    for f in fixtures() {
        for n in 0..f.len() {
            exercise(&f[..n]);
        }
    }
    // The 8 KB lossless JPEG XL is cut at a stride, to keep the run short.
    let big = data("pattern_alpha.jxl");
    for n in (0..big.len()).step_by(23) {
        exercise(&big[..n]);
    }
}

#[test]
fn flipped_bytes_never_panic() {
    let mut state = 0x5eed_u64;
    for f in fixtures().into_iter().chain([data("pattern_alpha.jxl")]) {
        for _ in 0..150 {
            let mut b = f.clone();
            for _ in 0..=(lcg(&mut state) % 4) {
                let at = lcg(&mut state) as usize % b.len();
                b[at] = lcg(&mut state) as u8;
            }
            exercise(&b);
        }
        // Header bytes matter most; hit each of the first 64 in turn.
        for at in 0..f.len().min(64) {
            for v in [0u8, 0xFF, 0x80] {
                let mut b = f.clone();
                b[at] = v;
                exercise(&b);
            }
        }
    }
}
