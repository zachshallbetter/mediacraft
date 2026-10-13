//! Diagnostic reports and stream integrity checks for AAC-LC bitstreams and configurations.

use serde::{Deserialize, Serialize};

use crate::config::{AdtsHeader, AudioSpecificConfig};
use crate::tables::SAMPLE_RATES;

/// Diagnostic summary for an AudioSpecificConfig descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AacDiagnosticReport {
    pub object_type: u8,
    pub sample_rate: u32,
    pub channel_config: u8,
    pub channel_count: u8,
    pub frame_length_samples: usize,
    pub sbr_present: bool,
    pub ps_present: bool,
    pub extension_sample_rate: Option<u32>,
    pub warnings: Vec<String>,
}

/// Diagnostic summary for an ADTS bitstream sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdtsStreamDiagnosticReport {
    pub frame_count: usize,
    pub sample_rate: u32,
    pub channel_count: u8,
    pub total_bytes: usize,
    pub has_crc: bool,
    pub average_frame_bytes: f32,
    pub estimated_bitrate_kbps: Option<f32>,
    pub warnings: Vec<String>,
}

impl AudioSpecificConfig {
    /// Generates a diagnostic report evaluating channel layout and compatibility risks.
    pub fn diagnose(&self) -> AacDiagnosticReport {
        let mut warnings = Vec::new();

        if self.object_type != 2 {
            warnings.push(format!(
                "non-AAC-LC audio object type {} detected",
                self.object_type
            ));
        }

        if self.sbr {
            warnings.push(
                "explicit SBR (HE-AAC v1) signalled; only AAC-LC core will be decoded by this engine"
                    .to_string(),
            );
        }

        if self.ps {
            warnings.push(
                "parametric stereo (HE-AAC v2) signalled; only mono AAC-LC core will be decoded"
                    .to_string(),
            );
        }

        if self.frame_length_flag {
            warnings.push(
                "960-sample frame length flag is set (non-standard for broadcast MP4)".to_string(),
            );
        }

        let channel_count = self.channels() as u8;
        if self.channel_config == 0 && self.pce.is_none() {
            warnings.push("channel configuration 0 without ProgramConfigElement".to_string());
        }

        if self.sample_rate < 8000 || self.sample_rate > 96000 {
            warnings.push(format!("unusual core sample rate: {} Hz", self.sample_rate));
        }

        AacDiagnosticReport {
            object_type: self.object_type,
            sample_rate: self.sample_rate,
            channel_config: self.channel_config,
            channel_count,
            frame_length_samples: if self.frame_length_flag { 960 } else { 1024 },
            sbr_present: self.sbr,
            ps_present: self.ps,
            extension_sample_rate: self.extension_sample_rate,
            warnings,
        }
    }
}

/// Inspects an ADTS bitstream buffer and reports structural diagnostics.
pub fn diagnose_adts_stream(mut data: &[u8]) -> AdtsStreamDiagnosticReport {
    let mut warnings = Vec::new();
    let mut frame_count = 0usize;
    let total_bytes = data.len();
    let mut sample_rate = 0u32;
    let mut channel_count = 0u8;
    let mut has_crc = false;
    let mut total_frame_bytes = 0usize;

    while !data.is_empty() {
        if data.len() < 7 {
            warnings.push(format!(
                "stream ends with {} trailing incomplete bytes",
                data.len()
            ));
            break;
        }

        match AdtsHeader::parse(data) {
            Ok(hdr) => {
                if frame_count == 0 {
                    sample_rate = SAMPLE_RATES
                        .get(hdr.sf_index as usize)
                        .copied()
                        .unwrap_or(0);
                    channel_count = hdr.channel_config;
                    has_crc = hdr.header_length == 9;
                } else {
                    let cur_rate = SAMPLE_RATES
                        .get(hdr.sf_index as usize)
                        .copied()
                        .unwrap_or(0);
                    if cur_rate != sample_rate {
                        warnings.push(format!(
                            "sample rate changed from {} to {} Hz at frame {}",
                            sample_rate, cur_rate, frame_count
                        ));
                    }
                    if hdr.channel_config != channel_count {
                        warnings.push(format!(
                            "channel configuration changed from {} to {} at frame {}",
                            channel_count, hdr.channel_config, frame_count
                        ));
                    }
                }

                if hdr.frame_length < hdr.header_length || hdr.frame_length > data.len() {
                    warnings.push(format!(
                        "frame {} declared invalid length {} (available: {})",
                        frame_count,
                        hdr.frame_length,
                        data.len()
                    ));
                    break;
                }

                total_frame_bytes = total_frame_bytes.saturating_add(hdr.frame_length);
                data = &data[hdr.frame_length..];
                frame_count = frame_count.saturating_add(1);
            }
            Err(e) => {
                warnings.push(format!(
                    "sync lost or parse error at frame {}: {}",
                    frame_count, e
                ));
                break;
            }
        }
    }

    let avg_frame = if frame_count > 0 {
        total_frame_bytes as f32 / frame_count as f32
    } else {
        0.0
    };

    let est_bitrate = if frame_count > 0 && sample_rate > 0 {
        // Each AAC frame represents 1024 samples: duration = frame_count * 1024 / sample_rate seconds
        let duration_sec = (frame_count as f64 * 1024.0) / (sample_rate as f64);
        if duration_sec > 0.0 {
            Some(((total_frame_bytes as f64 * 8.0) / duration_sec / 1000.0) as f32)
        } else {
            None
        }
    } else {
        None
    };

    AdtsStreamDiagnosticReport {
        frame_count,
        sample_rate,
        channel_count,
        total_bytes,
        has_crc,
        average_frame_bytes: avg_frame,
        estimated_bitrate_kbps: est_bitrate,
        warnings,
    }
}
