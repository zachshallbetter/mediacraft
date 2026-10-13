//! Safe extraction of archive files into a contained destination directory.

use crate::detect::{ArchiveFormat, detect_format};
use crate::safety::{ResourceBudget, copy_bounded, safe_destination};
use crate::{ArchiveError, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive as TarArchive;
use zip::ZipArchive;

/// Policy controlling how extraction handles suspicious or corrupted entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtractionPolicy {
    /// Immediately abort on the first unsafe entry, traversal attempt, or budget breach.
    #[default]
    Strict,
    /// Skip suspicious or unsafe entries with warnings recorded in the report, continuing with remaining valid files.
    Permissive,
}

/// Information about an entry skipped during extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedEntry {
    pub raw_name: String,
    pub reason: String,
}

/// Comprehensive report produced during safe extraction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionReport {
    pub format: String,
    pub extracted_paths: Vec<PathBuf>,
    pub skipped_entries: Vec<SkippedEntry>,
    pub total_uncompressed_bytes: u64,
    pub total_entries_processed: u64,
    pub warnings: Vec<String>,
}

/// Safely extracts an archive to `dest_dir`, enforcing path containment and resource limits with default strict policy.
pub fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    budget: &ResourceBudget,
) -> Result<Vec<PathBuf>> {
    let report =
        extract_archive_with_report(archive_path, dest_dir, budget, ExtractionPolicy::Strict)?;
    Ok(report.extracted_paths)
}

/// Safely extracts an archive to `dest_dir`, producing an exhaustive execution and diagnostic report.
pub fn extract_archive_with_report(
    archive_path: &Path,
    dest_dir: &Path,
    budget: &ResourceBudget,
    policy: ExtractionPolicy,
) -> Result<ExtractionReport> {
    tracing::info!(
        path = %archive_path.display(),
        destination = %dest_dir.display(),
        ?policy,
        "beginning safe archive extraction"
    );

    fs::create_dir_all(dest_dir).map_err(ArchiveError::Io)?;
    let canonical_root = dest_dir.canonicalize().map_err(ArchiveError::Io)?;

    let format = detect_format(archive_path)?;
    let mut report = ExtractionReport {
        format: format.as_str().to_string(),
        ..Default::default()
    };

    match format {
        ArchiveFormat::Zip => {
            extract_zip_into(archive_path, &canonical_root, budget, policy, &mut report)?
        }
        ArchiveFormat::Tar => {
            let file = File::open(archive_path).map_err(ArchiveError::Io)?;
            extract_tar_into(file, &canonical_root, budget, policy, &mut report)?;
        }
        ArchiveFormat::TarGz => {
            let file = File::open(archive_path).map_err(ArchiveError::Io)?;
            let gz = GzDecoder::new(file);
            extract_tar_into(gz, &canonical_root, budget, policy, &mut report)?;
        }
        ArchiveFormat::GzipSingle => {
            let mut file = File::open(archive_path).map_err(ArchiveError::Io)?;
            let mut gz = GzDecoder::new(&mut file);
            let out_name = archive_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("extracted_file");
            let target = safe_destination(&canonical_root, out_name, budget)?;
            let mut out = File::create(&target).map_err(ArchiveError::Io)?;
            let written = copy_bounded(&mut gz, &mut out, budget.max_single_entry_bytes)?;
            report.total_uncompressed_bytes = written;
            report.total_entries_processed = 1;
            report.extracted_paths.push(target);
        }
    }

    tracing::info!(
        extracted = report.extracted_paths.len(),
        skipped = report.skipped_entries.len(),
        bytes = report.total_uncompressed_bytes,
        "archive extraction complete"
    );

    Ok(report)
}

