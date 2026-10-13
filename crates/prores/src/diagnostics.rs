//! ProRes frame diagnostics and structural inspection (SMPTE RDD 36).

use serde::{Deserialize, Serialize};

use crate::header::{FRAME_ID, FrameHeader, PictureHeader};
use crate::{AlphaType, ChromaFormat, Error, Interlace, Profile};

/// Diagnostic report detailing the structural integrity and parameters of a ProRes frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProResDiagnosticReport {
    /// True if the frame begins with the 4-byte `'icpf'` atom identifier.
    pub valid_marker: bool,
    /// Total byte length of the frame as declared by the frame container.
    pub frame_size_bytes: usize,
    /// Video picture width in pixels (1..=65535).
    pub width: u32,
    /// Video picture height in pixels (1..=65535).
    pub height: u32,
    /// Chroma subsampling format (4:2:2 or 4:4:4).
    pub chroma_format: Option<String>,
    /// Interlace scan structure (Progressive, TopFieldFirst, BottomFieldFirst).
    pub interlace: Option<String>,
    /// 4-character encoder ID (e.g. `fmcr`, `Lavc`, `apl0`).
    pub encoder_id: Option<String>,
    /// True if an alpha channel is present.
    pub has_alpha: bool,
    /// Bit depth of the alpha channel coding (None, 8-bit, 16-bit).
    pub alpha_type: String,
    /// Number of coded pictures found (1 for progressive, 2 for interlaced).
    pub pictures_count: usize,
    /// Total number of slices across all pictures in the frame.
    pub total_slices: usize,
    /// True if custom luma or chroma quantisation matrices were transmitted.
    pub has_custom_qmat: bool,
    /// Inferred standard ProRes profile based on chroma format and data rate.
    pub inferred_profile: Option<String>,
    /// Non-fatal diagnostic warnings.
    pub warnings: Vec<String>,
    /// Structural validation errors encountered.
    pub errors: Vec<String>,
}

/// Inspect and diagnose a ProRes frame bitstream according to SMPTE RDD 36.
pub fn diagnose_prores_frame(data: &[u8]) -> ProResDiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if data.len() < 8 {
        errors.push("Data too short for ProRes frame header (minimum 8 bytes)".to_string());
        return ProResDiagnosticReport {
            valid_marker: false,
            frame_size_bytes: data.len(),
            width: 0,
            height: 0,
            chroma_format: None,
            interlace: None,
            encoder_id: None,
            has_alpha: false,
            alpha_type: "None".to_string(),
            pictures_count: 0,
            total_slices: 0,
            has_custom_qmat: false,
            inferred_profile: None,
            warnings,
            errors,
        };
    }

    let frame_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let valid_marker = data[4..8] == FRAME_ID;
    if !valid_marker {
        errors.push("Missing 'icpf' frame atom identifier".to_string());
    }

    if frame_size > data.len() {
        errors.push(format!(
            "Declared frame size ({} bytes) exceeds available data ({} bytes)",
            frame_size,
            data.len()
        ));
    } else if frame_size < 28 {
        errors.push(format!(
            "Frame size {} is below minimum valid size of 28",
            frame_size
        ));
    }

    let parsed_header = match FrameHeader::parse(data) {
        Ok((hdr, first_pic_offset, actual_frame_size)) => {
            Some((hdr, first_pic_offset, actual_frame_size))
        }
        Err(e) => {
            errors.push(format!("Failed to parse frame header: {}", e));
            None
        }
    };

    let mut width = 0;
    let mut height = 0;
    let mut chroma_format = None;
    let mut interlace_str = None;
    let mut encoder_id_str = None;
    let mut has_alpha = false;
    let mut alpha_type_str = "None".to_string();
    let mut has_custom_qmat = false;
    let mut pictures_count = 0;
    let mut total_slices = 0;
    let mut inferred_profile = None;

    if let Some((hdr, mut pic_offset, bounded_frame_size)) = parsed_header {
        width = u32::from(hdr.width);
        height = u32::from(hdr.height);

        chroma_format = Some(match hdr.chroma {
            ChromaFormat::Yuv422 => "4:2:2".to_string(),
            ChromaFormat::Yuv444 => "4:4:4".to_string(),
        });

        interlace_str = Some(match hdr.interlace {
            Interlace::Progressive => "Progressive".to_string(),
            Interlace::TopFieldFirst => "TopFieldFirst".to_string(),
            Interlace::BottomFieldFirst => "BottomFieldFirst".to_string(),
        });

        encoder_id_str = Some(String::from_utf8_lossy(&hdr.encoder_id).to_string());
        has_alpha = hdr.alpha != AlphaType::None;
        alpha_type_str = match hdr.alpha {
            AlphaType::None => "None".to_string(),
            AlphaType::Bits8 => "8-bit".to_string(),
            AlphaType::Bits16 => "16-bit".to_string(),
        };

        has_custom_qmat = hdr.luma_matrix_present || hdr.chroma_matrix_present;

        if width == 0 || height == 0 {
            errors.push("Invalid frame dimensions (width or height is 0)".to_string());
        }

        let expected_pictures = match hdr.interlace {
            Interlace::Progressive => 1,
            Interlace::TopFieldFirst | Interlace::BottomFieldFirst => 2,
        };

        let frame_slice = match data.get(..bounded_frame_size) {
            Some(s) => s,
            None => data,
        };

        for p_idx in 0..expected_pictures {
            if pic_offset >= frame_slice.len() {
                warnings.push(format!("Truncated picture {} offset", p_idx));
                break;
            }

            match PictureHeader::parse(&frame_slice[pic_offset..]) {
                Ok(pic) => {
                    pictures_count += 1;
                    total_slices += pic.num_slices;

                    if pic.picture_size > frame_slice.len().saturating_sub(pic_offset) {
                        warnings.push(format!(
                            "Picture {} declared size {} exceeds remaining frame data",
                            p_idx, pic.picture_size
                        ));
                    }

                    pic_offset = pic_offset.saturating_add(pic.picture_size);
                }
                Err(Error::Truncated) => {
                    warnings.push(format!("Truncated picture header for picture {}", p_idx));
                    break;
                }
                Err(e) => {
                    warnings.push(format!("Picture {} parse warning: {}", p_idx, e));
                    break;
                }
            }
        }

        // Infer profile from nominal bit rate
        if width > 0 && height > 0 {
            let mbs = (width.div_ceil(16)) * (height.div_ceil(16));
            if let Some(bits_per_mb) = ((bounded_frame_size * 8) as u32).checked_div(mbs) {
                let closest_profile = Profile::ALL
                    .into_iter()
                    .filter(|p| p.chroma() == hdr.chroma)
                    .min_by_key(|p| {
                        let target = p.nominal_bits_per_mb();
                        (target as i64 - bits_per_mb as i64).abs()
                    });
                if let Some(prof) = closest_profile {
                    inferred_profile = Some(format!("{:?}", prof));
                }
            }
        }
    }

    tracing::debug!(
        valid_marker,
        width,
        height,
        pictures_count,
        total_slices,
        "Diagnosed ProRes frame"
    );

    ProResDiagnosticReport {
        valid_marker,
        frame_size_bytes: frame_size,
        width,
        height,
        chroma_format,
        interlace: interlace_str,
        encoder_id: encoder_id_str,
        has_alpha,
        alpha_type: alpha_type_str,
        pictures_count,
        total_slices,
        has_custom_qmat,
        inferred_profile,
        warnings,
        errors,
    }
}
