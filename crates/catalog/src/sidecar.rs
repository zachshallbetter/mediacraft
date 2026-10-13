//! Portable Metadata Sidecar Engine (XMP & JSON) for MediaCraft.
//!
//! Provides non-destructive, portable sidecar storage for digital asset management
//! metadata (ratings, color labels, keywords/tags, titles, and descriptions).
//!
//! Conforms to ISO 16684-1 (XMP data model) and Dublin Core metadata vocabularies
//! without mutating original media files or relying on proprietary formats.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Supported metadata sidecar serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SidecarFormat {
    /// Standard ISO 16684-1 XMP XML packet (.xmp).
    Xmp,
    /// Fast native MediaCraft JSON sidecar (.mcsidecar).
    Json,
}

impl SidecarFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Xmp => "xmp",
            Self::Json => "mcsidecar",
        }
    }
}

/// Errors occurring during sidecar reading, parsing, or serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarError {
    Io(String),
    Serialization(String),
    Parse(String),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::Serialization(msg) => write!(f, "Serialization error: {msg}"),
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
        }
    }
}

impl std::error::Error for SidecarError {}

/// Portable metadata packet stored in external sidecar files.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MediaSidecar {
    /// Rating from 1 to 5 stars (0 or None for unrated).
    pub rating: Option<u8>,
    /// Color label or status (e.g., "Select", "Review", "Approved", "red", "green").
    pub label: Option<String>,
    /// Dublin Core subject tags / keywords.
    pub tags: Vec<String>,
    /// Asset title / display headline.
    pub title: Option<String>,
    /// Extended notes or description.
    pub description: Option<String>,
    /// Author, operator, or creator identity.
    pub creator: Option<String>,
}

impl MediaSidecar {
    /// Creates a new empty sidecar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serializes metadata to an ISO 16684-1 compliant XMP XML packet.
    pub fn to_xmp_xml(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
        out.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"MediaCraft Core 1.0\">\n");
        out.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
        out.push_str("    <rdf:Description rdf:about=\"\"\n");
        out.push_str("        xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n");
        out.push_str("        xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n");

        if let Some(r) = self.rating {
            out.push_str(&format!("      <xmp:Rating>{}</xmp:Rating>\n", r.min(5)));
        }

        if let Some(ref l) = self.label {
            out.push_str(&format!("      <xmp:Label>{}</xmp:Label>\n", xml_escape(l)));
        }

        if let Some(ref c) = self.creator {
            out.push_str("      <dc:creator>\n");
            out.push_str("        <rdf:Seq>\n");
            out.push_str(&format!("          <rdf:li>{}</rdf:li>\n", xml_escape(c)));
            out.push_str("        </rdf:Seq>\n");
            out.push_str("      </dc:creator>\n");
        }

        if let Some(ref t) = self.title {
            out.push_str("      <dc:title>\n");
            out.push_str("        <rdf:Alt>\n");
            out.push_str(&format!(
                "          <rdf:li xml:lang=\"x-default\">{}</rdf:li>\n",
                xml_escape(t)
            ));
            out.push_str("        </rdf:Alt>\n");
            out.push_str("      </dc:title>\n");
        }

        if let Some(ref d) = self.description {
            out.push_str("      <dc:description>\n");
            out.push_str("        <rdf:Alt>\n");
            out.push_str(&format!(
                "          <rdf:li xml:lang=\"x-default\">{}</rdf:li>\n",
                xml_escape(d)
            ));
            out.push_str("        </rdf:Alt>\n");
            out.push_str("      </dc:description>\n");
        }

        if !self.tags.is_empty() {
            out.push_str("      <dc:subject>\n");
            out.push_str("        <rdf:Bag>\n");
            for tag in &self.tags {
                out.push_str(&format!("          <rdf:li>{}</rdf:li>\n", xml_escape(tag)));
            }
            out.push_str("        </rdf:Bag>\n");
            out.push_str("      </dc:subject>\n");
        }

