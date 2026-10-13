//! H.264 / AVC encoder output diagnostics and timeline validation.

use serde::{Deserialize, Serialize};

use crate::{EncoderConfig, FrameType, Packet};

/// Diagnostic report verifying encoder packet sequences, timing causality, and bitstream statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct H264EncoderDiagnosticReport {
    /// Coded video width in pixels.
    pub width: u32,
    /// Coded video height in pixels.
    pub height: u32,
    /// Encoding profile name (Baseline, Main, High).
    pub profile: String,
    /// Preset speed name.
    pub preset: String,
    /// Maximum keyframe interval.
    pub keyint: u32,
    /// Number of B-frames configured.
    pub bframes: u8,
    /// Total packets produced.
    pub packets_generated: usize,
    /// Number of IDR keyframe packets.
    pub idr_packets: usize,
    /// Number of P-frame packets.
    pub p_packets: usize,
    /// Number of B-frame packets.
    pub b_packets: usize,
    /// Average slice QP across all packets.
    pub avg_qp: f32,
    /// Total coded byte volume.
    pub total_bytes: usize,
    /// Estimated stream bitrate in kbps based on configured framerate.
    pub estimated_bitrate_kbps: Option<f64>,
    /// Non-fatal diagnostic warnings.
    pub warnings: Vec<String>,
    /// Structural validation errors encountered.
    pub errors: Vec<String>,
}

/// Inspect and validate a sequence of packets emitted by the H.264 encoder.
pub fn diagnose_encoder_output(
    cfg: &EncoderConfig,
    packets: &[Packet],
) -> H264EncoderDiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if packets.is_empty() {
        warnings.push("No packets emitted by encoder".to_string());
        return H264EncoderDiagnosticReport {
            width: cfg.width,
            height: cfg.height,
            profile: format!("{:?}", cfg.profile),
            preset: format!("{:?}", cfg.preset),
            keyint: cfg.keyint,
            bframes: cfg.bframes,
            packets_generated: 0,
            idr_packets: 0,
            p_packets: 0,
            b_packets: 0,
            avg_qp: 0.0,
            total_bytes: 0,
            estimated_bitrate_kbps: None,
            warnings,
            errors,
        };
    }

    let mut idr_packets = 0;
    let mut p_packets = 0;
    let mut b_packets = 0;
    let mut total_qp: u64 = 0;
    let mut total_bytes = 0;

    // Check first packet is an IDR keyframe
    if let Some(first) = packets.first() {
        if !first.keyframe || first.frame_type != FrameType::Idr {
            errors.push("First coded packet in stream must be an IDR keyframe".to_string());
        }
    }

    let mut last_dts = i64::MIN;
    for (i, p) in packets.iter().enumerate() {
        total_bytes += p.data.len();
        total_qp += p.qp as u64;

        match p.frame_type {
            FrameType::Idr => {
                idr_packets += 1;
                if !p.keyframe {
                    warnings.push(format!(
                        "Packet {} is FrameType::Idr but keyframe flag is false",
                        i
                    ));
                }
            }
            FrameType::I => {
                idr_packets += 1;
            }
            FrameType::P => p_packets += 1,
            FrameType::B => b_packets += 1,
        }

        // Causal timing check: PTS must never be strictly less than DTS
        if p.pts < p.dts {
            errors.push(format!(
                "Packet {} has causal timeline violation: pts ({}) < dts ({})",
                i, p.pts, p.dts
            ));
        }

        // Monotonic DTS check: DTS must be monotonically non-decreasing
        if p.dts < last_dts {
            errors.push(format!(
                "Packet {} has non-monotonic DTS: dts ({}) < previous dts ({})",
                i, p.dts, last_dts
            ));
        }
        last_dts = p.dts;

        // Packet payload check
        if p.data.is_empty() {
            errors.push(format!("Packet {} has empty payload data", i));
        }
    }

    let avg_qp = if !packets.is_empty() {
        total_qp as f32 / packets.len() as f32
    } else {
        0.0
    };

    let estimated_bitrate_kbps = if cfg.fps_den > 0 && cfg.fps_num > 0 && !packets.is_empty() {
        let fps = cfg.fps_num as f64 / cfg.fps_den as f64;
        let duration_secs = packets.len() as f64 / fps;
        if duration_secs > 0.0 {
            Some((total_bytes as f64 * 8.0) / (duration_secs * 1000.0))
        } else {
            None
        }
    } else {
        None
    };

    tracing::debug!(
        packets_generated = packets.len(),
        idr_packets,
        p_packets,
        b_packets,
        total_bytes,
        "Diagnosed H.264 encoder output"
    );

    H264EncoderDiagnosticReport {
        width: cfg.width,
        height: cfg.height,
        profile: format!("{:?}", cfg.profile),
        preset: format!("{:?}", cfg.preset),
        keyint: cfg.keyint,
        bframes: cfg.bframes,
        packets_generated: packets.len(),
        idr_packets,
        p_packets,
        b_packets,
        avg_qp,
        total_bytes,
        estimated_bitrate_kbps,
        warnings,
        errors,
    }
}
