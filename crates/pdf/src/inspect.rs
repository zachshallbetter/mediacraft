//! High-level metadata, dimension, and page inspection for PDF deliverables.

use crate::cos::{Dict, Document, ObjRef, Object};
use std::sync::Arc;

/// Inspected metadata and geometry of a PDF document.
pub struct PdfInspector {
    doc: Document,
    info_dict: Option<Dict>,
    pages_root: Option<ObjRef>,
    page_count: usize,
}

impl PdfInspector {
    /// Opens and inspects a PDF byte stream.
    pub fn open(bytes: &[u8]) -> Result<Self, crate::cos::CosError> {
        let arc_data = Arc::new(bytes.to_vec());
        let doc = Document::open(arc_data)?;

        let info_dict = doc.trailer().get(b"Info").and_then(|o| {
            let resolved = doc.resolve(o);
            match resolved.as_ref() {
                Object::Dict(d) => Some(d.clone()),
                _ => None,
            }
        });

        let mut pages_root = None;
        let mut page_count = 0;

        if let Some(root_ref) = doc.root() {
            let root_obj = doc.get(root_ref);
            if let Object::Dict(cat) = root_obj.as_ref()
                && let Some(pages_obj) = cat.get(b"Pages")
            {
                let resolved = doc.resolve(pages_obj);
                if let Object::Dict(pages_d) = resolved.as_ref() {
                    if let Object::Ref(pr) = pages_obj {
                        pages_root = Some(*pr);
                    }
                    if let Some(c) = pages_d.int(b"Count")
                        && c >= 0
                    {
                        page_count = c as usize;
                    }
                }
            }
        }

        Ok(Self {
            doc,
            info_dict,
            pages_root,
            page_count,
        })
    }

    /// Total number of pages declared in the page tree.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// PDF Specification version (e.g., "1.4", "1.7", "2.0").
    pub fn version(&self) -> &str {
        self.doc.version()
    }

    /// Whether the document has an active security handler (encrypted).
    pub fn is_encrypted(&self) -> bool {
        self.doc.security().is_some()
    }

    /// Document title from `/Info /Title`.
    pub fn title(&self) -> Option<String> {
        self.get_info_string(b"Title")
    }

    /// Document author from `/Info /Author`.
    pub fn author(&self) -> Option<String> {
        self.get_info_string(b"Author")
    }

    /// Document subject from `/Info /Subject`.
    pub fn subject(&self) -> Option<String> {
        self.get_info_string(b"Subject")
    }

    /// Document creator / producing application from `/Info /Creator`.
    pub fn creator(&self) -> Option<String> {
        self.get_info_string(b"Creator")
    }

    /// Document creation date from `/Info /CreationDate`.
    pub fn creation_date(&self) -> Option<String> {
        self.get_info_string(b"CreationDate")
    }

    /// Helper to extract string from /Info
    fn get_info_string(&self, key: &[u8]) -> Option<String> {
        let dict = self.info_dict.as_ref()?;
        let val = dict.get(key)?;
        let resolved = self.doc.resolve(val);
        match resolved.as_ref() {
            Object::String(s) => Some(s.to_text()),
            Object::Name(n) => Some(String::from_utf8_lossy(n.as_ref()).to_string()),
            _ => None,
        }
    }

    /// Page dimensions `[x, y, width, height]` (MediaBox or CropBox).
    pub fn dimensions(&self, page_index: usize) -> Option<[f64; 4]> {
        let page_dict = self.find_page_dict(page_index)?;
        let rect = page_dict
            .get(b"CropBox")
            .or_else(|| page_dict.get(b"MediaBox"))?;
        let resolved = self.doc.resolve(rect);
        if let Object::Array(arr) = resolved.as_ref()
            && arr.len() >= 4
        {
            let x0 = Self::object_to_f64(&self.doc.resolve(&arr[0]))?;
            let y0 = Self::object_to_f64(&self.doc.resolve(&arr[1]))?;
            let x1 = Self::object_to_f64(&self.doc.resolve(&arr[2]))?;
            let y1 = Self::object_to_f64(&self.doc.resolve(&arr[3]))?;
            let width = (x1 - x0).abs();
            let height = (y1 - y0).abs();
            return Some([x0.min(x1), y0.min(y1), width, height]);
        }
        None
    }

