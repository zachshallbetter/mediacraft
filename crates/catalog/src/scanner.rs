//! Real filesystem scanner and media format prober for MediaCraft.
//!
//! Recursively indexes local directories, extracts stream properties, and merges
//! detected media into the workspace catalog.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::{ArchiveInfo, DocumentInfo, ImageInfo, MediaAsset, MediaCatalog, MediaKind};

/// Options controlling directory scanning depth and filters.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_depth: usize,
    pub include_hidden: bool,
    pub min_file_size_bytes: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: 8,
            include_hidden: false,
            min_file_size_bytes: 64,
        }
    }
}

/// Detailed diagnostic report of a catalog directory scan.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanReport {
    pub assets_indexed: usize,
    pub files_scanned: usize,
    pub directories_visited: usize,
    pub skipped_unsupported: usize,
    pub skipped_too_small: usize,
    pub errors: Vec<ScanErrorItem>,
}

/// Information about an error encountered while scanning a specific path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanErrorItem {
    pub path: String,
    pub error: String,
}

/// Recursively scans a filesystem path for media assets, returning the collected assets.
pub fn scan_path(root: &Path, options: &ScanOptions) -> Vec<MediaAsset> {
    scan_path_with_report(root, options).0
}

/// Recursively scans a filesystem path for media assets, returning both collected assets and a full diagnostic scan report.
pub fn scan_path_with_report(root: &Path, options: &ScanOptions) -> (Vec<MediaAsset>, ScanReport) {
    let mut results = Vec::new();
    let mut report = ScanReport::default();

    if !root.exists() {
        tracing::warn!(path = %root.display(), "scan path does not exist");
        return (results, report);
    }

    if root.is_file() {
        report.files_scanned = 1;
        if let Some(asset) = probe_file(root) {
            report.assets_indexed = 1;
            results.push(asset);
        } else {
            report.skipped_unsupported = 1;
        }
        return (results, report);
    }

    scan_dir_recursive(root, 0, options, &mut results, &mut report);
    tracing::info!(
        indexed = report.assets_indexed,
        scanned = report.files_scanned,
        directories = report.directories_visited,
        errors = report.errors.len(),
        "catalog scan complete"
    );

    (results, report)
}

fn scan_dir_recursive(
    dir: &Path,
    current_depth: usize,
    options: &ScanOptions,
    results: &mut Vec<MediaAsset>,
    report: &mut ScanReport,
) {
    if current_depth > options.max_depth {
        return;
    }

    report.directories_visited = report.directories_visited.saturating_add(1);

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "unable to read directory");
            report.errors.push(ScanErrorItem {
                path: dir.to_string_lossy().to_string(),
                error: e.to_string(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();

        if !options.include_hidden && filename.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            scan_dir_recursive(
                &path,
                current_depth.saturating_add(1),
                options,
                results,
                report,
            );
        } else if path.is_file() {
            report.files_scanned = report.files_scanned.saturating_add(1);
            if entry
                .metadata()
                .map(|m| m.len() < options.min_file_size_bytes)
                .unwrap_or(false)
            {
                report.skipped_too_small = report.skipped_too_small.saturating_add(1);
                continue;
            }
            if let Some(asset) = probe_file(&path) {
                report.assets_indexed = report.assets_indexed.saturating_add(1);
                results.push(asset);
            } else {
                report.skipped_unsupported = report.skipped_unsupported.saturating_add(1);
            }
        }
    }
}

/// Probes a single file on disk to determine its media properties.
pub fn probe_file(path: &Path) -> Option<MediaAsset> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = MediaKind::from_extension(&ext);
    if kind == MediaKind::Other {
        return None;
    }

    let meta = fs::metadata(path).ok()?;
    let file_size = meta.len();
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".to_string());
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".to_string());

    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| {
            let secs = d.as_secs();
            format!("1970-01-01 +{}s", secs)
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Video and audio stream details come from MediaCraft's own demuxers and parsers.
    let (video, audio, duration_secs) = match kind {
        MediaKind::Video | MediaKind::Audio => crate::av_probe::probe(path),
        _ => None,
    }
    .unwrap_or((None, None, None));

    let (document, archive) = match kind {
        MediaKind::Document if ext == "pdf" => (probe_pdf(path), None),
        MediaKind::Archive => (None, probe_archive(path)),
        _ => (None, None),
    };

    let still = match kind {
        MediaKind::Image | MediaKind::RawDng => {
            craft_image::probe_file(path, craft_image::DEFAULT_PROBE_BYTES).ok()
        }
        _ => None,
    };
    let m = still.as_ref().map(|s| &s.metadata);

    Some(MediaAsset {
        id,
        path: path.to_path_buf(),
        filename,
        file_size_bytes: file_size,
        kind,
        duration_secs,
        timecode_start: "00:00:00:00".to_string(),
        video,
        audio,
        camera_model: m.and_then(camera_name),
        date_created: m.and_then(|m| m.capture_time.clone()).unwrap_or(modified),
        geo_location: m
            .and_then(|m| m.gps)
            .map(|g| format!("{:.5}, {:.5}", g.latitude, g.longitude)),
        rating: m
            .and_then(|m| m.rating)
            .map_or(0, |r| u8::try_from(r.max(0)).unwrap_or(0)),
        tags: m.map(|m| m.keywords.clone()).unwrap_or_default(),
        has_proxy: false,
        needs_transcode: false,
        is_offline: false,
        document,
        archive,
        image: still.as_ref().map(image_info),
    })
}

