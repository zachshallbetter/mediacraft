//! An allowlist sanitiser for untrusted SVG.
//!
//! [`sanitize_svg`] parses the document, keeps only elements and attributes that draw, and
//! writes it back out as plain, well-formed SVG. What it never lets through: scripts, foreign
//! content, event handlers, animation (which can rewrite an `href` after the check), external
//! references, `javascript:` anywhere, DOCTYPE and entity declarations (entities that the
//! document defines for itself are expanded and the declaration is dropped), comments and
//! processing instructions. It is always compiled and does not depend on the renderer, so a
//! service that only stores or serves SVG can use it with every cargo feature off.
//!
//! The rules, in order of application:
//!
//! * The root must be `svg` in the SVG namespace. Elements outside that namespace, or not on the
//!   element allowlist, are dropped with their whole subtree. `script`, `foreignObject`,
//!   `animate*`, `set` and `metadata` are not on the list.
//! * Only allowlisted attributes survive, and only in no namespace, `xlink:href` or `xml:space`.
//!   Every `on*` attribute is therefore gone.
//! * An `href` or `xlink:href` must be a local `#fragment` or a `data:image/png`, `jpeg`, `gif` or
//!   `webp` URI. Nothing else is kept, so there is no external image, no `data:image/svg+xml`
//!   and no `javascript:` link.
//! * Any other attribute value that contains `javascript:`, `vbscript:` or `expression(`, or a
//!   `url(...)` whose target is not a local `#fragment`, is dropped.
//! * A `<style>` element (and a `style` attribute) containing `@import`, `expression(`, a
//!   non-fragment `url(`, a CSS escape (`\`), `image-set(` or similar is dropped whole.
//! * Text containing `javascript:` is dropped.
//! * Limits: element count, nesting depth, and the number of elements a document expands to once
//!   every `<use>` is followed (so a "billion laughs" built from nested `<use>` is refused), plus
//!   input and decompressed size. A document over any of them is an error, not a truncation.
//!
//! Gzip input (`.svgz`) is inflated, within the size limit, before parsing.

use crate::{Error, Format, Result};
use roxmltree::{Document, Node, NodeId, ParsingOptions};
use std::collections::{HashMap, HashSet};
use std::io::Read;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// Ceilings [`sanitize_svg_with`] enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SanitizeLimits {
    /// Most elements in the source document (default 100 000).
    pub max_elements: usize,
    /// Deepest element nesting (default 128).
    pub max_depth: usize,
    /// Most elements the document draws once every `<use>` is expanded (default 1 000 000).
    pub max_use_expansion: u64,
    /// Largest input, and largest size once inflated, in bytes (default 64 MiB).
    pub max_bytes: usize,
}

impl Default for SanitizeLimits {
    fn default() -> Self {
        Self {
            max_elements: 100_000,
            max_depth: 128,
            max_use_expansion: 1_000_000,
            max_bytes: 64 << 20,
        }
    }
}

/// Sanitise an SVG (or gzip-compressed SVG) with the default [`SanitizeLimits`]. The result is
/// UTF-8 SVG that renders the same as a benign input and cannot reach outside itself.
pub fn sanitize_svg(bytes: &[u8]) -> Result<Vec<u8>> {
    sanitize_svg_with(bytes, &SanitizeLimits::default())
}

/// [`sanitize_svg`] with explicit limits.
pub fn sanitize_svg_with(bytes: &[u8], limits: &SanitizeLimits) -> Result<Vec<u8>> {
    let raw = inflate_if_gzip(bytes, limits.max_bytes)?;
    let text =
        std::str::from_utf8(strip_bom(&raw)).map_err(|_| bad("the document is not UTF-8 text"))?;
    check_entity_expansion(text, limits.max_bytes)?;
    let opts = ParsingOptions {
        // Internal entities are expanded by the parser (with its own loop and fan-out limits);
        // external ones have no resolver, so a reference to one is an error.
        allow_dtd: true,
        nodes_limit: u32::try_from(limits.max_elements.saturating_mul(4)).unwrap_or(u32::MAX),
        entity_resolver: None,
    };
    let doc = Document::parse_with_options(text, opts).map_err(|e| bad(&e.to_string()))?;
    let root = doc.root_element();
    if !is_svg(root) || root.tag_name().name() != "svg" {
        return Err(bad("the root element is not <svg> in the SVG namespace"));
    }
    let elements = doc.descendants().filter(Node::is_element).count();
    if elements > limits.max_elements {
        return Err(bad(&format!(
            "{elements} elements, over the {} limit",
            limits.max_elements
        )));
    }
    check_use_expansion(&doc, limits)?;

    let mut out = String::with_capacity(text.len().min(limits.max_bytes) / 2 + 128);
    write_element(root, 0, limits, true, &mut out)?;
    if out.len() > limits.max_bytes {
        return Err(bad("the sanitised document is over the size limit"));
    }
    Ok(out.into_bytes())
}