        out.push_str("    </rdf:Description>\n");
        out.push_str("  </rdf:RDF>\n");
        out.push_str("</x:xmpmeta>\n");
        out.push_str("<?xpacket end=\"w\"?>\n");

        out
    }

    /// Parses an XMP XML packet extracting rating, label, tags, title, description, and creator.
    pub fn from_xmp_xml(xml: &str) -> Result<Self, SidecarError> {
        let mut sidecar = Self::new();

        // Extract <xmp:Rating>
        if let Some(num) =
            extract_tag_value(xml, "xmp:Rating").and_then(|s| s.trim().parse::<u8>().ok())
        {
            sidecar.rating = Some(num.min(5));
        }

        // Extract <xmp:Label>
        if let Some(label_str) = extract_tag_value(xml, "xmp:Label") {
            let decoded = xml_unescape(label_str.trim());
            if !decoded.is_empty() {
                sidecar.label = Some(decoded);
            }
        }

        // Extract <dc:title> inside <rdf:li>
        if let Some(title_val) =
            extract_tag_block(xml, "dc:title").and_then(|b| extract_tag_value(&b, "rdf:li"))
        {
            let decoded = xml_unescape(title_val.trim());
            if !decoded.is_empty() {
                sidecar.title = Some(decoded);
            }
        }

        // Extract <dc:description> inside <rdf:li>
        if let Some(desc_val) =
            extract_tag_block(xml, "dc:description").and_then(|b| extract_tag_value(&b, "rdf:li"))
        {
            let decoded = xml_unescape(desc_val.trim());
            if !decoded.is_empty() {
                sidecar.description = Some(decoded);
            }
        }

        // Extract <dc:creator> inside <rdf:li>
        if let Some(creator_val) =
            extract_tag_block(xml, "dc:creator").and_then(|b| extract_tag_value(&b, "rdf:li"))
        {
            let decoded = xml_unescape(creator_val.trim());
            if !decoded.is_empty() {
                sidecar.creator = Some(decoded);
            }
        }

        // Extract <dc:subject> tags inside <rdf:Bag> -> <rdf:li>
        if let Some(subject_block) = extract_tag_block(xml, "dc:subject") {
            let tags = extract_all_tag_values(&subject_block, "rdf:li");
            for t in tags {
                let decoded = xml_unescape(t.trim());
                if !decoded.is_empty() && !sidecar.tags.contains(&decoded) {
                    sidecar.tags.push(decoded);
                }
            }
        }

        Ok(sidecar)
    }

    /// Serializes metadata to formatted JSON.
    pub fn to_json(&self) -> Result<String, SidecarError> {
        serde_json::to_string_pretty(self).map_err(|e| SidecarError::Serialization(e.to_string()))
    }

    /// Deserializes metadata from JSON.
    pub fn from_json(json_str: &str) -> Result<Self, SidecarError> {
        serde_json::from_str(json_str).map_err(|e| SidecarError::Parse(e.to_string()))
    }
}

/// Computes the expected sidecar path for a given media file.
pub fn sidecar_path_for_asset(asset_path: &Path, format: SidecarFormat) -> PathBuf {
    match format {
        SidecarFormat::Xmp => asset_path.with_extension("xmp"),
        SidecarFormat::Json => asset_path.with_extension("mcsidecar"),
    }
}

/// Atomically writes a sidecar file next to the source media asset.
pub fn write_sidecar(
    asset_path: &Path,
    sidecar: &MediaSidecar,
    format: SidecarFormat,
) -> Result<PathBuf, SidecarError> {
    let sidecar_path = sidecar_path_for_asset(asset_path, format);

    let content = match format {
        SidecarFormat::Xmp => sidecar.to_xmp_xml(),
        SidecarFormat::Json => sidecar.to_json()?,
    };

    craft_atomic::atomic_write(&sidecar_path, content.as_bytes()).map_err(|e| {
        SidecarError::Io(format!(
            "Failed to write sidecar at {}: {e}",
            sidecar_path.display()
        ))
    })?;

    Ok(sidecar_path)
}

