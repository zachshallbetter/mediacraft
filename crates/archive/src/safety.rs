//! Safety and containment policies for untrusted archives (Zip Slip & Decompression Bomb defense).

use crate::{ArchiveError, Result};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Resource budgets for untrusted archives.
#[derive(Debug, Clone)]
pub struct ResourceBudget {
    pub max_entries: u64,
    pub max_total_uncompressed_bytes: u64,
    pub max_single_entry_bytes: u64,
    pub max_path_components: usize,
    pub max_path_bytes: usize,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_total_uncompressed_bytes: 32 * 1024 * 1024 * 1024, // 32 GiB
            max_single_entry_bytes: 8 * 1024 * 1024 * 1024,        // 8 GiB
            max_path_components: 64,
            max_path_bytes: 4096,
        }
    }
}

impl ResourceBudget {
    pub fn check_entry_count(&self, n: u64) -> Result<()> {
        if n > self.max_entries {
            return Err(ArchiveError::BudgetExceeded(format!(
                "entry count {n} exceeds max {}",
                self.max_entries
            )));
        }
        Ok(())
    }

    pub fn check_entry_size(&self, size: u64) -> Result<()> {
        if size > self.max_single_entry_bytes {
            return Err(ArchiveError::BudgetExceeded(format!(
                "entry size {size} exceeds max {}",
                self.max_single_entry_bytes
            )));
        }
        Ok(())
    }

    pub fn check_total(&self, total: u64) -> Result<()> {
        if total > self.max_total_uncompressed_bytes {
            return Err(ArchiveError::BudgetExceeded(format!(
                "total uncompressed {total} exceeds max {}",
                self.max_total_uncompressed_bytes
            )));
        }
        Ok(())
    }
}

/// Copies from `reader` to `writer`, enforcing that no more than `max_bytes` are written.
pub fn copy_bounded<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    max_bytes: u64,
) -> Result<u64> {
    let mut buf = [0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = reader.read(&mut buf).map_err(ArchiveError::Io)?;
        if read == 0 {
            break;
        }
        let next_total = written.saturating_add(read as u64);
        if next_total > max_bytes {
            return Err(ArchiveError::BudgetExceeded(format!(
                "stream exceeded maximum allowed size of {max_bytes} bytes"
            )));
        }
        writer.write_all(&buf[..read]).map_err(ArchiveError::Io)?;
        written = next_total;
    }
    Ok(written)
}

/// Sanitizes an archive entry name, removing leading separators, relative references (`.`, `..`),
/// and backslash confusion on POSIX systems.
pub fn sanitize_entry_name(name: &str) -> Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    let mut cleaned = PathBuf::new();
    for comp in Path::new(&normalized).components() {
        match comp {
            Component::Normal(c) => cleaned.push(c),
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err(ArchiveError::PathEscape(format!(
                    "entry name '{name}' attempts directory traversal ('..')"
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::PathEscape(format!(
                    "entry name '{name}' attempts absolute path breakout"
                )));
            }
        }
    }
    if cleaned.as_os_str().is_empty() {
        return Err(ArchiveError::UnsafeEntry(format!(
            "entry name '{name}' resolves to empty path"
        )));
    }
    Ok(cleaned)
}

/// Validates that joining `rel` to `dest_root` does not escape `dest_root`.
pub fn safe_destination(dest_root: &Path, rel: &str, budget: &ResourceBudget) -> Result<PathBuf> {
    let rel_path = sanitize_entry_name(rel)?;
    let comp_count = rel_path.components().count();
    if comp_count > budget.max_path_components {
        return Err(ArchiveError::BudgetExceeded(format!(
            "path components {comp_count} exceeds limit {}",
            budget.max_path_components
        )));
    }
    let target = dest_root.join(&rel_path);
    assert_tree_contained(dest_root, &target)?;
    Ok(target)
}

/// Asserts that `candidate` is strictly contained within `root`.
pub fn assert_tree_contained(root: &Path, candidate: &Path) -> Result<()> {
    if !candidate.starts_with(root) {
        return Err(ArchiveError::PathEscape(format!(
            "path '{}' escapes target root '{}'",
            candidate.display(),
            root.display()
        )));
    }
    Ok(())
}