/// Refuse a document whose entity references could expand past `max` bytes. The parser limits
/// nesting and fan-out, but not how often one long entity is referenced from the text, so a few
/// kilobytes of source could otherwise become gigabytes.
fn check_entity_expansion(text: &str, max: usize) -> Result<()> {
    let mut longest = 0usize;
    let mut nested = false;
    let mut rest = text;
    let mut declared = false;
    while let Some(i) = rest.find("<!ENTITY") {
        declared = true;
        rest = &rest[i + 8..];
        let Some(q) = rest.find(['"', '\'']) else {
            break;
        };
        let quote = rest[q..].chars().next().unwrap_or('"');
        let value = &rest[q + 1..];
        let Some(end) = value.find(quote) else {
            break;
        };
        longest = longest.max(end);
        nested |= value[..end].contains('&');
        rest = &value[end + 1..];
    }
    if !declared {
        return Ok(());
    }
    let refs = text.matches('&').count();
    // Nested entities fan out up to 255 references each, which the parser then allows.
    let fan = if nested { 255 } else { 1 };
    if refs.saturating_mul(longest).saturating_mul(fan) > max {
        return Err(bad("entity references expand to more than the size limit"));
    }
    Ok(())
}

fn bad(why: &str) -> Error {
    Error::Malformed(Format::Svg, why.to_string())
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)
}

/// Gzip magic means `.svgz`; inflate it, refusing to grow past `max` bytes.
pub(crate) fn inflate_if_gzip(bytes: &[u8], max: usize) -> Result<std::borrow::Cow<'_, [u8]>> {
    if bytes.len() > max {
        return Err(bad("input is over the size limit"));
    }
    if !bytes.starts_with(&[0x1F, 0x8B]) {
        return Ok(bytes.into());
    }
    let mut out = Vec::new();
    let mut d = flate2::read::GzDecoder::new(bytes).take(max as u64 + 1);
    d.read_to_end(&mut out)
        .map_err(|e| bad(&format!("gzip data is damaged: {e}")))?;
    if out.len() > max {
        return Err(bad("decompressed document is over the size limit"));
    }
    Ok(out.into())
}

fn is_svg(n: Node) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(SVG_NS)
}

// ---------------------------------------------------------------------------------------------
// `use` expansion

/// Reject documents whose `<use>` references multiply into more drawn elements than the limit,
/// and reference cycles.
fn check_use_expansion(doc: &Document, limits: &SanitizeLimits) -> Result<()> {
    let mut ids: HashMap<&str, Node> = HashMap::new();
    let mut any_use = false;
    for n in doc.descendants().filter(|n| is_svg(*n)) {
        if let Some(id) = n.attribute("id") {
            ids.entry(id).or_insert(n);
        }
        any_use |= n.tag_name().name() == "use";
    }
    if !any_use {
        return Ok(());
    }
    let mut ctx = Expansion {
        ids,
        memo: HashMap::new(),
        active: HashSet::new(),
        limit: limits.max_use_expansion,
    };
    ctx.cost(doc.root_element(), 0).map(|_| ())
}

struct Expansion<'a, 'i> {
    ids: HashMap<&'a str, Node<'a, 'i>>,
    memo: HashMap<NodeId, u64>,
    active: HashSet<NodeId>,
    limit: u64,
}

impl<'a, 'i> Expansion<'a, 'i> {
    /// Elements drawn for `n`: itself, its children, and the target of a `<use>`.
    fn cost(&mut self, n: Node<'a, 'i>, hops: usize) -> Result<u64> {
        if let Some(c) = self.memo.get(&n.id()) {
            return Ok(*c);
        }
        if hops > 1024 {
            return Err(bad("references nest too deeply"));
        }
        if !self.active.insert(n.id()) {
            return Err(bad("a <use> refers back to itself"));
        }
        let mut total: u64 = 1;
        for c in n.children().filter(|c| c.is_element()) {
            total = total.saturating_add(self.cost(c, hops + 1)?);
            if total > self.limit {
                break;
            }
        }
        if is_svg(n)
            && n.tag_name().name() == "use"
            && total <= self.limit
            && let Some(target) = href_attr(n)
                .and_then(|h| h.trim().strip_prefix('#'))
                .and_then(|id| self.ids.get(id).copied())
        {
            total = total.saturating_add(self.cost(target, hops + 1)?);
        }
        self.active.remove(&n.id());
        if total > self.limit {
            return Err(bad(&format!(
                "<use> expands to over {} elements",
                self.limit
            )));
        }
        self.memo.insert(n.id(), total);
        Ok(total)
    }
}

