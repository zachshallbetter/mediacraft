//! H.265 / HEVC stream diagnostics and NAL unit inspection (ITU-T Rec. H.265).

use serde::{Deserialize, Serialize};

use crate::params::{Pps, Sps};
use crate::slice::{NalHeader, nal_type};
use mediacraft_bitstream::{annexb_nals, length_prefixed_nals, unescape_rbsp};

/// Diagnostic report summarizing the parameter sets, profile/tier/level, and slice structure of an HEVC stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HEVCDiagnosticReport {
    /// Number of Video Parameter Set (VPS) NAL units.
    pub vps_count: usize,
    /// Number of Sequence Parameter Set (SPS) NAL units.
    pub sps_count: usize,
    /// Number of Picture Parameter Set (PPS) NAL units.
    pub pps_count: usize,
    /// Number of Intra Random Access Point (IRAP: IDR, CRA, BLA) keyframe slice NAL units.
    pub irap_count: usize,
    /// Number of trailing / non-IRAP video slice NAL units.
    pub trail_count: usize,
    /// Number of Supplemental Enhancement Information (SEI) NAL units.
    pub sei_count: usize,
    /// Number of Access Unit Delimiter (AUD) NAL units.
    pub aud_count: usize,
    /// Number of other NAL units (EOS, EOB, filler, etc.).
    pub other_nal_count: usize,
    /// Active picture width in pixels after applying conformance window cropping.
    pub width: Option<u32>,
    /// Active picture height in pixels after applying conformance window cropping.
    pub height: Option<u32>,
    /// Profile indicator (1 = Main, 2 = Main 10, 3 = Main Still Picture).
    pub profile_idc: Option<u8>,
    /// True if High tier, false if Main tier.
    pub is_high_tier: Option<bool>,
    /// Level indicator (e.g. 120 = Level 4.0, 150 = Level 5.0, 153 = Level 5.1).
    pub level_idc: Option<u8>,
    /// Luma bit depth (typically 8 or 10).
    pub bit_depth: Option<u32>,
    /// Chroma subsampling indicator (1 = 4:2:0).
    pub chroma_format_idc: Option<u32>,
    /// Non-fatal diagnostic warnings discovered during analysis.
    pub warnings: Vec<String>,
    /// Structural validation errors encountered.
    pub errors: Vec<String>,
}

/// Inspect and diagnose an H.265 / HEVC video stream (Annex B or length-prefixed).
pub fn diagnose_hevc_stream(data: &[u8]) -> HEVCDiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if data.is_empty() {
        errors.push("Empty HEVC stream data".to_string());
        return HEVCDiagnosticReport {
            vps_count: 0,
            sps_count: 0,
            pps_count: 0,
            irap_count: 0,
            trail_count: 0,
            sei_count: 0,
            aud_count: 0,
            other_nal_count: 0,
            width: None,
            height: None,
            profile_idc: None,
            is_high_tier: None,
            level_idc: None,
            bit_depth: None,
            chroma_format_idc: None,
            warnings,
            errors,
        };
    }

    let annex_nals = annexb_nals(data);
    let nals = if !annex_nals.is_empty() {
        annex_nals
    } else {
        match length_prefixed_nals(data, 4) {
            Ok(n) if !n.is_empty() => n,
            _ => {
                errors.push(
                    "Stream contains no valid Annex-B or 4-byte length-prefixed NAL units"
                        .to_string(),
                );
                return HEVCDiagnosticReport {
                    vps_count: 0,
                    sps_count: 0,
                    pps_count: 0,
                    irap_count: 0,
                    trail_count: 0,
                    sei_count: 0,
                    aud_count: 0,
                    other_nal_count: 0,
                    width: None,
                    height: None,
                    profile_idc: None,
                    is_high_tier: None,
                    level_idc: None,
                    bit_depth: None,
                    chroma_format_idc: None,
                    warnings,
                    errors,
                };
            }
        }
    };

    let mut vps_count = 0;
    let mut sps_count = 0;
    let mut pps_count = 0;
    let mut irap_count = 0;
    let mut trail_count = 0;
    let mut sei_count = 0;
    let mut aud_count = 0;
    let mut other_nal_count = 0;

    let mut width = None;
    let mut height = None;
    let mut profile_idc = None;
    let mut is_high_tier = None;
    let mut level_idc = None;
    let mut bit_depth = None;
    let mut chroma_format_idc = None;

    for (nal_idx, nal) in nals.iter().enumerate() {
        if nal.len() < 2 {
            warnings.push(format!("NAL unit {} too short (< 2 bytes)", nal_idx));
            continue;
        }

        let header = match NalHeader::parse(nal) {
            Ok(h) => h,
            Err(e) => {
                warnings.push(format!("NAL unit {} invalid header: {}", nal_idx, e));
                continue;
            }
        };

        match header.nal_type {
            nal_type::VPS => {
                vps_count += 1;
            }
            nal_type::SPS => {
                sps_count += 1;
                let rbsp = unescape_rbsp(&nal[2..]);
                match Sps::parse(&rbsp) {
                    Ok(sps) => {
                        if profile_idc.is_none() {
                            profile_idc = Some(sps.ptl.profile_idc);
                            is_high_tier = Some(sps.ptl.tier);
                            level_idc = Some(sps.ptl.level_idc);
                            bit_depth = Some(sps.bit_depth_luma);
                            chroma_format_idc = Some(sps.chroma_array_type());
                            let (_, _, w, h) = sps.crop_rect();
                            width = Some(w);
                            height = Some(h);
                        }
                    }
                    Err(e) => {
                        errors.push(format!("SPS parse error at NAL {}: {}", nal_idx, e));
                    }
                }
            }
            nal_type::PPS => {
                pps_count += 1;
                let rbsp = unescape_rbsp(&nal[2..]);
                if let Err(e) = Pps::parse(&rbsp) {
                    warnings.push(format!("PPS parse warning at NAL {}: {}", nal_idx, e));
                }
            }
            nal_type::AUD => {
                aud_count += 1;
            }
            nal_type::PREFIX_SEI | nal_type::SUFFIX_SEI => {
                sei_count += 1;
            }
            _ => {
                if header.is_irap() {
                    irap_count += 1;
                } else if header.is_vcl() {
                    trail_count += 1;
                } else {
                    other_nal_count += 1;
                }
            }
        }
    }

    if irap_count == 0 && trail_count == 0 {
        warnings.push("Stream contains no video slice NAL units".to_string());
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
        vps_count,
        sps_count,
        pps_count,
        irap_count,
        trail_count,
        width,
        height,
        "Diagnosed HEVC stream"
    );

    HEVCDiagnosticReport {
        vps_count,
        sps_count,
        pps_count,
        irap_count,
        trail_count,
        sei_count,
        aud_count,
        other_nal_count,
        width,
        height,
        profile_idc,
        is_high_tier,
        level_idc,
        bit_depth,
        chroma_format_idc,
        warnings,
        errors,
    }
}