fn extract_zip_into(
    archive_path: &Path,
    dest_root: &Path,
    budget: &ResourceBudget,
    policy: ExtractionPolicy,
    report: &mut ExtractionReport,
) -> Result<()> {
    let file = File::open(archive_path).map_err(ArchiveError::Io)?;
    let mut zip = ZipArchive::new(file).map_err(ArchiveError::Zip)?;
    budget.check_entry_count(zip.len() as u64)?;

    for i in 0..zip.len() {
        report.total_entries_processed = report.total_entries_processed.saturating_add(1);
        let mut entry = zip.by_index(i).map_err(ArchiveError::Zip)?;
        let raw_name = entry.name().to_string();

        let target = match safe_destination(dest_root, &raw_name, budget) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(entry = %raw_name, error = %e, "unsafe or escaping zip entry");
                if policy == ExtractionPolicy::Strict {
                    return Err(e);
                }
                report.skipped_entries.push(SkippedEntry {
                    raw_name: raw_name.clone(),
                    reason: e.to_string(),
                });
                report
                    .warnings
                    .push(format!("Skipped unsafe entry '{raw_name}': {e}"));
                continue;
            }
        };

        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(ArchiveError::Io)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(ArchiveError::Io)?;
        }

        let mut out = File::create(&target).map_err(ArchiveError::Io)?;
        let written = match copy_bounded(&mut entry, &mut out, budget.max_single_entry_bytes) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(entry = %raw_name, error = %e, "entry exceeded single-file budget");
                let _ = fs::remove_file(&target);
                if policy == ExtractionPolicy::Strict {
                    return Err(e);
                }
                report.skipped_entries.push(SkippedEntry {
                    raw_name: raw_name.clone(),
                    reason: e.to_string(),
                });
                report
                    .warnings
                    .push(format!("Skipped oversized entry '{raw_name}': {e}"));
                continue;
            }
        };

        report.total_uncompressed_bytes = report.total_uncompressed_bytes.saturating_add(written);
        if let Err(e) = budget.check_total(report.total_uncompressed_bytes) {
            tracing::error!(
                total_bytes = report.total_uncompressed_bytes,
                "aggregate budget breached"
            );
            return Err(e);
        }

        report.extracted_paths.push(target);
    }

    Ok(())
}

fn extract_tar_into<R: Read>(
    reader: R,
    dest_root: &Path,
    budget: &ResourceBudget,
    policy: ExtractionPolicy,
    report: &mut ExtractionReport,
) -> Result<()> {
    let mut archive = TarArchive::new(reader);

    for entry_res in archive.entries().map_err(ArchiveError::Io)? {
        report.total_entries_processed = report.total_entries_processed.saturating_add(1);
        budget.check_entry_count(report.total_entries_processed)?;

        let mut entry = match entry_res {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "corrupted tar entry header");
                if policy == ExtractionPolicy::Strict {
                    return Err(ArchiveError::Io(e));
                }
                report.warnings.push(format!("Corrupt tar header: {e}"));
                continue;
            }
        };

        let path = match entry.path() {
            Ok(p) => p,
            Err(e) => {
                if policy == ExtractionPolicy::Strict {
                    return Err(ArchiveError::Io(e));
                }
                report.warnings.push(format!("Invalid tar entry path: {e}"));
                continue;
            }
        };

        let path_str = path.to_string_lossy().to_string();
        let target = match safe_destination(dest_root, &path_str, budget) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(entry = %path_str, error = %e, "unsafe or escaping tar entry");
                if policy == ExtractionPolicy::Strict {
                    return Err(e);
                }
                report.skipped_entries.push(SkippedEntry {
                    raw_name: path_str.clone(),
                    reason: e.to_string(),
                });
                report
                    .warnings
                    .push(format!("Skipped unsafe tar entry '{path_str}': {e}"));
                continue;
            }
        };

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).map_err(ArchiveError::Io)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(ArchiveError::Io)?;
        }

        let mut out = File::create(&target).map_err(ArchiveError::Io)?;
        let written = match copy_bounded(&mut entry, &mut out, budget.max_single_entry_bytes) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(entry = %path_str, error = %e, "tar entry exceeded single-file budget");
                let _ = fs::remove_file(&target);
                if policy == ExtractionPolicy::Strict {
                    return Err(e);
                }
                report.skipped_entries.push(SkippedEntry {
                    raw_name: path_str.clone(),
                    reason: e.to_string(),
                });
                report
                    .warnings
                    .push(format!("Skipped oversized tar entry '{path_str}': {e}"));
                continue;
            }
        };

        report.total_uncompressed_bytes = report.total_uncompressed_bytes.saturating_add(written);
        if let Err(e) = budget.check_total(report.total_uncompressed_bytes) {
            tracing::error!(
                total_bytes = report.total_uncompressed_bytes,
                "aggregate budget breached"
            );
            return Err(e);
        }

        report.extracted_paths.push(target);
    }

    Ok(())
}