fn href_attr<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    n.attribute("href")
        .or_else(|| n.attribute((XLINK_NS, "href")))
}

// ---------------------------------------------------------------------------------------------
// Allowlists

const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "symbol",
    "use",
    "switch",
    "desc",
    "title",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "textPath",
    "image",
    "a",
    "clipPath",
    "mask",
    "marker",
    "pattern",
    "linearGradient",
    "radialGradient",
    "stop",
    "filter",
    "style",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
];

/// Elements whose text children are kept; elsewhere text is layout whitespace.
const TEXT_ELEMENTS: &[&str] = &["text", "tspan", "textPath", "title", "desc", "style", "a"];

const ATTRIBUTES: &[&str] = &[
    // Identity, structure, geometry.
    "id",
    "class",
    "style",
    "transform",
    "x",
    "y",
    "width",
    "height",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x1",
    "y1",
    "x2",
    "y2",
    "d",
    "points",
    "viewBox",
    "preserveAspectRatio",
    "version",
    "pathLength",
    "fx",
    "fy",
    "fr",
    "refX",
    "refY",
    "href",
    "lang",
    "systemLanguage",
    "requiredFeatures",
    "transform-origin",
    "transform-box",
    "vector-effect",
    // Painting.
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-opacity",
    "opacity",
    "color",
    "display",
    "visibility",
    "overflow",
    "clip",
    "clip-path",
    "clip-rule",
    "mask",
    "mask-type",
    "filter",
    "marker-start",
    "marker-mid",
    "marker-end",
    "marker",
    "paint-order",
    "mix-blend-mode",
    "isolation",
    "shape-rendering",
    "image-rendering",
    "text-rendering",
    "color-interpolation",
    "color-interpolation-filters",
    "color-rendering",
    "enable-background",
    "stop-color",
    "stop-opacity",
    "offset",
    "flood-color",
    "flood-opacity",
    "lighting-color",
    // Units, gradients, patterns, clips, masks, markers.
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "patternUnits",
    "patternContentUnits",
    "patternTransform",
    "clipPathUnits",
    "maskUnits",
    "maskContentUnits",
    "markerUnits",
    "markerWidth",
    "markerHeight",
    "orient",
    "filterUnits",
    "primitiveUnits",
    // Filter primitives.
    "in",
    "in2",
    "result",
    "mode",
    "type",
    "values",
    "tableValues",
    "slope",
    "intercept",
    "amplitude",
    "exponent",
    "k1",
    "k2",
    "k3",
    "k4",
    "operator",
    "stdDeviation",
    "dx",
    "dy",
    "scale",
    "xChannelSelector",
    "yChannelSelector",
    "order",
    "kernelMatrix",
    "divisor",
    "bias",
    "targetX",
    "targetY",
    "edgeMode",
    "kernelUnitLength",
    "preserveAlpha",
    "radius",
    "baseFrequency",
    "numOctaves",
    "seed",
    "stitchTiles",
    "surfaceScale",
    "diffuseConstant",
    "specularConstant",
    "specularExponent",
    "azimuth",
    "elevation",
    "z",
    "pointsAtX",
    "pointsAtY",
    "pointsAtZ",
    "limitingConeAngle",
    // Text.
    "font",
    "font-family",
    "font-size",
    "font-style",
    "font-variant",
    "font-weight",
    "font-stretch",
    "text-anchor",
    "text-decoration",
    "letter-spacing",
    "word-spacing",
    "writing-mode",
    "direction",
    "unicode-bidi",
    "dominant-baseline",
    "alignment-baseline",
    "baseline-shift",
    "textLength",
    "lengthAdjust",
    "rotate",
    "startOffset",
    "method",
    "spacing",
    "side",
    "white-space",
];

fn allowed_element(name: &str) -> bool {
    ELEMENTS.contains(&name)
}

// ---------------------------------------------------------------------------------------------
// Value checks

/// Lower-case, with whitespace and control characters removed, so `java\tscript:` is seen.
fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .flat_map(char::to_lowercase)
        .collect()
}

fn has_script_scheme(squashed: &str) -> bool {
    squashed.contains("javascript:")
        || squashed.contains("vbscript:")
        || squashed.contains("expression(")
}

/// Every `url(` in `s` (already lower-cased) must point at a `#fragment`.
fn urls_are_local(s: &str) -> bool {
    let mut rest = s;
    while let Some(i) = rest.find("url(") {
        let after = rest[i + 4..].trim_start();
        let after = after
            .strip_prefix(['"', '\''])
            .unwrap_or(after)
            .trim_start();
        if !after.starts_with('#') {
            return false;
        }
        rest = &rest[i + 4..];
    }
    true
}

