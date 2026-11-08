//! The SVG sanitiser: XSS vectors, limits, benign passthrough and robustness. No cargo feature is
//! needed: the sanitiser, sniffing and headers are always built.

use craft_image::{Format, read_header, sanitize_svg, sniff};

const NS: &str = r#"xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#;

fn svg(body: &str) -> String {
    format!(r#"<svg {NS} width="8" height="8" viewBox="0 0 8 8">{body}</svg>"#)
}

fn clean(s: &str) -> String {
    let out = sanitize_svg(s.as_bytes()).expect("sanitises");
    let out = String::from_utf8(out).expect("utf-8");
    roxmltree::Document::parse(&out).expect("output is well-formed XML");
    out
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

#[test]
fn script_is_removed() {
    let out = clean(&svg(
        r#"<script>alert(1)</script><rect width="8" height="8"/>"#,
    ));
    assert!(!lower(&out).contains("script") && !out.contains("alert"));
    assert!(out.contains("<rect"));
    let out = clean(&svg(
        r#"<g><script xlink:href="https://evil.test/x.js"/></g>"#,
    ));
    assert!(!lower(&out).contains("script") && !out.contains("evil"));
}

#[test]
fn event_handlers_are_removed() {
    let src = format!(
        r#"<svg {NS} onload="alert(1)" onclick="x()"><rect onmouseover="y()" ONFOCUS="z()" width="1" height="1"/></svg>"#
    );
    let out = clean(&src);
    assert!(!lower(&out).contains("on") || !out.contains(" on"), "{out}");
    assert!(!lower(&out).contains("alert") && !out.contains("x()") && !out.contains("y()"));
    assert!(out.contains("<rect"));
}

#[test]
fn javascript_links_are_removed() {
    for href in [
        "javascript:alert(1)",
        "  JaVaScRiPt:alert(1)",
        "java\tscript:alert(1)",
        "&#x6A;avascript:alert(1)",
    ] {
        let out = clean(&svg(&format!(
            r#"<a xlink:href="{href}"><rect width="1" height="1"/></a><a href="{href}"/>"#
        )));
        assert!(!squash(&out).contains("javascript:"), "{href}: {out}");
        assert!(out.contains("<rect"));
    }
    // Anywhere else too: a paint, a style attribute, a title.
    let out = clean(&svg(
        r#"<rect fill="url(javascript:alert(1))" style="fill:url('javascript:x')" width="1" height="1"/><title>javascript:alert(1)</title>"#,
    ));
    assert!(!squash(&out).contains("javascript:"), "{out}");
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

#[test]
fn foreign_object_with_html_is_removed() {
    let out = clean(&svg(
        r#"<foreignObject width="8" height="8"><body xmlns="http://www.w3.org/1999/xhtml"><iframe src="https://evil.test"/><img src=x onerror="alert(1)"/></body></foreignObject><circle r="2"/>"#,
    ));
    let l = lower(&out);
    assert!(!l.contains("foreignobject") && !l.contains("iframe") && !l.contains("onerror"));
    assert!(!l.contains("evil") && !l.contains("xhtml"));
    assert!(out.contains("<circle"));
}

#[test]
fn style_with_external_targets_is_removed() {
    for css in [
        "@import url(https://evil.test/a.css);",
        "@import 'https://evil.test/a.css';",
        "rect{fill:url(https://evil.test/x)}",
        "rect{fill:URL( \"//evil.test/x\" )}",
        "rect{background:url(data:text/html,<script>)}",
        "rect{width:expression(alert(1))}",
        "rect{fill:u\\72l(https://evil.test/x)}",
        "@font-face{font-family:f;src:url(https://evil.test/f.woff)}",
        "rect{fill:/* c */url(//evil.test/x)}",
    ] {
        let out = clean(&svg(&format!(
            "<style>{css}</style><rect width='1' height='1'/>"
        )));
        assert!(
            !out.contains("evil")
                && !lower(&out).contains("import")
                && !lower(&out).contains("expression"),
            "{css}: {out}"
        );
        assert!(!out.contains("<style"), "{css}");
        assert!(out.contains("<rect"));
    }
    // Same through the attribute, and through presentation attributes.
    let out = clean(&svg(
        r#"<rect style="fill:url(https://evil.test/x)" width="1" height="1"/><rect filter="url(https://evil.test/f.svg#a)" mask="url('//evil.test/m')" width="1" height="1"/>"#,
    ));
    assert!(!out.contains("evil"), "{out}");
}

#[test]
fn local_references_and_plain_css_survive() {
    let src = svg(
        r##"<defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/></linearGradient><clipPath id="c"><rect width="4" height="4"/></clipPath><style>.a{fill:url(#g)} rect > .b{stroke:#00f}</style></defs><rect class="a" clip-path="url(#c)" width="8" height="8" style="fill:url( '#g' )"/>"##,
    );
    let out = clean(&src);
    assert!(
        out.contains("url(#g)") && out.contains("clip-path=\"url(#c)\"") && out.contains("<style>")
    );
    assert!(out.contains("rect &gt; .b"));
}

#[test]
fn external_hrefs_are_removed_but_fragments_and_data_images_stay() {
    let png = "data:image/png;base64,iVBORw0KGgo=";
    let src = svg(&format!(
        r##"<image xlink:href="https://evil.test/a.png" width="1" height="1"/>
        <image href="file:///etc/passwd" width="1" height="1"/>
        <image href="//evil.test/a.png" width="1" height="1"/>
        <image href="a.png" width="1" height="1"/>
        <image href="data:image/svg+xml;base64,PHN2Zy8+" width="1" height="1"/>
        <image href="data:text/html,hi" width="1" height="1"/>
        <image id="ok1" href="{png}" width="1" height="1"/>
        <image id="ok2" xlink:href="DATA:image/JPEG;base64,/9j/" width="1" height="1"/>
        <g id="t"/><use href="#t"/><use xlink:href="https://evil.test/x.svg#t"/>"##
    ));
    let out = clean(&src);
    assert!(
        !out.contains("evil")
            && !out.contains("passwd")
            && !out.contains("a.png")
            && !out.contains("svg+xml")
            && !out.contains("text/html"),
        "{out}"
    );
    assert!(out.contains(png) && out.contains("DATA:image/JPEG"));
    assert!(out.contains(r##"href="#t""##));
}

#[test]
fn animation_foreign_namespaces_and_comments_are_removed() {
    let src = format!(
        r##"<?xml version="1.0"?><?xml-stylesheet href="https://evil.test/s.css"?><!-- hi --><svg {NS} xmlns:i="http://i.test/" i:attr="x" data-x="1"><i:meta>t</i:meta><metadata><x/></metadata><a id="a"><animate attributeName="href" to="javascript:alert(1)"/><set attributeName="onload" to="x"/></a><rect width="1" height="1"/></svg>"##
    );
    let out = clean(&src);
    let l = lower(&out);
    for bad in [
        "animate", "<set", "metadata", "i:", "evil", "hi", "data-x", "alert",
    ] {
        assert!(!l.contains(bad), "{bad}: {out}");
    }
}

#[test]
fn doctype_and_entities() {
    // Internal entities are expanded and the declaration dropped.
    let out = clean(&format!(
        r##"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [ <!ENTITY red "#ff0000"> ]><svg {NS}><rect fill="&red;" width="1" height="1"/></svg>"##
    ));
    assert!(
        out.contains("#ff0000")
            && !out.contains("DOCTYPE")
            && !out.contains("ENTITY")
            && !out.contains("w3.org/Graphics")
    );

    // External entities are never read: error, or at least nothing from the file.
    for decl in [
        "SYSTEM \"file:///etc/passwd\"",
        "PUBLIC \"x\" \"http://evil.test/x\"",
    ] {
        let src = format!(
            r#"<?xml version="1.0"?><!DOCTYPE svg [ <!ENTITY xxe {decl}> ]><svg {NS}><text>&xxe;</text></svg>"#
        );
        match sanitize_svg(src.as_bytes()) {
            Err(_) => {}
            Ok(o) => {
                let o = String::from_utf8(o).expect("utf-8");
                assert!(!o.contains("root:") && !o.contains("evil") && !o.contains("ENTITY"));
            }
        }
    }
    // A parameter-entity DTD fetch is just text to us.
    let src = format!(
        r#"<!DOCTYPE svg [ <!ENTITY % d SYSTEM "http://evil.test/x.dtd"> %d; ]><svg {NS}/>"#
    );
    if let Ok(o) = sanitize_svg(src.as_bytes()) {
        assert!(!String::from_utf8(o).expect("utf-8").contains("evil"));
    }
}

#[test]
fn entity_expansion_bomb_is_refused_or_bounded() {
    let mut dtd = String::from(r#"<!ENTITY l0 "lollollollollollollollollollol">"#);
    for i in 1..10 {
        let prev = format!("&l{};", i - 1);
        dtd.push_str(&format!(r#"<!ENTITY l{i} "{}">"#, prev.repeat(10)));
    }
    let src =
        format!(r#"<?xml version="1.0"?><!DOCTYPE svg [{dtd}]><svg {NS}><text>&l9;</text></svg>"#);
    match sanitize_svg(src.as_bytes()) {
        Err(_) => {}
        Ok(o) => assert!(o.len() < 1 << 20, "{} bytes", o.len()),
    }
    // Quadratic blow-up through one big entity used many times.
    let big = "A".repeat(50_000);
    let uses = "&big;".repeat(20_000);
    let src =
        format!(r#"<!DOCTYPE svg [<!ENTITY big "{big}">]><svg {NS}><text>{uses}</text></svg>"#);
    match sanitize_svg(src.as_bytes()) {
        Err(_) => {}
        Ok(o) => assert!(o.len() < 64 << 20, "{} bytes", o.len()),
    }
}

#[test]
fn use_bomb_and_cycles_are_refused() {
    // g9 draws ten g8, ... : 10^9 elements from a few hundred bytes.
    let mut body = String::from(r#"<g id="g0"><rect width="1" height="1"/></g>"#);
    for i in 1..10 {
        body.push_str(&format!(
            r##"<g id="g{i}">{}</g>"##,
            format!(r##"<use href="#g{}"/>"##, i - 1).repeat(10)
        ));
    }
    body.push_str(r##"<use href="#g9"/>"##);
    let t = std::time::Instant::now();
    assert!(sanitize_svg(svg(&body).as_bytes()).is_err());
    assert!(t.elapsed().as_secs() < 5);

    // A reference loop.
    let body = r##"<g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g><use href="#a"/>"##;
    assert!(sanitize_svg(svg(body).as_bytes()).is_err());
    let body = r##"<g id="a"><use href="#a"/></g>"##;
    assert!(sanitize_svg(svg(body).as_bytes()).is_err());

    // A sane amount of reuse is fine.
    let body = r##"<g id="a"><rect width="1" height="1"/></g><use href="#a" x="1"/><use href="#a" x="2"/>"##;
    assert!(sanitize_svg(svg(body).as_bytes()).is_ok());
}

#[test]
fn element_and_depth_caps() {
    let flat = svg(&"<rect/>".repeat(100_001));
    assert!(sanitize_svg(flat.as_bytes()).is_err());
    let deep = svg(&format!("{}{}", "<g>".repeat(5000), "</g>".repeat(5000)));
    assert!(sanitize_svg(deep.as_bytes()).is_err());
    let ok = svg(&format!("{}{}", "<g>".repeat(100), "</g>".repeat(100)));
    assert!(sanitize_svg(ok.as_bytes()).is_ok());
}

#[test]
fn gzip_input_and_gzip_bombs() {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    let src = svg(r#"<rect width="1" height="1"/>"#);
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(src.as_bytes()).expect("write");
    let gz = e.finish().expect("finish");
    assert_eq!(sniff(&gz), Some(Format::Svg));
    assert_eq!(read_header(&gz).expect("header").width, 8);
    assert!(clean_bytes(&gz).contains("<rect"));

    let mut e = GzEncoder::new(Vec::new(), Compression::best());
    e.write_all(b"<svg>").expect("write");
    for _ in 0..80 {
        e.write_all(&vec![b' '; 1 << 20]).expect("write");
    }
    let bomb = e.finish().expect("finish");
    assert!(bomb.len() < 1 << 20);
    assert!(sanitize_svg(&bomb).is_err());
}

fn clean_bytes(b: &[u8]) -> String {
    String::from_utf8(sanitize_svg(b).expect("sanitises")).expect("utf-8")
}

#[test]
fn sniff_and_header() {
    let s = svg("");
    assert_eq!(sniff(s.as_bytes()), Some(Format::Svg));
    let with_prolog =
        format!("\u{feff}\n <?xml version=\"1.0\"?>\n<!-- c -->\n<!DOCTYPE svg>\n{s}");
    assert_eq!(sniff(with_prolog.as_bytes()), Some(Format::Svg));
    assert_eq!(sniff(b"<html><body><svg/></body></html>"), None);
    assert_eq!(sniff(b"<?xml version='1.0'?><rss/>"), None);
    let h = read_header(s.as_bytes()).expect("header");
    assert_eq!((h.format, h.width, h.height), (Format::Svg, 8, 8));
    let h = read_header(
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="50%" viewBox="0 0 30 20"/>"#,
    )
    .expect("header");
    assert_eq!((h.width, h.height), (30, 20));
    assert_eq!(Format::Svg.extension(), "svg");
    assert_eq!(Format::Svg.mime(), "image/svg+xml");
}

#[cfg(not(feature = "svg"))]
#[test]
fn decoding_without_the_feature_is_unsupported() {
    let e = craft_image::decode(svg("").as_bytes(), &craft_image::DecodeOptions::default());
    assert!(matches!(
        e,
        Err(craft_image::Error::Unsupported(Format::Svg, _))
    ));
    assert!(!Format::Svg.can_decode());
}

const SAMPLE: &str = r##"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "x">]><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" onload="x()" width="8" height="8" viewBox="0 0 8 8"><style>.c{fill:url(#g)}</style><defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/></linearGradient></defs><script>alert(1)</script><rect class="c" width="4" height="4"/><use xlink:href="https://evil.test/x#y"/><image href="https://evil.test/a.png"/><foreignObject><p onclick="z()"/></foreignObject></svg>"##;

#[test]
fn every_prefix_and_byte_flip_is_handled() {
    let bytes = SAMPLE.as_bytes();
    for n in 0..=bytes.len() {
        let out = sanitize_svg(&bytes[..n]);
        if let Ok(o) = out {
            assert!(roxmltree::Document::parse(std::str::from_utf8(&o).expect("utf-8")).is_ok());
        }
    }
    for i in 0..bytes.len() {
        for mask in [0x01u8, 0x20, 0x80, 0xFF] {
            let mut b = bytes.to_vec();
            b[i] ^= mask;
            if let Ok(o) = sanitize_svg(&b) {
                let t = std::str::from_utf8(&o).expect("utf-8");
                assert!(roxmltree::Document::parse(t).is_ok());
                assert!(
                    !squash(t).contains("javascript:")
                        && !t.contains("evil")
                        && !t.contains("<script")
                );
            }
            let _ = read_header(&b);
            let _ = sniff(&b);
        }
    }
}
