//! Safe extraction of archive files into a contained destination directory.

use crate::detect::{ArchiveFormat, detect_format};
use crate::safety::{ResourceBudget, copy_bounded, safe_destination};
use crate::{ArchiveError, Result};
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive as TarArchive;
use zip::ZipArchive;

/// Safely extracts an archive to `dest_dir`, enforcing path containment and resource limits.
pub fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    budget: &ResourceBudget,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dest_dir).map_err(ArchiveError::Io)?;
    let canonical_root = dest_dir.canonicalize().map_err(ArchiveError::Io)?;

    let format = detect_format(archive_path)?;
    match format {
        ArchiveFormat::Zip => extract_zip(archive_path, &canonical_root, budget),
        ArchiveFormat::Tar => {
            let file = File::open(archive_path).map_err(ArchiveError::Io)?;
            extract_tar(file, &canonical_root, budget)
        }
        ArchiveFormat::TarGz => {
            let file = File::open(archive_path).map_err(ArchiveError::Io)?;
            let gz = GzDecoder::new(file);
            extract_tar(gz, &canonical_root, budget)
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
            copy_bounded(&mut gz, &mut out, budget.max_single_entry_bytes)?;
            Ok(vec![target])
        }
    }
}

fn extract_zip(
    archive_path: &Path,
    dest_root: &Path,
    budget: &ResourceBudget,
) -> Result<Vec<PathBuf>> {
    let file = File::open(archive_path).map_err(ArchiveError::Io)?;
    let mut zip = ZipArchive::new(file).map_err(ArchiveError::Zip)?;
    budget.check_entry_count(zip.len() as u64)?;

    let mut extracted = Vec::new();
    let mut total_bytes = 0u64;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(ArchiveError::Zip)?;
        let raw_name = entry.name().to_string();
        let target = safe_destination(dest_root, &raw_name, budget)?;

        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(ArchiveError::Io)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(ArchiveError::Io)?;
        }

        let mut out = File::create(&target).map_err(ArchiveError::Io)?;
        let written = copy_bounded(&mut entry, &mut out, budget.max_single_entry_bytes)?;
        total_bytes = total_bytes.saturating_add(written);
        budget.check_total(total_bytes)?;

        extracted.push(target);
    }

    Ok(extracted)
}

fn extract_tar<R: Read>(
    reader: R,
    dest_root: &Path,
    budget: &ResourceBudget,
) -> Result<Vec<PathBuf>> {
    let mut archive = TarArchive::new(reader);
    let mut extracted = Vec::new();
    let mut total_bytes = 0u64;
    let mut count = 0u64;

    for entry_res in archive.entries().map_err(ArchiveError::Io)? {
        let mut entry = entry_res.map_err(ArchiveError::Io)?;
        count = count.saturating_add(1);
        budget.check_entry_count(count)?;

        let path = entry.path().map_err(ArchiveError::Io)?;
        let path_str = path.to_string_lossy();
        let target = safe_destination(dest_root, &path_str, budget)?;

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).map_err(ArchiveError::Io)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(ArchiveError::Io)?;
        }

        let mut out = File::create(&target).map_err(ArchiveError::Io)?;
        let written = copy_bounded(&mut entry, &mut out, budget.max_single_entry_bytes)?;
        total_bytes = total_bytes.saturating_add(written);
        budget.check_total(total_bytes)?;

        extracted.push(target);
    }

    Ok(extracted)
}
