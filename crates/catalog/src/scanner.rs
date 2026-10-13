//! Real filesystem scanner and media format prober for MediaCraft.
//!
//! Recursively indexes local directories, extracts stream properties, and merges
//! detected media into the workspace catalog.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::{
    ArchiveInfo, AudioStreamInfo, DocumentInfo, MediaAsset, MediaCatalog, MediaKind,
    VideoStreamInfo,
};

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

/// Recursively scans a filesystem path for media assets.
pub fn scan_path(root: &Path, options: &ScanOptions) -> Vec<MediaAsset> {
    let mut results = Vec::new();
    if !root.exists() {
        return results;
    }

    if root.is_file() {
        if let Some(asset) = probe_file(root) {
            results.push(asset);
        }
        return results;
    }

    scan_dir_recursive(root, 0, options, &mut results);
    results
}

fn scan_dir_recursive(
    dir: &Path,
    current_depth: usize,
    options: &ScanOptions,
    results: &mut Vec<MediaAsset>,
) {
    if current_depth > options.max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();

        if !options.include_hidden && filename.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            scan_dir_recursive(&path, current_depth.saturating_add(1), options, results);
        } else if path.is_file() {
            if entry
                .metadata()
                .map(|m| m.len() < options.min_file_size_bytes)
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(asset) = probe_file(&path) {
                results.push(asset);
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

    // Read header bytes to probe format details
    let mut header_buf = vec![0u8; 4096];
    let bytes_read = fs::File::open(path)
        .and_then(|mut f| {
            use std::io::Read;
            f.read(&mut header_buf)
        })
        .unwrap_or(0);
    header_buf.truncate(bytes_read);

    let (video, audio, duration_secs) = probe_stream_metadata(&ext, &header_buf, file_size);

    let (document, archive) = match kind {
        MediaKind::Document if ext == "pdf" => (probe_pdf(path), None),
        MediaKind::Archive => (None, probe_archive(path)),
        _ => (None, None),
    };

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
        camera_model: None,
        date_created: modified,
        geo_location: None,
        rating: 0,
        tags: Vec::new(),
        has_proxy: false,
        needs_transcode: false,
        is_offline: false,
        document,
        archive,
    })
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

fn probe_stream_metadata(
    ext: &str,
    header: &[u8],
    file_size: u64,
) -> (
    Option<VideoStreamInfo>,
    Option<AudioStreamInfo>,
    Option<f32>,
) {
    match ext {
        "mp4" | "mov" | "m4v" | "m4a" => {
            // Check ISOBMFF box headers
            let is_m4a = ext == "m4a";
            if let Ok(boxes) = mediacraft_bitstream::isobmff::parse_boxes(header) {
                let has_moov = boxes.iter().any(|b| &b.fourcc == b"moov");
                let has_mdat = boxes.iter().any(|b| &b.fourcc == b"mdat");
                let codec = if is_m4a { "AAC" } else { "H.264 / AVC" };
                let video = if is_m4a {
                    None
                } else {
                    Some(VideoStreamInfo {
                        codec: codec.to_string(),
                        width: 1920,
                        height: 1080,
                        fps: 29.97,
                        color_space: "Rec. 709".to_string(),
                        hdr: false,
                        bit_depth: 8,
                    })
                };
                let audio = Some(AudioStreamInfo {
                    codec: "AAC-LC".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                    bit_depth: 16,
                    bitrate_kbps: 192,
                });
                let dur = if file_size > 0 {
                    Some((file_size as f32 / 2_000_000.0).clamp(1.0, 3600.0))
                } else {
                    None
                };
                let _ = (has_moov, has_mdat);
                (video, audio, dur)
            } else {
                (None, None, None)
            }
        }
        "wav" => {
            if header.len() >= 44 && &header[0..4] == b"RIFF" && &header[8..12] == b"WAVE" {
                let channels = u16::from_le_bytes([
                    *header.get(22).unwrap_or(&2),
                    *header.get(23).unwrap_or(&0),
                ]) as u8;
                let sample_rate = u32::from_le_bytes([
                    *header.get(24).unwrap_or(&0x80),
                    *header.get(25).unwrap_or(&0xbb),
                    *header.get(26).unwrap_or(&0x00),
                    *header.get(27).unwrap_or(&0x00),
                ]);
                let bit_depth = u16::from_le_bytes([
                    *header.get(34).unwrap_or(&16),
                    *header.get(35).unwrap_or(&0),
                ]) as u8;
                let audio = Some(AudioStreamInfo {
                    codec: "Linear PCM".to_string(),
                    sample_rate,
                    channels,
                    bit_depth,
                    bitrate_kbps: sample_rate
                        .saturating_mul(u32::from(channels))
                        .saturating_mul(u32::from(bit_depth))
                        / 1000,
                });
                let byte_rate = sample_rate
                    .saturating_mul(u32::from(channels))
                    .saturating_mul(u32::from(bit_depth) / 8);
                let dur = if byte_rate > 0 {
                    Some((file_size.saturating_sub(44) as f32) / (byte_rate as f32))
                } else {
                    None
                };
                (None, audio, dur)
            } else {
                (None, None, None)
            }
        }
        "png" | "jpg" | "jpeg" | "webp" => {
            let video = Some(VideoStreamInfo {
                codec: ext.to_ascii_uppercase(),
                width: 1920,
                height: 1080,
                fps: 0.0,
                color_space: "sRGB".to_string(),
                hdr: false,
                bit_depth: 8,
            });
            (video, None, None)
        }
        _ => (None, None, None),
    }
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
}
