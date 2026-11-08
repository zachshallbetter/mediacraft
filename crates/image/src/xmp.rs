//! XMP (ISO 16684-1): the RDF/XML packet, read into the descriptive, rights and location
//! properties of Dublin Core, XMP basic, XMP Rights, Photoshop and IPTC Core. Both the
//! attribute and the element forms of simple properties are read, as are `rdf:Alt`, `rdf:Bag`
//! and `rdf:Seq` arrays. Namespaces are matched by URI, never by prefix.

use quick_xml::events::{BytesStart, Event};
use serde::{Deserialize, Serialize};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const RIGHTS: &str = "http://ns.adobe.com/xap/1.0/rights/";
const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
const IPTC_CORE: &str = "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/";

const MAX_PACKET: usize = 4 << 20;
const MAX_ELEMENTS: usize = 200_000;
const MAX_DEPTH: usize = 64;

/// XMP properties this crate reads. Text is as written; arrays keep their order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Xmp {
    /// `dc:title` (x-default).
    pub title: Option<String>,
    /// `dc:description` (x-default): the caption.
    pub description: Option<String>,
    /// `dc:creator`.
    pub creators: Vec<String>,
    /// `dc:rights` (x-default): the copyright notice.
    pub rights: Option<String>,
    /// `dc:subject`: keywords.
    pub keywords: Vec<String>,
    /// `xmp:Rating`, −1 (rejected) to 5.
    pub rating: Option<i8>,
    /// `xmp:CreateDate`.
    pub create_date: Option<String>,
    /// `xmp:CreatorTool`.
    pub creator_tool: Option<String>,
    /// `photoshop:DateCreated`: when the content was created.
    pub date_created: Option<String>,
    /// `photoshop:Headline`.
    pub headline: Option<String>,
    /// `photoshop:Credit`.
    pub credit: Option<String>,
    /// `photoshop:Source`.
    pub source: Option<String>,
    /// `photoshop:City`.
    pub city: Option<String>,
    /// `photoshop:State`.
    pub state: Option<String>,
    /// `photoshop:Country`.
    pub country: Option<String>,
    /// `Iptc4xmpCore:CountryCode`.
    pub country_code: Option<String>,
    /// `Iptc4xmpCore:Location`: place within the city.
    pub sublocation: Option<String>,
    /// `Iptc4xmpCore:AltTextAccessibility` (x-default).
    pub alt_text: Option<String>,
    /// `Iptc4xmpCore:ExtDescrAccessibility` (x-default).
    pub extended_description: Option<String>,
    /// `xmpRights:UsageTerms` (x-default).
    pub usage_terms: Option<String>,
    /// `xmpRights:WebStatement`.
    pub web_statement: Option<String>,
    /// `xmpRights:Marked`: `true` copyrighted, `false` public domain.
    pub copyright_marked: Option<bool>,
}

/// Why a packet could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum XmpError {
    /// Larger than 4 MiB.
    #[error("XMP packet is over 4 MiB")]
    TooLarge,
    /// Too many elements or nested too deeply.
    #[error("XMP packet is too complex")]
    TooComplex,
    /// Not well-formed XML.
    #[error("XMP is not well-formed XML: {0}")]
    Xml(String),
}

/// One property value: its `xml:lang` (for `rdf:Alt` items) and text.
pub(crate) type Item = (Option<String>, String);

/// A property being read: namespace, local name, and the items or text collected so far.
struct Open {
    ns: String,
    local: String,
    depth: usize,
    text: String,
    items: Vec<Item>,
    li: Option<(Option<String>, String, usize)>,
}