/// A property value, attribute or otherwise: no script scheme, no non-fragment `url(`.
fn value_ok(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    !has_script_scheme(&squash(v)) && urls_are_local(&lower)
}

fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// A stylesheet or `style` attribute that cannot load or run anything.
fn css_ok(css: &str) -> bool {
    let css = strip_css_comments(css);
    if css.contains('\\') {
        // A CSS escape can spell `url(` or `@import` past any keyword check.
        return false;
    }
    let lower = css.to_ascii_lowercase();
    const BANNED: &[&str] = &[
        "@import",
        "image-set(",
        "-webkit-image-set(",
        "src(",
        "cross-fade(",
        "element(",
        "-moz-binding",
        "behavior:",
        "<!--",
    ];
    !BANNED.iter().any(|b| lower.contains(b)) && value_ok(&css)
}

fn href_ok(v: &str) -> bool {
    let t = v.trim();
    if let Some(frag) = t.strip_prefix('#') {
        return !frag.is_empty() && !frag.chars().any(char::is_control);
    }
    let lower = t.get(..24.min(t.len())).unwrap_or("").to_ascii_lowercase();
    ["png", "jpeg", "gif", "webp"].iter().any(|m| {
        lower
            .strip_prefix("data:image/")
            .and_then(|r| r.strip_prefix(m))
            .is_some_and(|r| r.starts_with(';') || r.starts_with(','))
    })
}

/// The output name and cleaned value of an attribute, `None` when it is dropped.
fn clean_attribute(el: &str, a: roxmltree::Attribute) -> Option<(&'static str, String)> {
    let local = a.name();
    let name = match a.namespace() {
        None => local,
        Some(XLINK_NS) if local == "href" => "href",
        Some(XML_NS) if local == "space" => return Some(("xml:space", a.value().to_string())),
        _ => return None,
    };
    let key: &'static str = ATTRIBUTES.iter().copied().find(|k| *k == name)?;
    let v = a.value();
    let ok = match key {
        // A `<use>` can only instantiate something in this document.
        "href" => href_ok(v) && (el != "use" || v.trim_start().starts_with('#')),
        "style" => css_ok(v),
        _ => value_ok(v),
    };
    if !ok {
        return None;
    }
    let out_name = if a.namespace() == Some(XLINK_NS) {
        "xlink:href"
    } else {
        key
    };
    Some((out_name, v.to_string()))
}

// ---------------------------------------------------------------------------------------------
// Writer

fn write_element(
    n: Node,
    depth: usize,
    limits: &SanitizeLimits,
    root: bool,
    out: &mut String,
) -> Result<()> {
    if depth > limits.max_depth {
        return Err(bad(&format!(
            "nesting is deeper than the {} limit",
            limits.max_depth
        )));
    }
    let name = n.tag_name().name();
    if name == "style" {
        let css: String = n.children().filter_map(|c| c.text()).collect();
        if !css_ok(&css) {
            return Ok(());
        }
    }
    out.push('<');
    out.push_str(name);
    if root {
        out.push_str(
            " xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"",
        );
    }
    for a in n.attributes() {
        if let Some((k, v)) = clean_attribute(name, a) {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            escape(&v, true, out);
            out.push('"');
        }
    }
    let keeps_text = TEXT_ELEMENTS.contains(&name);
    let mut body = String::new();
    for c in n.children() {
        if c.is_element() {
            if is_svg(c) && allowed_element(c.tag_name().name()) {
                write_element(c, depth + 1, limits, false, &mut body)?;
            }
        } else if keeps_text
            && let Some(t) = c.text()
            && !has_script_scheme(&squash(t))
        {
            escape(t, false, &mut body);
        }
    }
    if body.is_empty() {
        out.push_str("/>");
    } else {
        out.push('>');
        out.push_str(&body);
        out.push_str("</");
        out.push_str(name);
        out.push('>');
    }
    Ok(())
}

fn escape(s: &str, attribute: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\n' if attribute => out.push_str("&#10;"),
            '\t' if attribute => out.push_str("&#9;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(s: &str) -> String {
        String::from_utf8(sanitize_svg(s.as_bytes()).unwrap()).unwrap()
    }

    #[test]
    fn benign_passes_and_is_well_formed() {
        let out = clean(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="4" height="4" fill="#f00" style="stroke:#000"/></svg>"##,
        );
        assert!(out.contains("<rect"));
        assert!(roxmltree::Document::parse(&out).is_ok());
    }

    #[test]
    fn root_must_be_svg() {
        assert!(sanitize_svg(b"<html xmlns='http://www.w3.org/1999/xhtml'/>").is_err());
        assert!(sanitize_svg(b"<svg/>").is_err());
    }
}
