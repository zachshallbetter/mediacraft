//! Bitstream and container diagnostic inspection tools.
//!
//! Provides structured validation and metadata reporting for NAL unit byte streams,
//! bit readers/writers, and ISOBMFF container structures.

use crate::annexb_nals;
use crate::isobmff::parse_boxes;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Diagnostic information about an ISOBMFF container structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsobmffDiagnosticReport {
    pub box_count: usize,
    pub top_level_boxes: Vec<String>,
    pub has_ftyp: bool,
    pub has_moov: bool,
    pub has_mdat: bool,
    pub is_faststart: bool,
    pub ftyp_offset: Option<usize>,
    pub moov_offset: Option<usize>,
    pub mdat_offset: Option<usize>,
    pub moov_size: Option<usize>,
    pub mdat_size: Option<usize>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Comprehensive diagnostic report for a bitstream buffer or container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitstreamDiagnosticReport {
    pub total_bytes: usize,
    pub annexb_nal_count: usize,
    pub has_emulation_prevention: bool,
    pub isobmff: Option<IsobmffDiagnosticReport>,
    pub valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Inspect an ISOBMFF container buffer and report top-level boxes and faststart layout.
pub fn diagnose_isobmff_stream(data: &[u8]) -> IsobmffDiagnosticReport {
    debug!(bytes = data.len(), "diagnosing isobmff stream");

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let boxes = match parse_boxes(data) {
        Ok(b) => b,
        Err(err) => {
            warn!(%err, "failed to parse isobmff boxes");
            errors.push(err.to_string());
            return IsobmffDiagnosticReport {
                box_count: 0,
                top_level_boxes: Vec::new(),
                has_ftyp: false,
                has_moov: false,
                has_mdat: false,
                is_faststart: false,
                ftyp_offset: None,
                moov_offset: None,
                mdat_offset: None,
                moov_size: None,
                mdat_size: None,
                warnings,
                errors,
            };
        }
    };

    let mut top_level_boxes = Vec::with_capacity(boxes.len());
    let mut ftyp_offset = None;
    let mut moov_offset = None;
    let mut mdat_offset = None;
    let mut moov_size = None;
    let mut mdat_size = None;

    for b in &boxes {
        top_level_boxes.push(b.fourcc_str());
        if &b.fourcc == b"ftyp" && ftyp_offset.is_none() {
            ftyp_offset = Some(b.start);
        } else if &b.fourcc == b"moov" && moov_offset.is_none() {
            moov_offset = Some(b.start);
            moov_size = Some(b.total_size);
        } else if &b.fourcc == b"mdat" && mdat_offset.is_none() {
            mdat_offset = Some(b.start);
            mdat_size = Some(b.total_size);
        }
    }

    let has_ftyp = ftyp_offset.is_some();
    let has_moov = moov_offset.is_some();
    let has_mdat = mdat_offset.is_some();

    if !has_ftyp {
        warnings.push("container does not start with ftyp box".to_string());
    }
    if !has_moov {
        warnings.push("missing required moov metadata box".to_string());
    }
    if !has_mdat {
        warnings.push("missing mdat media data box".to_string());
    }

    let is_faststart = match (moov_offset, mdat_offset) {
        (Some(moov), Some(mdat)) => moov < mdat,
        _ => false,
    };

    if has_moov && has_mdat && !is_faststart {
        warnings.push("mdat precedes moov; stream is not web faststart optimized".to_string());
    }

    IsobmffDiagnosticReport {
        box_count: boxes.len(),
        top_level_boxes,
        has_ftyp,
        has_moov,
        has_mdat,
        is_faststart,
        ftyp_offset,
        moov_offset,
        mdat_offset,
        moov_size,
        mdat_size,
        warnings,
        errors,
    }
}

/// Inspect a generic bitstream buffer, checking Annex-B NAL delimiters and ISOBMFF structure.
pub fn diagnose_bitstream(data: &[u8]) -> BitstreamDiagnosticReport {
    debug!(bytes = data.len(), "diagnosing bitstream");

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if data.is_empty() {
        errors.push("bitstream buffer is empty".to_string());
        return BitstreamDiagnosticReport {
            total_bytes: 0,
            annexb_nal_count: 0,
            has_emulation_prevention: false,
            isobmff: None,
            valid: false,
            warnings,
            errors,
        };
    }

    // Check for emulation prevention bytes (0x00 0x00 0x03)
    let has_emulation_prevention = data.windows(3).any(|w| w == [0, 0, 3]);

    // Check Annex-B NAL units
    let nals = annexb_nals(data);
    let annexb_nal_count = nals.len();

    // Check if buffer might be an ISOBMFF container (starts with known box size or ftyp)
    let isobmff = if data.len() >= 8 {
        let fourcc = &data[4..8];
        if fourcc == b"ftyp" || fourcc == b"moov" || fourcc == b"mdat" || fourcc == b"free" {
            Some(diagnose_isobmff_stream(data))
        } else {
            None
        }
    } else {
        None
    };

    if annexb_nal_count == 0 && isobmff.is_none() && data.len() >= 4 {
        warnings.push(
            "buffer does not contain Annex-B NAL start codes or ISOBMFF box headers".to_string(),
        );
    }

    let valid = errors.is_empty() && (annexb_nal_count > 0 || isobmff.is_some() || data.len() < 4);

    BitstreamDiagnosticReport {
        total_bytes: data.len(),
        annexb_nal_count,
        has_emulation_prevention,
        isobmff,
        valid,
        warnings,
        errors,
    }
}