/// Read a packet.
pub fn parse_xmp(packet: &[u8]) -> Result<Xmp, XmpError> {
    if packet.len() > MAX_PACKET {
        return Err(XmpError::TooLarge);
    }
    let props = properties(packet)?;
    let first = |ns: &str, local: &str| -> Option<String> {
        let items = props
            .iter()
            .find(|(n, l, _)| n == ns && l == local)
            .map(|(_, _, v)| v)?;
        let pick = items
            .iter()
            .find(|(lang, _)| lang.as_deref() == Some("x-default"))
            .or_else(|| items.first());
        pick.map(|(_, t)| t.trim().to_string())
            .filter(|t| !t.is_empty())
    };
    let all = |ns: &str, local: &str| -> Vec<String> {
        props
            .iter()
            .filter(|(n, l, _)| n == ns && l == local)
            .flat_map(|(_, _, v)| v.iter().map(|(_, t)| t.trim().to_string()))
            .filter(|t| !t.is_empty())
            .collect()
    };
    Ok(Xmp {
        title: first(DC, "title"),
        description: first(DC, "description"),
        creators: all(DC, "creator"),
        rights: first(DC, "rights"),
        keywords: all(DC, "subject"),
        rating: first(XMP, "Rating")
            .and_then(|r| r.parse::<f64>().ok())
            .filter(|r| r.is_finite())
            .map(|r| r.round().clamp(-1.0, 5.0) as i8),
        create_date: first(XMP, "CreateDate"),
        creator_tool: first(XMP, "CreatorTool"),
        date_created: first(PHOTOSHOP, "DateCreated"),
        headline: first(PHOTOSHOP, "Headline"),
        credit: first(PHOTOSHOP, "Credit"),
        source: first(PHOTOSHOP, "Source"),
        city: first(PHOTOSHOP, "City"),
        state: first(PHOTOSHOP, "State"),
        country: first(PHOTOSHOP, "Country"),
        country_code: first(IPTC_CORE, "CountryCode"),
        sublocation: first(IPTC_CORE, "Location"),
        alt_text: first(IPTC_CORE, "AltTextAccessibility"),
        extended_description: first(IPTC_CORE, "ExtDescrAccessibility"),
        usage_terms: first(RIGHTS, "UsageTerms"),
        web_statement: first(RIGHTS, "WebStatement"),
        copyright_marked: first(RIGHTS, "Marked").and_then(|m| {
            match m.to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        }),
    })
}

