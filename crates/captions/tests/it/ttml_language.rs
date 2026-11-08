use craft_captions::{Format, parse};

/// An `xml:lang` that is empty or not a language tag leaves the language unknown.
#[test]
fn ttml_import_ignores_a_bad_xml_lang() {
    let tt = |lang: &str| format!(r#"<tt xmlns="http://www.w3.org/ns/ttml" xml:lang="{lang}"><body><div><p begin="1s" end="2s">Hi</p></div></body></tt>"#);
    let doc = parse(tt(" fr-CA ").as_bytes(), Format::Ttml).unwrap();
    assert_eq!((doc.language.as_deref(), doc.warnings.len()), (Some("fr-CA"), 0));
    let doc = parse(tt("").as_bytes(), Format::Ttml).unwrap();
    assert_eq!((doc.language, doc.warnings.len()), (None, 0));
    for bad in ["pt BR", "&lt;x&gt;", &"a".repeat(36)] {
        let doc = parse(tt(bad).as_bytes(), Format::Ttml).unwrap();
        assert_eq!(doc.language, None, "{bad}");
        assert_eq!(doc.warnings, ["ignored xml:lang: not a language tag"], "{bad}");
        assert_eq!(doc.cues.len(), 1);
    }
}
