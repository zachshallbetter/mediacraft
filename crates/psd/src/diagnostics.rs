//! Diagnostic reporting, anomaly detection, and validation for PSD/PSB files.

use crate::file::PsdFile;
use crate::header::Version;
use crate::resources::ids;
use crate::tagged::{BlockData, SectionType};

/// Detailed diagnostic report for a PSD/PSB file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PsdDiagnosticReport {
    /// File format version (PSD v1 or PSB v2).
    pub version: Version,
    /// Canvas width in pixels.
    pub canvas_width: u32,
    /// Canvas height in pixels.
    pub canvas_height: u32,
    /// Color channels in composite image.
    pub channels: u16,
    /// Bits per channel (1, 8, 16, 32).
    pub depth: u16,
    /// Total layer count.
    pub layer_count: usize,
    /// Layer group count.
    pub group_count: usize,
    /// Image resource count.
    pub resource_count: usize,
    /// Global tagged block count.
    pub global_block_count: usize,
    /// Estimated uncompressed composite pixel memory in bytes.
    pub uncompressed_composite_bytes: u64,
    /// Estimated uncompressed layer memory in bytes across all layers.
    pub uncompressed_layers_bytes: u64,
    /// Whether DPI resolution info (resource ID 1005) is present.
    pub has_resolution_info: bool,
    /// Whether an embedded ICC color profile (resource ID 1039) is present.
    pub has_icc_profile: bool,
    /// Detected structural warnings or anomalies.
    pub warnings: Vec<String>,
    /// Fatal structural errors or policy violations.
    pub errors: Vec<String>,
}

impl PsdFile {
    /// Inspects the parsed PSD/PSB file and generates a comprehensive diagnostic report.
    pub fn diagnose(&self) -> PsdDiagnosticReport {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // 1. Geometry and Dimension checks
        if self.header.width == 0 || self.header.height == 0 {
            errors.push(format!(
                "canvas dimensions cannot be zero: {}x{}",
                self.header.width, self.header.height
            ));
        }

        let max_dim = if self.header.version.is_psb() {
            300_000
        } else {
            30_000
        };
        if self.header.width > max_dim || self.header.height > max_dim {
            errors.push(format!(
                "canvas dimensions {}x{} exceed maximum limit of {} for {:?}",
                self.header.width, self.header.height, max_dim, self.header.version
            ));
        }

        // 2. Resource inspection
        let mut seen_resource_ids = std::collections::HashSet::new();
        let mut has_resolution_info = false;
        let mut has_icc_profile = false;

        for res in &self.resources {
            if !seen_resource_ids.insert(res.id) {
                warnings.push(format!("duplicate image resource block with ID {}", res.id));
            }
            if res.id == ids::RESOLUTION_INFO {
                has_resolution_info = true;
            }
            if res.id == ids::ICC_PROFILE {
                has_icc_profile = true;
            }
        }

        // 3. Layer inspection and memory calculation
        let mut layer_count = 0;
        let mut group_count: usize = 0;
        let mut group_depth: usize = 0;
        let mut uncompressed_layers_bytes: u64 = 0;

        let bytes_per_sample = match self.header.depth {
            16 => 2u64,
            32 => 4u64,
            _ => 1u64,
        };

        if let Some(ref li) = self.layer_info {
            layer_count = li.layers.len();

            for layer in &li.layers {
                let name = layer.name();
                let w = layer.rect.width();
                let h = layer.rect.height();

                if w < 0 || h < 0 {
                    errors.push(format!(
                        "layer '{}' has negative dimensions: {}x{}",
                        name, w, h
                    ));
                } else if w > 0 && h > 0 {
                    // Check if bounds extend outside canvas
                    let out_of_bounds = layer.rect.left < 0
                        || layer.rect.top < 0
                        || layer.rect.right > (self.header.width as i32)
                        || layer.rect.bottom > (self.header.height as i32);
                    if out_of_bounds {
                        warnings.push(format!(
                            "layer '{}' extends beyond canvas bounds ([{}, {}, {}, {}] vs [0, 0, {}, {}])",
                            name,
                            layer.rect.left,
                            layer.rect.top,
                            layer.rect.right,
                            layer.rect.bottom,
                            self.header.width,
                            self.header.height
                        ));
                    }

                    let layer_pixel_count = (w as u64).saturating_mul(h as u64);
                    let channel_count = layer.channels.len() as u64;
                    let layer_bytes = layer_pixel_count
                        .saturating_mul(channel_count)
                        .saturating_mul(bytes_per_sample);
                    uncompressed_layers_bytes =
                        uncompressed_layers_bytes.saturating_add(layer_bytes);
                }

                // Check group hierarchy via SectionDivider
                for block in &layer.blocks {
                    if let Some(Ok(BlockData::SectionDivider(ref sd))) = block.parsed() {
                        match sd.kind {
                            SectionType::OpenFolder | SectionType::ClosedFolder => {
                                group_count = group_count.saturating_add(1);
                                group_depth = group_depth.saturating_add(1);
                            }
                            SectionType::BoundingDivider => {
                                group_depth = group_depth.saturating_sub(1);
                            }
                            SectionType::Other | SectionType::Unknown(_) => {}
                        }
                    }
                }
            }
        }

        if group_depth > 0 {
            warnings.push(format!(
                "unbalanced layer group hierarchy: {} unclosed folder group(s)",
                group_depth
            ));
        }

        // 4. Composite uncompressed bytes
        let composite_pixels = (self.header.width as u64).saturating_mul(self.header.height as u64);
        let uncompressed_composite_bytes = composite_pixels
            .saturating_mul(self.header.channels as u64)
            .saturating_mul(bytes_per_sample);

        // 5. Trailing bytes check
        if !self.layer_mask_trailing.is_empty() {
            warnings.push(format!(
                "layer and mask section contains {} unparsed trailing bytes",
                self.layer_mask_trailing.len()
            ));
        }

        PsdDiagnosticReport {
            version: self.header.version,
            canvas_width: self.header.width,
            canvas_height: self.header.height,
            channels: self.header.channels,
            depth: self.header.depth,
            layer_count,
            group_count,
            resource_count: self.resources.len(),
            global_block_count: self.global_blocks.len(),
            uncompressed_composite_bytes,
            uncompressed_layers_bytes,
            has_resolution_info,
            has_icc_profile,
            warnings,
            errors,
        }
    }
}
