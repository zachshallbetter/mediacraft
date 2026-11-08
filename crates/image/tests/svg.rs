//! SVG rendering through `usvg` and `resvg`: pixels, fitting, limits, what never loads, and
//! robustness of the decode path. Sanitiser rules are in `svg_sanitize.rs`.

use craft_image::{
    DecodeOptions, Error, Format, Limits, OutputFormat, Rgba8Image, ThumbnailOptions, decode,
    encode_plain, sanitize_svg, thumbnail,
};

const NS: &str = r#"xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#;

fn render(src: &str) -> Rgba8Image {
    decode(src.as_bytes(), &DecodeOptions::default())
        .expect("decodes")
        .image
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                s.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[test]
fn renders_pixels() {
    let img = render(&format!(
        r##"<svg {NS} width="8" height="4" viewBox="0 0 8 4"><rect width="4" height="4" fill="#ff0000"/><rect x="4" width="4" height="4" fill="#0000ff" fill-opacity="0.5"/></svg>"##
    ));
    assert_eq!((img.width(), img.height()), (8, 4));
    assert_eq!(img.pixel(1, 1), Some([255, 0, 0, 255]));
    let p = img.pixel(6, 2).expect("pixel");
    assert!(
        p[0] == 0 && p[1] == 0 && p[2] >= 254 && p[3].abs_diff(128) <= 1,
        "{p:?}"
    );
    // Straight alpha, not premultiplied: transparent where nothing is drawn.
    let img = render(&format!(r#"<svg {NS} width="4" height="4"/>"#));
    assert_eq!(img.pixel(0, 0), Some([0, 0, 0, 0]));
}

#[test]
fn gradients_clips_use_and_styles_render() {
    let img = render(&format!(
        r##"<svg {NS} width="8" height="8"><style>.c{{fill:url(#g)}}</style><defs><linearGradient id="g" x1="0" x2="1"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient><clipPath id="k"><rect width="8" height="4"/></clipPath><rect id="r" width="8" height="8" class="c"/></defs><use href="#r" clip-path="url(#k)"/></svg>"##
    ));
    let l = img.pixel(0, 1).expect("pixel")[0];
    let r = img.pixel(7, 1).expect("pixel")[0];
    assert!(l < 40 && r > 215, "{l} {r}");
    assert_eq!(img.pixel(4, 6).expect("pixel")[3], 0, "clipped away");
}

#[test]
fn fits_to_the_requested_size() {
    let src = format!(
        r##"<svg {NS} width="40" height="20" viewBox="0 0 4 2"><rect width="4" height="2" fill="#0f0"/></svg>"##
    );
    let opts = DecodeOptions {
        fit: Some((16, 16)),
        ..DecodeOptions::default()
    };
    let d = decode(src.as_bytes(), &opts).expect("decodes");
    assert_eq!((d.image.width(), d.image.height()), (16, 8));
    assert_eq!((d.source_width, d.source_height), (40, 20));
    assert_eq!(d.image.pixel(15, 7), Some([0, 255, 0, 255]));
    // Up as well: vectors stay sharp.
    let opts = DecodeOptions {
        fit: Some((200, 200)),
        ..DecodeOptions::default()
    };
    let d = decode(src.as_bytes(), &opts).expect("decodes");
    assert_eq!((d.image.width(), d.image.height()), (200, 100));
    // And through the thumbnail path.
    let t = thumbnail(src.as_bytes(), &ThumbnailOptions::default());
    assert!(t.is_ok(), "{:?}", t.err());
}

#[test]
fn limits_are_enforced_before_drawing() {
    let big = format!(r#"<svg {NS} width="100000" height="100000"/>"#);
    let e = decode(big.as_bytes(), &DecodeOptions::default());
    assert!(matches!(e, Err(Error::TooLarge { .. })), "{e:?}");
    // The same file is fine when the caller asks for a small rendering.
    let opts = DecodeOptions {
        fit: Some((32, 32)),
        ..DecodeOptions::default()
    };
    let d = decode(big.as_bytes(), &opts).expect("decodes");
    assert_eq!((d.image.width(), d.image.height()), (32, 32));
    // A tight limit applies to the fitted size too.
    let opts = DecodeOptions {
        fit: Some((64, 64)),
        limits: Limits {
            max_pixels: 100,
            ..Limits::default()
        },
        ..DecodeOptions::default()
    };
    assert!(matches!(
        decode(big.as_bytes(), &opts),
        Err(Error::TooLarge { .. })
    ));
}

#[test]
fn data_images_render_and_external_images_never_load() {
    let mut red = Rgba8Image::filled(2, 2, [255, 0, 0, 255]).expect("image");
    red.data_mut()[3] = 255;
    let png = encode_plain(&red, OutputFormat::Png).expect("png");
    let uri = format!("data:image/png;base64,{}", base64(&png));
    let img = render(&format!(
        r#"<svg {NS} width="4" height="4"><image href="{uri}" width="4" height="4"/></svg>"#
    ));
    assert_eq!(img.pixel(2, 2), Some([255, 0, 0, 255]));

    // A real file on disk, by absolute and relative path, and a URL: none of them draw.
    let dir = std::env::temp_dir().join(format!("craft-svg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join("red.png");
    std::fs::write(&file, &png).expect("write");
    for href in [
        file.to_string_lossy().into_owned(),
        format!("file://{}", file.display()),
        "red.png".to_string(),
        "http://127.0.0.1:9/red.png".to_string(),
    ] {
        let src = format!(
            r#"<svg {NS} width="4" height="4"><image href="{href}" xlink:href="{href}" width="4" height="4"/></svg>"#
        );
        let img = render(&src);
        assert!(img.data().iter().all(|b| *b == 0), "{href} drew something");
    }
    let _ = std::fs::remove_dir_all(&dir);

    // Neither does an SVG smuggled in as an image data URI.
    let inner = base64(
        format!(
            r##"<svg {NS} width="4" height="4"><rect width="4" height="4" fill="#f00"/></svg>"##
        )
        .as_bytes(),
    );
    for mime in ["image/svg+xml", "image/png"] {
        let img = render(&format!(
            r#"<svg {NS} width="4" height="4"><image href="data:{mime};base64,{inner}" width="4" height="4"/></svg>"#
        ));
        assert!(img.data().iter().all(|b| *b == 0), "{mime}");
    }
}

#[test]
fn text_without_fonts_is_skipped_not_fatal() {
    let img = render(&format!(
        r##"<svg {NS} width="16" height="16"><rect width="16" height="16" fill="#00f"/><text x="1" y="12" font-size="12" font-family="Arial">Hi</text></svg>"##
    ));
    assert_eq!(img.pixel(8, 8), Some([0, 0, 255, 255]));
}

#[test]
fn svgz_decodes() {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    let src = format!(
        r##"<svg {NS} width="2" height="2"><rect width="2" height="2" fill="#f00"/></svg>"##
    );
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(src.as_bytes()).expect("write");
    let gz = e.finish().expect("finish");
    let d = decode(&gz, &DecodeOptions::default()).expect("decodes");
    assert_eq!(d.format, Format::Svg);
    assert_eq!(d.image.pixel(1, 1), Some([255, 0, 0, 255]));
}

#[test]
fn hostile_files_render_without_effects() {
    let src = format!(
        r##"<svg {NS} onload="alert(1)" width="4" height="4"><script>alert(1)</script><foreignObject width="4" height="4"><div>x</div></foreignObject><rect width="4" height="4" fill="#f00" onclick="x()"/></svg>"##
    );
    assert_eq!(render(&src).pixel(1, 1), Some([255, 0, 0, 255]));
}

/// A document with a bit of everything benign. Sanitising must not change a pixel.
const BENIGN: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- benign -->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="32" height="24" viewBox="0 0 32 24">
  <metadata>ignored</metadata>
  <style>.t{fill:#336699;stroke:#000;stroke-width:1} .g{fill:url(#grad)}</style>
  <defs>
    <linearGradient id="grad" x1="0" y1="0" x2="1" y2="1" gradientUnits="objectBoundingBox"><stop offset="0" stop-color="#ff8800"/><stop offset="1" stop-color="#0088ff" stop-opacity="0.8"/></linearGradient>
    <radialGradient id="rad"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/></radialGradient>
    <clipPath id="clip"><circle cx="24" cy="12" r="6"/></clipPath>
    <mask id="mask"><rect width="32" height="24" fill="url(#rad)"/></mask>
    <filter id="blur"><feGaussianBlur stdDeviation="1"/></filter>
    <marker id="m" markerWidth="4" markerHeight="4" refX="2" refY="2"><circle cx="2" cy="2" r="1" fill="red"/></marker>
    <pattern id="pat" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2" fill="#0a0"/></pattern>
    <g id="unit"><rect width="3" height="3" fill="#a0a"/></g>
  </defs>
  <rect class="g" x="1" y="1" width="14" height="10" rx="2"/>
  <rect class="t" x="1" y="13" width="10" height="9" filter="url(#blur)" inkscape:label="x"/>
  <circle cx="24" cy="12" r="8" fill="url(#pat)" clip-path="url(#clip)" mask="url(#mask)"/>
  <path d="M14 14 L20 22 L26 14" stroke="#222" stroke-width="2" fill="none" marker-end="url(#m)" transform="translate(1 0)"/>
  <use xlink:href="#unit" x="12" y="1"/><use href="#unit" x="12" y="5" opacity="0.5"/>
  <g style="opacity:0.7"><polygon points="28,2 31,2 31,5" fill="#00f" fill-rule="evenodd"/></g>
</svg>"##;

#[test]
fn benign_files_render_the_same_after_sanitising() {
    let before = render(BENIGN);
    let clean =
        String::from_utf8(sanitize_svg(BENIGN.as_bytes()).expect("sanitises")).expect("utf-8");
    let after = render(&clean);
    assert_eq!(before, after);
    assert!(
        before.data().chunks(4).filter(|p| p[3] > 0).count() > 100,
        "drew something"
    );
    // And it is stable: sanitising the output changes nothing.
    assert_eq!(
        sanitize_svg(clean.as_bytes()).expect("sanitises"),
        clean.as_bytes()
    );
}

#[test]
fn prefixes_and_byte_flips_do_not_panic() {
    let bytes = BENIGN.replace(
        "<metadata>ignored</metadata>",
        r#"<image href="data:image/png;base64,iVBORw0KGgo=" width="1" height="1"/>"#,
    );
    let bytes = bytes.replace("width=\"32\" height=\"24\"", "width=\"8\" height=\"6\"");
    let bytes = bytes.as_bytes();
    let opts = DecodeOptions {
        limits: Limits {
            max_pixels: 1 << 20,
            ..Limits::default()
        },
        ..DecodeOptions::default()
    };
    for n in 0..=bytes.len() {
        let _ = decode(&bytes[..n], &opts);
    }
    for i in 0..bytes.len() {
        for mask in [0x04u8, 0x80] {
            let mut b = bytes.to_vec();
            b[i] ^= mask;
            let _ = decode(&b, &opts);
        }
    }
}
