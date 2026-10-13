//! H.264 / AVC stream diagnostics and NAL unit inspection (ITU-T Rec. H.264).

use serde::{Deserialize, Serialize};

use crate::params::{Pps, Sps};
use crate::slice::{NalHeader, nal_type};
use mediacraft_bitstream::{annexb_nals, length_prefixed_nals, unescape_rbsp};

/// Diagnostic report summarizing the NAL structure, SPS parameters, and visual stream properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct H264DiagnosticReport {
    /// Number of Sequence Parameter Set (SPS) NAL units encountered.
    pub sps_count: usize,
    /// Number of Picture Parameter Set (PPS) NAL units encountered.
    pub pps_count: usize,
    /// Number of IDR (Instantaneous Decoder Refresh, keyframe) slice NAL units.
    pub idr_count: usize,
    /// Number of non-IDR (P / B) slice NAL units.
    pub non_idr_count: usize,
    /// Number of Supplemental Enhancement Information (SEI) NAL units.
    pub sei_count: usize,
    /// Number of Access Unit Delimiter (AUD) NAL units.
    pub aud_count: usize,
    /// Number of other NAL units (end of sequence, end of stream, filler, etc.).
    pub other_nal_count: usize,
    /// Active picture width in pixels after cropping (derived from SPS).
    pub width: Option<u32>,
    /// Active picture height in pixels after cropping (derived from SPS).
    pub height: Option<u32>,
    /// H.264 profile indicator (e.g. 66 = Baseline, 77 = Main, 100 = High).
    pub profile_idc: Option<u8>,
    /// H.264 level indicator (e.g. 31 = Level 3.1, 40 = Level 4.0, 51 = Level 5.1).
    pub level_idc: Option<u8>,
    /// Luma bit depth (typically 8, up to 14 for High 10/4:2:2/4:4:4).
    pub bit_depth: Option<u32>,
    /// Chroma subsampling indicator (0 = monochrome, 1 = 4:2:0, 2 = 4:2:2, 3 = 4:4:4).
    pub chroma_format_idc: Option<u32>,
    /// True if interlaced coding or field pictures are enabled in the SPS.
    pub is_interlaced: bool,
    /// Non-fatal diagnostic warnings discovered during analysis.
    pub warnings: Vec<String>,
    /// Structural validation errors encountered.
    pub errors: Vec<String>,
}

/// Inspect and diagnose an H.264 / AVC video stream (Annex B or length-prefixed).
pub fn diagnose_h264_stream(data: &[u8]) -> H264DiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if data.is_empty() {
        errors.push("Empty H.264 stream data".to_string());
        return H264DiagnosticReport {
            sps_count: 0,
            pps_count: 0,
            idr_count: 0,
            non_idr_count: 0,
            sei_count: 0,
            aud_count: 0,
            other_nal_count: 0,
            width: None,
            height: None,
            profile_idc: None,
            level_idc: None,
            bit_depth: None,
            chroma_format_idc: None,
            is_interlaced: false,
            warnings,
            errors,
        };
    }

    // Try Annex-B first (0x000001 or 0x00000001 start codes)
    let annex_nals = annexb_nals(data);
    let nals = if !annex_nals.is_empty() {
        annex_nals
    } else {
        // Fallback: try 4-byte length-prefixed format (e.g. MP4 avc1 sample)
        match length_prefixed_nals(data, 4) {
            Ok(n) if !n.is_empty() => n,
            _ => {
                errors.push(
                    "Stream contains no valid Annex-B or 4-byte length-prefixed NAL units"
                        .to_string(),
                );
                return H264DiagnosticReport {
                    sps_count: 0,
                    pps_count: 0,
                    idr_count: 0,
                    non_idr_count: 0,
                    sei_count: 0,
                    aud_count: 0,
                    other_nal_count: 0,
                    width: None,
                    height: None,
                    profile_idc: None,
                    level_idc: None,
                    bit_depth: None,
                    chroma_format_idc: None,
                    is_interlaced: false,
                    warnings,
                    errors,
                };
            }
        }
    };

    let mut sps_count = 0;
    let mut pps_count = 0;
    let mut idr_count = 0;
    let mut non_idr_count = 0;
    let mut sei_count = 0;
    let mut aud_count = 0;
    let mut other_nal_count = 0;

    let mut width = None;
    let mut height = None;
    let mut profile_idc = None;
    let mut level_idc = None;
    let mut bit_depth = None;
    let mut chroma_format_idc = None;
    let mut is_interlaced = false;

    let mut spss: [Option<Sps>; 32] = Default::default();

    for (nal_idx, nal) in nals.iter().enumerate() {
        if nal.is_empty() {
            warnings.push(format!("NAL unit {} has zero length", nal_idx));
            continue;
        }

        let header = match NalHeader::parse(nal[0]) {
            Ok(h) => h,
            Err(e) => {
                warnings.push(format!("NAL unit {} invalid header: {}", nal_idx, e));
                continue;
            }
        };

        match header.nal_unit_type {
            nal_type::SPS => {
                sps_count += 1;
                let rbsp = unescape_rbsp(&nal[1..]);
                match Sps::parse(&rbsp) {
                    Ok(sps) => {
                        if profile_idc.is_none() {
                            profile_idc = Some(sps.profile_idc);
                            level_idc = Some(sps.level_idc);
                            bit_depth = Some(sps.bit_depth_luma);
                            chroma_format_idc = Some(sps.chroma_array_type());
                            let (_, _, w, h) = sps.crop_rect();
                            width = Some(w);
                            height = Some(h);
                            is_interlaced = !sps.frame_mbs_only;
                        }
                        let id = sps.id as usize;
                        if id < spss.len() {
                            spss[id] = Some(sps);
                        }
                    }
                    Err(e) => {
                        errors.push(format!("SPS parse error at NAL {}: {}", nal_idx, e));
                    }
                }
            }
            nal_type::PPS => {
                pps_count += 1;
                let rbsp = unescape_rbsp(&nal[1..]);
                if let Err(e) = Pps::parse(&rbsp, &spss) {
                    warnings.push(format!("PPS parse warning at NAL {}: {}", nal_idx, e));
                }
            }
            nal_type::IDR => {
                idr_count += 1;
            }
            nal_type::SLICE => {
                non_idr_count += 1;
            }
            nal_type::SEI => {
                sei_count += 1;
            }
            nal_type::AUD => {
                aud_count += 1;
            }
            _ => {
                other_nal_count += 1;
            }
        }
    }

    if idr_count == 0 && non_idr_count == 0 {
        warnings.push("Stream contains no video slice NAL units (IDR or non-IDR)".to_string());
    }
    if sps_count == 0 {
        warnings.push(
            "Stream does not contain an inline SPS (requires out-of-band parameter sets)"
                .to_string(),
        );
    }
    if pps_count == 0 {
        warnings.push("Stream does not contain an inline PPS".to_string());
    }

    tracing::debug!(
        sps_count,
        pps_count,
        idr_count,
        non_idr_count,
        width,
        height,
        "Diagnosed H.264 stream"
    );

    H264DiagnosticReport {
        sps_count,
        pps_count,
        idr_count,
        non_idr_count,
        sei_count,
        aud_count,
        other_nal_count,
        width,
        height,
        profile_idc,
        level_idc,
        bit_depth,
        chroma_format_idc,
        is_interlaced,
        warnings,
        errors,
    }
}
