//! Magic byte and file extension format detection for archives.

use crate::{ArchiveError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    GzipSingle,
}

impl ArchiveFormat {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::GzipSingle => "GZIP",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::GzipSingle => "gz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ZipKind {
    Generic,
    OfficeOpenXml,
    OpenDocument,
    Epub,
    Usdz,
    ThreeMf,
    ComicCbz,
}

pub fn zip_kind_for_path(path: &Path) -> ZipKind {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if name.ends_with(".docx")
        || name.ends_with(".xlsx")
        || name.ends_with(".pptx")
        || name.ends_with(".dotx")
        || name.ends_with(".xltx")
    {
        ZipKind::OfficeOpenXml
    } else if name.ends_with(".odt") || name.ends_with(".ods") || name.ends_with(".odp") {
        ZipKind::OpenDocument
    } else if name.ends_with(".epub") {
        ZipKind::Epub
    } else if name.ends_with(".usdz") {
        ZipKind::Usdz
    } else if name.ends_with(".3mf") {
        ZipKind::ThreeMf
    } else if name.ends_with(".cbz") {
        ZipKind::ComicCbz
    } else {
        ZipKind::Generic
    }
}

pub fn detect_bytes(bytes: &[u8]) -> Result<ArchiveFormat> {
    if bytes.len() >= 4 && &bytes[0..4] == b"PK\x03\x04" {
        return Ok(ArchiveFormat::Zip);
    }
    if bytes.len() >= 2 && &bytes[0..2] == b"\x1f\x8b" {
        // Gzip - could be tar.gz or single stream
        return Ok(ArchiveFormat::TarGz);
    }
    if bytes.len() >= 512 && is_tar_header(&bytes[0..512]) {
        return Ok(ArchiveFormat::Tar);
    }
    Err(ArchiveError::Unsupported(
        "unrecognized archive signature".to_string(),
    ))
}

pub fn detect_format(path: &Path) -> Result<ArchiveFormat> {
    let mut file = File::open(path).map_err(ArchiveError::Io)?;
    let mut magic = [0u8; 512];
    let n = file.read(&mut magic).map_err(ArchiveError::Io)?;
    if n >= 4 && &magic[0..4] == b"PK\x03\x04" {
        return Ok(ArchiveFormat::Zip);
    }
    if n >= 2 && &magic[0..2] == b"\x1f\x8b" {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Ok(ArchiveFormat::TarGz);
        }
        return Ok(ArchiveFormat::TarGz);
    }
    if n >= 512 && is_tar_header(&magic) {
        return Ok(ArchiveFormat::Tar);
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".tar") {
        return Ok(ArchiveFormat::Tar);
    }
    Err(ArchiveError::Unsupported(format!(
        "unknown format for path: {}",
        path.display()
    )))
}

fn is_tar_header(block: &[u8]) -> bool {
    if block.len() < 512 {
        return false;
    }
    // Check "ustar" magic at offset 257..262
    if &block[257..262] == b"ustar" {
        return true;
    }
    // Alternatively verify checksum at offset 148..156
    let chk_slice = &block[148..156];
    if chk_slice.iter().all(|&b| b == 0) {
        return false;
    }
    true
}
