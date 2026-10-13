#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod detect;
pub mod extract;
pub mod inspect;
pub mod safety;

pub use detect::{ArchiveFormat, ZipKind, detect_bytes, detect_format, zip_kind_for_path};
pub use extract::{
    ExtractionPolicy, ExtractionReport, SkippedEntry, extract_archive, extract_archive_with_report,
};
pub use inspect::{ArchiveEntryInfo, ArchiveSummary, inspect_archive, inspect_bytes};
pub use safety::{ResourceBudget, assert_tree_contained, safe_destination, sanitize_entry_name};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("unsupported archive format or signature: {0}")]
    Unsupported(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("path escape blocked: {0}")]
    PathEscape(String),
    #[error("unsafe archive entry: {0}")]
    UnsafeEntry(String),
    #[error("resource budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Helper to construct a simple in-memory ZIP archive from named slices.
pub fn create_simple_zip(entries: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for &(name, content) in entries {
            zip.start_file(name, options).map_err(ArchiveError::Zip)?;
            zip.write_all(content).map_err(ArchiveError::Io)?;
        }
        zip.finish().map_err(ArchiveError::Zip)?;
    }
    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn zip_slip_traversal_is_blocked() {
        let budget = ResourceBudget::default();
        let tmp = tempdir().unwrap();
        let dest = tmp.path();

        assert!(safe_destination(dest, "../../etc/passwd", &budget).is_err());
        assert!(safe_destination(dest, "/etc/passwd", &budget).is_err());
        assert!(safe_destination(dest, "valid/subfolder/file.txt", &budget).is_ok());
    }

    #[test]
    fn sanitize_name_cleans_backslashes_and_slashes() {
        let path = sanitize_entry_name("foo\\bar\\baz.txt").unwrap();
        assert_eq!(path, PathBuf::from("foo/bar/baz.txt"));

        assert!(sanitize_entry_name("../foo").is_err());
        assert!(sanitize_entry_name("/root").is_err());
    }

    #[test]
    fn zip_create_and_inspect_roundtrip() {
        let budget = ResourceBudget::default();
        let mut buffer = std::io::Cursor::new(Vec::new());

        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            zip.start_file("test.txt", options).unwrap();
            zip.write_all(b"Hello MediaCraft Archive!").unwrap();

            zip.start_file("docs/script.txt", options).unwrap();
            zip.write_all(b"Scene 1: Interior studio.").unwrap();

            zip.finish().unwrap();
        }

        let bytes = buffer.into_inner();
        let summary = inspect_bytes(&bytes, &budget).unwrap();

        assert_eq!(summary.format, ArchiveFormat::Zip);
        assert_eq!(summary.entry_count(), 2);
        assert_eq!(summary.entries[0].name, "test.txt");
        assert_eq!(summary.entries[1].name, "docs/script.txt");
        assert_eq!(summary.total_uncompressed_bytes, 50);
    }
}