/// Every top-level property of every `rdf:Description`: `(namespace, local name, values)`.
pub(crate) fn properties(packet: &[u8]) -> Result<Vec<(String, String, Vec<Item>)>, XmpError> {
    let text = String::from_utf8_lossy(packet);
    let mut reader = quick_xml::Reader::from_str(&text);
    reader.config_mut().check_end_names = false;
    // Namespace scopes: one frame per open element.
    let mut scopes: Vec<Vec<(String, String)>> = vec![vec![("xml".into(), XML.into())]];
    // Resolved names of open elements.
    let mut open: Vec<(String, String)> = Vec::new();
    let mut description_depth: Option<usize> = None;
    let mut current: Option<Open> = None;
    let mut out = Vec::new();
    let mut count = 0usize;

    loop {
        let ev = reader
            .read_event()
            .map_err(|e| XmpError::Xml(e.to_string()))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                count += 1;
                if count > MAX_ELEMENTS || open.len() >= MAX_DEPTH {
                    return Err(XmpError::TooComplex);
                }
                let empty = matches!(ev, Event::Empty(_));
                let (scope, attrs) = read_attributes(e);
                scopes.push(scope);
                let name = resolve(&String::from_utf8_lossy(e.name().as_ref()), &scopes, false);
                let depth = open.len();
                let parent_is_rdf = open.last().is_some_and(|(ns, l)| ns == RDF && l == "RDF");

                if name.0 == RDF && name.1 == "Description" && parent_is_rdf {
                    for (k, v) in &attrs {
                        let (ns, local) = resolve(k, &scopes, true);
                        if ns != RDF && ns != XML && !ns.is_empty() {
                            out.push((ns, local, vec![(None, v.clone())]));
                        }
                    }
                    if !empty {
                        description_depth = Some(depth);
                    }
                } else if description_depth.is_some_and(|d| depth == d + 1) && current.is_none() {
                    let resource = attrs
                        .iter()
                        .find(|(k, _)| resolve(k, &scopes, true) == (RDF.into(), "resource".into()))
                        .map(|(_, v)| v.clone());
                    let prop = Open {
                        ns: name.0.clone(),
                        local: name.1.clone(),
                        depth,
                        text: String::new(),
                        items: Vec::new(),
                        li: None,
                    };
                    if empty {
                        if let Some(r) = resource {
                            out.push((prop.ns, prop.local, vec![(None, r)]));
                        }
                    } else {
                        current = Some(prop);
                    }
                } else if let Some(p) = current.as_mut()
                    && name.0 == RDF
                    && name.1 == "li"
                    && depth == p.depth + 2
                {
                    let lang = attrs
                        .iter()
                        .find(|(k, _)| resolve(k, &scopes, true) == (XML.into(), "lang".into()))
                        .map(|(_, v)| v.clone());
                    if !empty {
                        p.li = Some((lang, String::new(), depth));
                    }
                }
                if empty {
                    scopes.pop();
                } else {
                    open.push(name);
                }
            }
            Event::End(_) => {
                let Some(_closed) = open.pop() else { continue };
                scopes.pop();
                let depth = open.len();
                if let Some(p) = current.as_mut() {
                    if p.li.as_ref().is_some_and(|(_, _, d)| *d == depth) {
                        if let Some((lang, t, _)) = p.li.take() {
                            p.items.push((lang, t));
                        }
                    } else if p.depth == depth
                        && let Some(p) = current.take()
                    {
                        let values = if p.items.is_empty() {
                            vec![(None, p.text)]
                        } else {
                            p.items
                        };
                        out.push((p.ns, p.local, values));
                    }
                }
                if description_depth == Some(depth) {
                    description_depth = None;
                }
            }
            Event::Text(t) => {
                let s = t.decode().map_err(|e| XmpError::Xml(e.to_string()))?;
                push_text(&mut current, open.len(), &s);
            }
            Event::CData(t) => push_text(&mut current, open.len(), &String::from_utf8_lossy(&t)),
            Event::GeneralRef(r) => {
                let name = String::from_utf8_lossy(&r).into_owned();
                let s = entity(&name).map_or_else(|| format!("&{name};"), String::from);
                push_text(&mut current, open.len(), &s);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Text goes to the open `rdf:li`, or to a simple property's own element.
fn push_text(current: &mut Option<Open>, depth: usize, s: &str) {
    let Some(p) = current.as_mut() else { return };
    if let Some((_, t, d)) = p.li.as_mut() {
        if *d + 1 == depth {
            t.push_str(s);
        }
    } else if p.depth + 1 == depth {
        p.text.push_str(s);
    }
}

/// `xmlns` declarations become a scope; other attributes are returned unescaped.
fn read_attributes(e: &BytesStart<'_>) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let mut scope = Vec::new();
    let mut attrs = Vec::new();
    for a in e.attributes().with_checks(false).flatten() {
        let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let v = unescape(&String::from_utf8_lossy(&a.value));
        if k == "xmlns" {
            scope.push((String::new(), v));
        } else if let Some(p) = k.strip_prefix("xmlns:") {
            scope.push((p.to_string(), v));
        } else {
            attrs.push((k, v));
        }
    }
    (scope, attrs)
}

/// `prefix:local` → (namespace URI, local). Unprefixed attributes have no namespace.
fn resolve(qname: &str, scopes: &[Vec<(String, String)>], attribute: bool) -> (String, String) {
    let (prefix, local) = qname.split_once(':').unwrap_or(("", qname));
    if prefix.is_empty() && attribute {
        return (String::new(), local.to_string());
    }
    let ns = scopes
        .iter()
        .rev()
        .flat_map(|s| s.iter().rev())
        .find(|(p, _)| p == prefix)
        .map(|(_, u)| u.clone())
        .unwrap_or_default();
    (ns, local.to_string())
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let code =
                if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok()?
                } else {
                    name.strip_prefix('#')?.parse().ok()?
                };
            char::from_u32(code)
        }
    }
}