/// The catalogue's view of a `craft-image` probe.
fn image_info(p: &craft_image::ImageInfo) -> ImageInfo {
    let (width, height) = p.header.display_size();
    let m = &p.metadata;
    ImageInfo {
        format: p.header.format.to_string(),
        width,
        height,
        orientation: p.header.orientation.to_exif(),
        bit_depth: p.header.bit_depth,
        has_alpha: p.header.has_alpha,
        animated: p.header.animated,
        color_profile: p.icc.as_ref().and_then(|i| i.description.clone()),
        lens: m.lens.clone(),
        exposure: exposure_summary(m),
        title: m.title.clone(),
        caption: m.caption.clone(),
        alt_text: m.alt_text.clone(),
        creators: m.creators.clone(),
        copyright: m.copyright.clone(),
        decodable: p.decodable,
    }
}

/// "Canon EOS R5" rather than "Canon Canon EOS R5": most makers repeat the make in the model.
fn camera_name(m: &craft_image::Metadata) -> Option<String> {
    let make = m
        .camera_make
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let model = m
        .camera_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (make, model) {
        (Some(make), Some(model)) => {
            // "NIKON CORPORATION" + "NIKON Z 6" → "NIKON Z 6".
            let brand = make
                .split_whitespace()
                .next()
                .unwrap_or(make)
                .to_ascii_lowercase();
            if model.to_ascii_lowercase().starts_with(&brand) {
                Some(model.to_string())
            } else {
                Some(format!("{make} {model}"))
            }
        }
        (None, Some(one)) | (Some(one), None) => Some(one.to_string()),
        (None, None) => None,
    }
}

/// "1/250 s · f/2.8 · ISO 400 · 50 mm", from whichever parts are known.
fn exposure_summary(m: &craft_image::Metadata) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(t) = m.exposure_time.filter(|t| t.is_finite() && *t > 0.0) {
        parts.push(if t < 1.0 {
            format!("1/{} s", (1.0 / t).round())
        } else {
            format!("{} s", (t * 10.0).round() / 10.0)
        });
    }
    if let Some(f) = m.f_number {
        parts.push(format!("f/{}", (f * 10.0).round() / 10.0));
    }
    if let Some(iso) = m.iso {
        parts.push(format!("ISO {iso}"));
    }
    if let Some(fl) = m.focal_length {
        parts.push(format!("{} mm", fl.round()));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn probe_pdf(path: &Path) -> Option<DocumentInfo> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > 64 * 1024 * 1024 {
        return Some(DocumentInfo {
            page_count: 0,
            title: None,
            author: None,
            page_width: None,
            page_height: None,
            is_encrypted: false,
        });
    }
    let bytes = fs::read(path).ok()?;
    let inspector = craft_pdf::PdfInspector::open(&bytes).ok()?;
    let count = inspector.page_count();
    let dims = inspector.dimensions(0);
    let (w, h) = dims
        .map(|d| (Some(d[2]), Some(d[3])))
        .unwrap_or((None, None));
    Some(DocumentInfo {
        page_count: count,
        title: inspector.title(),
        author: inspector.author(),
        page_width: w,
        page_height: h,
        is_encrypted: inspector.is_encrypted(),
    })
}

fn probe_archive(path: &Path) -> Option<ArchiveInfo> {
    let budget = craft_archive::ResourceBudget::default();
    let summary = craft_archive::inspect_archive(path, &budget).ok()?;
    let has_encrypted = summary.entries.iter().any(|e| e.is_encrypted);
    Some(ArchiveInfo {
        format: summary.format.label().to_string(),
        entry_count: summary.entry_count(),
        uncompressed_bytes: summary.total_uncompressed_bytes,
        has_encrypted_entries: has_encrypted,
    })
}