    fn object_to_f64(obj: &Object) -> Option<f64> {
        obj.as_f64()
    }

    /// Traverses the page tree to find the `Dict` of the page at `target_index`.
    fn find_page_dict(&self, target_index: usize) -> Option<Dict> {
        let root = self.pages_root?;
        let mut current_idx = 0;
        self.traverse_pages(root, &mut current_idx, target_index)
    }

    /// Produces a comprehensive diagnostic receipt evaluating declared vs actual page tree geometry, encryption, and metadata.
    pub fn diagnose(&self) -> PdfDiagnosticReport {
        let mut warnings = Vec::new();
        let mut page_dimensions = Vec::new();

        // Enumerate reachable pages
        let mut actual_pages = 0;
        if let Some(root) = self.pages_root {
            actual_pages = self.count_reachable_pages(root, 0);
        }

        for i in 0..actual_pages {
            if let Some(dims) = self.dimensions(i) {
                page_dimensions.push(dims);
            }
        }

        if actual_pages != self.page_count {
            warnings.push(format!(
                "Declared page count ({}) differs from actual reachable pages ({})",
                self.page_count, actual_pages
            ));
        }

        if self.is_encrypted() {
            warnings.push("Document has active encryption (Standard Security Handler)".to_string());
        }

        tracing::debug!(
            version = %self.version(),
            declared_pages = self.page_count,
            actual_pages,
            encrypted = self.is_encrypted(),
            "pdf diagnostics generated"
        );

        PdfDiagnosticReport {
            version: self.version().to_string(),
            is_encrypted: self.is_encrypted(),
            page_count_declared: self.page_count,
            page_count_actual: actual_pages,
            title: self.title(),
            author: self.author(),
            creator: self.creator(),
            subject: self.subject(),
            creation_date: self.creation_date(),
            page_dimensions,
            warnings,
        }
    }

    fn count_reachable_pages(&self, node_ref: ObjRef, depth: usize) -> usize {
        if depth > 32 {
            // Guard against cyclic or absurdly deep page trees
            return 0;
        }
        let node_obj = self.doc.get(node_ref);
        let dict = match node_obj.as_ref() {
            Object::Dict(d) => d,
            _ => return 0,
        };

        if dict.name(b"Type") == Some(b"Page") {
            return 1;
        }

        let mut count: usize = 0;
        if let Some(Object::Array(kids)) = dict.get(b"Kids") {
            for kid in kids {
                if let Object::Ref(kr) = kid {
                    count = count
                        .saturating_add(self.count_reachable_pages(*kr, depth.saturating_add(1)));
                }
            }
        }
        count
    }

    fn traverse_pages(
        &self,
        node_ref: ObjRef,
        current_idx: &mut usize,
        target_index: usize,
    ) -> Option<Dict> {
        let node_obj = self.doc.get(node_ref);
        let dict = match node_obj.as_ref() {
            Object::Dict(d) => d,
            _ => return None,
        };

        let type_name = dict.name(b"Type");

        if type_name == Some(b"Page") {
            if *current_idx == target_index {
                return Some(dict.clone());
            }
            *current_idx += 1;
            return None;
        }

        // Otherwise it's a Pages intermediate node
        if let Some(Object::Array(kids)) = dict.get(b"Kids") {
            for kid in kids {
                if let Object::Ref(kr) = kid
                    && let Some(p) = self.traverse_pages(*kr, current_idx, target_index)
                {
                    return Some(p);
                }
            }
        }
        None
    }
}

/// Comprehensive diagnostic report of a PDF file's structure, encryption, metadata, and geometry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PdfDiagnosticReport {
    pub version: String,
    pub is_encrypted: bool,
    pub page_count_declared: usize,
    pub page_count_actual: usize,
    pub title: Option<String>,
    pub author: Option<String>,
    pub creator: Option<String>,
    pub subject: Option<String>,
    pub creation_date: Option<String>,
    pub page_dimensions: Vec<[f64; 4]>,
    pub warnings: Vec<String>,
}
