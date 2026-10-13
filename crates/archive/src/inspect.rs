//! High-level archive inspection without full payload decompression.

use crate::detect::{ArchiveFormat, ZipKind, detect_bytes, detect_format, zip_kind_for_path};
use crate::safety::{ResourceBudget, sanitize_entry_name};
use crate::{ArchiveError, Result};
use flate2::read::GzDecoder;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use tar::Archive as TarArchive;
use zip::ZipArchive;

/// Metadata for a single entry within an archive.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub compressed_size: Option<u64>,
    pub is_encrypted: bool,
}

/// Overall summary of an archive container.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveSummary {
    pub format: ArchiveFormat,
    pub zip_kind: Option<ZipKind>,
    pub entries: Vec<ArchiveEntryInfo>,
    pub total_uncompressed_bytes: u64,
}

impl ArchiveSummary {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// Inspects an archive on disk using the provided resource budget.
pub fn inspect_archive(path: &Path, budget: &ResourceBudget) -> Result<ArchiveSummary> {
    let format = detect_format(path)?;
    let zip_kind = if format == ArchiveFormat::Zip {
        Some(zip_kind_for_path(path))
    } else {
        None
    };

    match format {
        ArchiveFormat::Zip => {
            let file = File::open(path).map_err(ArchiveError::Io)?;
            inspect_zip_reader(file, budget, zip_kind)
        }
        ArchiveFormat::Tar => {
            let file = File::open(path).map_err(ArchiveError::Io)?;
            inspect_tar_reader(file, budget, ArchiveFormat::Tar)
        }
        ArchiveFormat::TarGz => {
            let file = File::open(path).map_err(ArchiveError::Io)?;
            let gz = GzDecoder::new(file);
            inspect_tar_reader(gz, budget, ArchiveFormat::TarGz)
        }
        ArchiveFormat::GzipSingle => {
            let _file = File::open(path).map_err(ArchiveError::Io)?;
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            Ok(ArchiveSummary {
                format: ArchiveFormat::GzipSingle,
                zip_kind: None,
                entries: vec![ArchiveEntryInfo {
                    name: stem.to_string(),
                    is_dir: false,
                    size: None,
                    compressed_size: None,
                    is_encrypted: false,
                }],
                total_uncompressed_bytes: 0,
            })
        }
    }
}

/// Inspects an archive from memory.
pub fn inspect_bytes(bytes: &[u8], budget: &ResourceBudget) -> Result<ArchiveSummary> {
    let format = detect_bytes(bytes)?;
    match format {
        ArchiveFormat::Zip => {
            let cursor = Cursor::new(bytes);
            inspect_zip_reader(cursor, budget, Some(ZipKind::Generic))
        }
        ArchiveFormat::Tar => {
            let cursor = Cursor::new(bytes);
            inspect_tar_reader(cursor, budget, ArchiveFormat::Tar)
        }
        ArchiveFormat::TarGz => {
            let cursor = Cursor::new(bytes);
            let gz = GzDecoder::new(cursor);
            inspect_tar_reader(gz, budget, ArchiveFormat::TarGz)
        }
        ArchiveFormat::GzipSingle => Ok(ArchiveSummary {
            format: ArchiveFormat::GzipSingle,
            zip_kind: None,
            entries: vec![ArchiveEntryInfo {
                name: "stream".to_string(),
                is_dir: false,
                size: None,
                compressed_size: None,
                is_encrypted: false,
            }],
            total_uncompressed_bytes: 0,
        }),
    }
}

fn inspect_zip_reader<R: std::io::Read + std::io::Seek>(
    reader: R,
    budget: &ResourceBudget,
    zip_kind: Option<ZipKind>,
) -> Result<ArchiveSummary> {
    let mut zip = ZipArchive::new(reader).map_err(ArchiveError::Zip)?;
    let count = zip.len() as u64;
    budget.check_entry_count(count)?;

    let mut entries = Vec::with_capacity(zip.len().min(1024));
    let mut total_uncompressed = 0u64;

    for i in 0..zip.len() {
        let file = zip.by_index_raw(i).map_err(ArchiveError::Zip)?;
        let raw_name = file.name();
        let sanitized = sanitize_entry_name(raw_name)?;
        let size = file.size();
        budget.check_entry_size(size)?;

        total_uncompressed = total_uncompressed.saturating_add(size);
        budget.check_total(total_uncompressed)?;

        entries.push(ArchiveEntryInfo {
            name: sanitized.to_string_lossy().to_string(),
            is_dir: file.is_dir(),
            size: Some(size),
            compressed_size: Some(file.compressed_size()),
            is_encrypted: file.encrypted(),
        });
    }

    Ok(ArchiveSummary {
        format: ArchiveFormat::Zip,
        zip_kind,
        entries,
        total_uncompressed_bytes: total_uncompressed,
    })
}

fn inspect_tar_reader<R: Read>(
    reader: R,
    budget: &ResourceBudget,
    format: ArchiveFormat,
) -> Result<ArchiveSummary> {
    let mut archive = TarArchive::new(reader);
    let mut entries = Vec::new();
    let mut total_uncompressed = 0u64;
    let mut count = 0u64;

    for entry_res in archive.entries().map_err(ArchiveError::Io)? {
        let entry = entry_res.map_err(ArchiveError::Io)?;
        count = count.saturating_add(1);
        budget.check_entry_count(count)?;

        let path = entry.path().map_err(ArchiveError::Io)?;
        let sanitized = sanitize_entry_name(&path.to_string_lossy())?;
        let size = entry.size();
        budget.check_entry_size(size)?;

        total_uncompressed = total_uncompressed.saturating_add(size);
        budget.check_total(total_uncompressed)?;

        entries.push(ArchiveEntryInfo {
            name: sanitized.to_string_lossy().to_string(),
            is_dir: entry.header().entry_type().is_dir(),
            size: Some(size),
            compressed_size: None,
            is_encrypted: false,
        });
    }

    Ok(ArchiveSummary {
        format,
        zip_kind: None,
        entries,
        total_uncompressed_bytes: total_uncompressed,
    })
}