impl MediaCatalog {
    /// Scans a directory and merges newly discovered media assets into the catalog.
    pub fn scan_and_merge(&mut self, dir: &Path, options: &ScanOptions) -> usize {
        let scanned = scan_path(dir, options);
        let mut added = 0;
        for asset in scanned {
            if !self.assets.iter().any(|a| a.path == asset.path) {
                self.assets.push(asset);
                added += 1;
            }
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little-endian EXIF block: Make, Model and Orientation 6 (rotate 90° clockwise).
    fn camera_exif() -> Vec<u8> {
        let mut b = b"II*\0\x08\0\0\0".to_vec();
        b.extend_from_slice(&3u16.to_le_bytes());
        let entry = |b: &mut Vec<u8>, tag: u16, typ: u16, count: u32, value: u32| {
            b.extend_from_slice(&tag.to_le_bytes());
            b.extend_from_slice(&typ.to_le_bytes());
            b.extend_from_slice(&count.to_le_bytes());
            b.extend_from_slice(&value.to_le_bytes());
        };
        // Strings follow the IFD, which ends at 8 + 2 + 3 × 12 + 4 = 50.
        entry(&mut b, 0x010F, 2, 6, 50);
        entry(&mut b, 0x0110, 2, 13, 56);
        entry(&mut b, 0x0112, 3, 1, 6);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(b"Canon\0Canon EOS R5\0");
        b
    }

    #[test]
    fn probe_image_reads_header_and_exif_through_craft_image()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("mc_scan_image_test_{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let pixels = craft_image::Rgba8Image::filled(30, 20, [200, 120, 40, 255])?;
        let opts = craft_image::EncodeOptions {
            exif: Some(camera_exif()),
            ..Default::default()
        };
        let jpeg = craft_image::encode(
            &pixels,
            craft_image::OutputFormat::Jpeg { quality: 80 },
            &opts,
        )?;
        let path = dir.join("portrait.jpg");
        fs::write(&path, jpeg)?;

        let asset = probe_file(&path).ok_or("the JPEG was not indexed")?;
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(asset.kind, MediaKind::Image);
        assert_eq!(asset.camera_model.as_deref(), Some("Canon EOS R5"));
        let image = asset.image.ok_or("no image metadata")?;
        assert_eq!(image.format, "JPEG");
        // Stored 30×20, displayed upright as 20×30.
        assert_eq!((image.width, image.height, image.orientation), (20, 30, 6));
        assert!(image.decodable);
        Ok(())
    }

    #[test]
    fn exposure_and_camera_names_read_naturally() {
        let m = craft_image::Metadata {
            camera_make: Some("NIKON CORPORATION".into()),
            camera_model: Some("NIKON Z 6".into()),
            exposure_time: Some(0.004),
            f_number: Some(2.8),
            iso: Some(400),
            focal_length: Some(50.0),
            ..Default::default()
        };
        assert_eq!(camera_name(&m).as_deref(), Some("NIKON Z 6"));
        assert_eq!(
            exposure_summary(&m).as_deref(),
            Some("1/250 s · f/2.8 · ISO 400 · 50 mm")
        );
        let m = craft_image::Metadata {
            camera_make: Some("FUJIFILM".into()),
            camera_model: Some("X-T5".into()),
            exposure_time: Some(2.0),
            ..Default::default()
        };
        assert_eq!(camera_name(&m).as_deref(), Some("FUJIFILM X-T5"));
        assert_eq!(exposure_summary(&m).as_deref(), Some("2 s"));
        assert_eq!(exposure_summary(&craft_image::Metadata::default()), None);
    }

    #[test]
    fn scan_temp_directory_detects_files() {
        let temp_dir = std::env::temp_dir().join(format!("mc_scan_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let clip1 = temp_dir.join("test_clip.mp4");
        // Write synthetic M4A / MP4
        let dummy =
            mediacraft_bitstream::isobmff::build_synthetic_m4a(&[0x11; 1000], 48000, 2, 1024, true);
        let _ = fs::write(&clip1, dummy);

        let wav1 = temp_dir.join("test_sound.wav");
        let mut wav_data = vec![0u8; 44 + 4800];
        wav_data[0..4].copy_from_slice(b"RIFF");
        wav_data[8..12].copy_from_slice(b"WAVE");
        wav_data[22] = 2; // stereo
        wav_data[24..28].copy_from_slice(&48000u32.to_le_bytes());
        wav_data[34] = 16; // 16-bit
        let _ = fs::write(&wav1, wav_data);

        let mut catalog = MediaCatalog::new();
        let added = catalog.scan_and_merge(&temp_dir, &ScanOptions::default());
        assert_eq!(added, 2);
        assert_eq!(catalog.all_assets().len(), 2);

        // Idempotent rescan
        let added2 = catalog.scan_and_merge(&temp_dir, &ScanOptions::default());
        assert_eq!(added2, 0);

        let _ = fs::remove_file(&clip1);
        let _ = fs::remove_file(&wav1);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn scan_pdf_and_archive_populates_metadata() {
        let temp_dir =
            std::env::temp_dir().join(format!("mc_scan_pdf_zip_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let pdf_path = temp_dir.join("cue_sheet.pdf");
        let pdf_bytes = b"%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
xref\n\
0 4\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
trailer\n<< /Size 4 /Root 1 0 R >>\n\
startxref\n\
190\n\
%%EOF\n";
        let _ = fs::write(&pdf_path, pdf_bytes);

        let zip_path = temp_dir.join("deliverables.zip");
        let zip_res = craft_archive::create_simple_zip(&[
            ("cues.txt", b"01:00:00 Music Start\n01:02:15 Dialog End\n"),
            ("notes.txt", b"Final deliverables for client review\n"),
        ]);
        assert!(zip_res.is_ok());
        if let Ok(zip_bytes) = zip_res {
            let _ = fs::write(&zip_path, zip_bytes);
        }

        let mut catalog = MediaCatalog::new();
        let added = catalog.scan_and_merge(&temp_dir, &ScanOptions::default());
        assert_eq!(added, 2);

        let pdf_asset = catalog
            .all_assets()
            .iter()
            .find(|a| a.filename == "cue_sheet.pdf");
        assert!(pdf_asset.is_some());
        if let Some(pdf_asset) = pdf_asset {
            assert_eq!(pdf_asset.kind, MediaKind::Document);
            assert!(pdf_asset.document.is_some());
            if let Some(doc_info) = &pdf_asset.document {
                assert_eq!(doc_info.page_count, 1);
                assert_eq!(doc_info.page_width, Some(612.0));
                assert_eq!(doc_info.page_height, Some(792.0));
                assert!(!doc_info.is_encrypted);
            }
        }

        let zip_asset = catalog
            .all_assets()
            .iter()
            .find(|a| a.filename == "deliverables.zip");
        assert!(zip_asset.is_some());
        if let Some(zip_asset) = zip_asset {
            assert_eq!(zip_asset.kind, MediaKind::Archive);
            assert!(zip_asset.archive.is_some());
            if let Some(arch_info) = &zip_asset.archive {
                assert_eq!(arch_info.format, "ZIP");
                assert_eq!(arch_info.entry_count, 2);
                assert!(!arch_info.has_encrypted_entries);
                assert!(arch_info.uncompressed_bytes > 0);
            }
        }

        let _ = fs::remove_file(&pdf_path);
        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn scanner_reports_accurate_diagnostics_and_skipped_stats() {
        let temp_dir = std::env::temp_dir().join("mediacraft_diag_test");
        let _ = fs::create_dir_all(&temp_dir);

        // Valid WAV file
        let wav_path = temp_dir.join("tone.wav");
        let mut wav_bytes = vec![0u8; 44];
        wav_bytes[0..4].copy_from_slice(b"RIFF");
        wav_bytes[8..12].copy_from_slice(b"WAVE");
        wav_bytes[22] = 2; // stereo
        wav_bytes[24] = 0x80;
        wav_bytes[25] = 0xbb; // 48000 Hz
        wav_bytes[34] = 16; // 16-bit
        let _ = fs::write(&wav_path, &wav_bytes);

        // Tiny file (too small)
        let tiny_path = temp_dir.join("small.wav");
        let _ = fs::write(&tiny_path, b"tiny");

        // Unsupported extension
        let unsupported_path = temp_dir.join("readme.xyz");
        let _ = fs::write(
            &unsupported_path,
            b"some unsupported content here that is big enough",
        );

        let options = ScanOptions {
            max_depth: 2,
            include_hidden: false,
            min_file_size_bytes: 32,
        };

        let (assets, report) = scan_path_with_report(&temp_dir, &options);
        assert_eq!(assets.len(), 1);
        assert_eq!(report.assets_indexed, 1);
        assert_eq!(report.files_scanned, 3);
        assert_eq!(report.skipped_too_small, 1);
        assert_eq!(report.skipped_unsupported, 1);
        assert!(report.directories_visited >= 1);

        let _ = fs::remove_file(&wav_path);
        let _ = fs::remove_file(&tiny_path);
        let _ = fs::remove_file(&unsupported_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