/// Replace predefined and numeric character references; unknown references stay as written.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match rest
            .find(';')
            .and_then(|j| entity(&rest[1..j]).map(|c| (c, j)))
        {
            Some((c, j)) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const PACKET: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:ps="http://ns.adobe.com/photoshop/1.0/"
    xmlns:Iptc4xmpCore="http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/"
    xmlns:xmpRights="http://ns.adobe.com/xap/1.0/rights/"
    xmp:Rating="4"
    ps:City="Paris &amp; suburbs"
    xmpRights:Marked="True">
   <dc:title><rdf:Alt><rdf:li xml:lang="fr">Tour</rdf:li><rdf:li xml:lang="x-default">Eiffel Tower</rdf:li></rdf:Alt></dc:title>
   <dc:description><rdf:Alt><rdf:li xml:lang="x-default">Dusk over the Champ de Mars &lt;3</rdf:li></rdf:Alt></dc:description>
   <dc:creator><rdf:Seq><rdf:li>Ada</rdf:li><rdf:li>Grace</rdf:li></rdf:Seq></dc:creator>
   <dc:subject><rdf:Bag><rdf:li>paris</rdf:li><rdf:li>tower</rdf:li><rdf:li>dusk</rdf:li></rdf:Bag></dc:subject>
   <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">© 2024 Ada</rdf:li></rdf:Alt></dc:rights>
   <ps:DateCreated>2024-05-01T14:30:15+02:00</ps:DateCreated>
   <Iptc4xmpCore:AltTextAccessibility><rdf:Alt><rdf:li xml:lang="x-default">The Eiffel Tower lit up at dusk</rdf:li></rdf:Alt></Iptc4xmpCore:AltTextAccessibility>
   <Iptc4xmpCore:CreatorContactInfo rdf:parseType="Resource"><Iptc4xmpCore:CiEmailWork>a@example.com</Iptc4xmpCore:CiEmailWork></Iptc4xmpCore:CreatorContactInfo>
   <xmpRights:WebStatement rdf:resource="https://example.com/license"/>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

    #[test]
    fn reads_attribute_element_and_array_forms() {
        let x = parse_xmp(PACKET.as_bytes()).unwrap();
        assert_eq!(x.title.as_deref(), Some("Eiffel Tower"));
        assert_eq!(
            x.description.as_deref(),
            Some("Dusk over the Champ de Mars <3")
        );
        assert_eq!(x.creators, vec!["Ada", "Grace"]);
        assert_eq!(x.keywords, vec!["paris", "tower", "dusk"]);
        assert_eq!(x.rights.as_deref(), Some("© 2024 Ada"));
        assert_eq!(x.rating, Some(4));
        assert_eq!(x.city.as_deref(), Some("Paris & suburbs"));
        assert_eq!(x.date_created.as_deref(), Some("2024-05-01T14:30:15+02:00"));
        assert_eq!(
            x.alt_text.as_deref(),
            Some("The Eiffel Tower lit up at dusk")
        );
        assert_eq!(x.copyright_marked, Some(true));
        assert_eq!(
            x.web_statement.as_deref(),
            Some("https://example.com/license")
        );
        // Struct fields are not mistaken for top-level properties.
        assert_eq!(x.sublocation, None);
    }

    #[test]
    fn prefixes_do_not_matter_only_uris() {
        let p = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:q="http://purl.org/dc/elements/1.1/" xmlns:dc="urn:not-dublin-core"><dc:title>wrong</dc:title><q:title>right</q:title></rdf:Description></rdf:RDF>"#;
        assert_eq!(
            parse_xmp(p.as_bytes()).unwrap().title.as_deref(),
            Some("right")
        );
    }

    #[test]
    fn hostile_packets_are_refused_not_panicked_on() {
        assert!(parse_xmp(&vec![b'<'; MAX_PACKET + 1]).is_err());
        let deep = "<a>".repeat(MAX_DEPTH + 5);
        assert_eq!(parse_xmp(deep.as_bytes()), Err(XmpError::TooComplex));
        // Malformed XML may parse leniently or fail; either way it must return.
        let _ = parse_xmp(b"<rdf:RDF><unclosed");
        let _ = parse_xmp(b"<rdf:RDF xmlns:rdf=\"&#xFFFFFFFF;\"><rdf:Description a:b=\"&#0;\"/>");
        assert_eq!(parse_xmp(b"").unwrap(), Xmp::default());
    }
}