/// Reads and parses an existing sidecar file (.xmp or .mcsidecar) for an asset if present.
pub fn read_sidecar(asset_path: &Path) -> Result<Option<MediaSidecar>, SidecarError> {
    // Prefer native JSON sidecar if present
    let json_path = sidecar_path_for_asset(asset_path, SidecarFormat::Json);
    if json_path.is_file() {
        let text = fs::read_to_string(&json_path)
            .map_err(|e| SidecarError::Io(format!("Reading {}: {e}", json_path.display())))?;
        return MediaSidecar::from_json(&text).map(Some);
    }

    // Try standard .xmp sidecar
    let xmp_path = sidecar_path_for_asset(asset_path, SidecarFormat::Xmp);
    if xmp_path.is_file() {
        let text = fs::read_to_string(&xmp_path)
            .map_err(|e| SidecarError::Io(format!("Reading {}: {e}", xmp_path.display())))?;
        return MediaSidecar::from_xmp_xml(&text).map(Some);
    }

    // Try compound extension e.g. "take1.mov.xmp"
    let compound_xmp_name = match asset_path.file_name() {
        Some(name) => format!("{}.xmp", name.to_string_lossy()),
        None => return Ok(None),
    };
    let compound_xmp = asset_path.with_file_name(compound_xmp_name);
    if compound_xmp.is_file() {
        let text = fs::read_to_string(&compound_xmp)
            .map_err(|e| SidecarError::Io(format!("Reading {}: {e}", compound_xmp.display())))?;
        return MediaSidecar::from_xmp_xml(&text).map(Some);
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// XML Helpers (pure-Rust, safe, non-panicking, zero external dependencies)
// ---------------------------------------------------------------------------

fn xml_escape(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

fn xml_unescape(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn extract_tag_value(source: &str, tag_name: &str) -> Option<String> {
    let open_prefix = format!("<{tag_name}");
    let close_tag = format!("</{tag_name}>");

    let start_idx = source.find(&open_prefix)?;
    let after_tag = start_idx.checked_add(open_prefix.len())?;
    let char_after = source.get(after_tag..after_tag.saturating_add(1))?;
    if char_after != ">" && char_after != " " && char_after != "\n" && char_after != "\t" {
        return None;
    }

    let head_rel = source[start_idx..].find('>')?;
    let content_start = start_idx.checked_add(head_rel)?.checked_add(1)?;
    let end_rel = source[content_start..].find(&close_tag)?;
    let content_end = content_start.checked_add(end_rel)?;

    if source.is_char_boundary(content_start) && source.is_char_boundary(content_end) {
        Some(source[content_start..content_end].to_string())
    } else {
        None
    }
}

fn extract_tag_block(source: &str, tag_name: &str) -> Option<String> {
    let open_prefix = format!("<{tag_name}");
    let close_tag = format!("</{tag_name}>");

    let start_idx = source.find(&open_prefix)?;
    let tag_head_end = source[start_idx..].find('>')?;
    let block_start = start_idx.checked_add(tag_head_end)?.checked_add(1)?;
    let end_idx = source[block_start..].find(&close_tag)?;
    let block_end = block_start.checked_add(end_idx)?;

    if source.is_char_boundary(block_start) && source.is_char_boundary(block_end) {
        Some(source[block_start..block_end].to_string())
    } else {
        None
    }
}

fn extract_all_tag_values(source: &str, tag_name: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut cursor = 0;
    let open_prefix = format!("<{tag_name}");
    let close_tag = format!("</{tag_name}>");

    while cursor < source.len() {
        let sub = match source.get(cursor..) {
            Some(s) => s,
            None => break,
        };

        let start_rel = match sub.find(&open_prefix) {
            Some(s) => s,
            None => break,
        };

        let after_tag = start_rel.saturating_add(open_prefix.len());
        if matches!(
            sub.get(after_tag..after_tag.saturating_add(1)),
            Some(c) if c != ">" && c != " " && c != "\n" && c != "\t"
        ) {
            cursor = cursor.saturating_add(after_tag);
            continue;
        }

        let tag_head_end = match sub[start_rel..].find('>') {
            Some(e) => e,
            None => break,
        };

        let content_start_rel = match start_rel
            .checked_add(tag_head_end)
            .and_then(|x| x.checked_add(1))
        {
            Some(s) => s,
            None => break,
        };

        let sub_content = match sub.get(content_start_rel..) {
            Some(s) => s,
            None => break,
        };

        let end_rel = match sub_content.find(&close_tag) {
            Some(e) => e,
            None => break,
        };

        if sub_content.is_char_boundary(0) && sub_content.is_char_boundary(end_rel) {
            let val = sub_content[..end_rel].to_string();
            results.push(val);
        }

        let advance = match content_start_rel
            .checked_add(end_rel)
            .and_then(|x| x.checked_add(close_tag.len()))
        {
            Some(a) => a,
            None => break,
        };

        cursor = match cursor.checked_add(advance) {
            Some(c) => c,
            None => break,
        };
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xmp_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let sidecar = MediaSidecar {
            rating: Some(4),
            label: Some("Select".to_string()),
            tags: vec!["b-roll".to_string(), "4k".to_string(), "drone".to_string()],
            title: Some("Alpine Summit Take 01".to_string()),
            description: Some("Hero shot at golden hour with morning mist".to_string()),
            creator: Some("Zach Shallbetter".to_string()),
        };

        let xml = sidecar.to_xmp_xml();
        assert!(xml.contains("<xmp:Rating>4</xmp:Rating>"));
        assert!(xml.contains("<xmp:Label>Select</xmp:Label>"));
        assert!(xml.contains("<rdf:li>b-roll</rdf:li>"));
        assert!(xml.contains("Alpine Summit Take 01"));

        let parsed = MediaSidecar::from_xmp_xml(&xml)?;
        assert_eq!(parsed.rating, Some(4));
        assert_eq!(parsed.label.as_deref(), Some("Select"));
        assert_eq!(parsed.title.as_deref(), Some("Alpine Summit Take 01"));
        assert_eq!(
            parsed.description.as_deref(),
            Some("Hero shot at golden hour with morning mist")
        );
        assert_eq!(parsed.creator.as_deref(), Some("Zach Shallbetter"));
        assert_eq!(parsed.tags, vec!["b-roll", "4k", "drone"]);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let sidecar = MediaSidecar {
            rating: Some(5),
            label: Some("Approved".to_string()),
            tags: vec!["client".to_string(), "deliverable".to_string()],
            title: Some("Final Master".to_string()),
            description: None,
            creator: Some("Studio".to_string()),
        };

        let json = sidecar.to_json()?;
        let parsed = MediaSidecar::from_json(&json)?;
        assert_eq!(sidecar, parsed);
        Ok(())
    }

    #[test]
    fn test_atomic_sidecar_file_writing() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!("mc_sidecar_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let asset_path = temp_dir.join("Clip_001.mov");
        let _ = fs::write(&asset_path, b"test payload");

        let sidecar = MediaSidecar {
            rating: Some(5),
            label: Some("Green".to_string()),
            tags: vec!["commercial".to_string()],
            title: Some("Commercial Teaser".to_string()),
            description: Some("Cut for social platforms".to_string()),
            creator: Some("Zach Shallbetter".to_string()),
        };

        // Write XMP
        let xmp_written = write_sidecar(&asset_path, &sidecar, SidecarFormat::Xmp)?;
        assert!(xmp_written.is_file());

        // Read XMP back
        let loaded = read_sidecar(&asset_path)?.ok_or("sidecar missing")?;
        assert_eq!(loaded.rating, Some(5));
        assert_eq!(loaded.label.as_deref(), Some("Green"));
        assert_eq!(loaded.tags, vec!["commercial"]);

        // Cleanup
        let _ = fs::remove_file(xmp_written);
        let _ = fs::remove_file(asset_path);
        let _ = fs::remove_dir(temp_dir);
        Ok(())
    }
}
