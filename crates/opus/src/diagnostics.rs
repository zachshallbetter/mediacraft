//! Opus stream diagnostics and packet inspection (RFC 6716, RFC 7845, RFC 8251).

use serde::{Deserialize, Serialize};

use crate::header::OpusHead;
use crate::packet::{Mode, Packet};
use crate::{Error, Result};

/// Structural diagnostics for an Opus identification header (`OpusHead` or ISO-BMFF `dOps`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpusHeadDiagnostic {
    /// Number of audio channels (1..=255).
    pub channels: usize,
    /// Codec pre-skip in samples at 48 kHz.
    pub pre_skip: u16,
    /// Original input sampling rate in Hz.
    pub input_sample_rate: u32,
    /// Output gain in decibels.
    pub output_gain_db: f32,
    /// Channel mapping family (0 = mono/stereo, 1 = surround, 255 = discrete).
    pub mapping_family: u8,
    /// Number of coded elementary streams.
    pub streams: usize,
    /// Number of coupled (stereo) streams.
    pub coupled_streams: usize,
    /// Channel-to-stream mapping table.
    pub channel_mapping: Vec<u8>,
}

/// Structural diagnostics for a single Opus audio packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpusPacketDiagnostic {
    /// Coding mode (SILK-only, Hybrid, or CELT-only).
    pub mode: String,
    /// Audio bandwidth (Narrowband, Mediumband, Wideband, SuperWideband, Fullband).
    pub bandwidth: String,
    /// True if the packet is coded as stereo.
    pub is_stereo: bool,
    /// Number of audio frames packed in the packet (1..=48).
    pub frame_count: usize,
    /// Number of audio samples per frame at 48 kHz.
    pub samples_per_frame_48k: usize,
    /// Total audio samples across all frames in this packet at 48 kHz.
    pub total_samples_48k: usize,
    /// Duration of the audio packet in milliseconds.
    pub duration_ms: f64,
}

/// Comprehensive stream diagnosis combining header and packet inspections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpusDiagnosticReport {
    /// Parsed identification header, if provided.
    pub head: Option<OpusHeadDiagnostic>,
    /// Number of packets evaluated.
    pub packets_inspected: usize,
    /// Set of distinct coding modes observed across all inspected packets.
    pub modes_detected: Vec<String>,
    /// Set of distinct bandwidths observed across all inspected packets.
    pub bandwidths_detected: Vec<String>,
    /// Non-fatal diagnostic warnings.
    pub warnings: Vec<String>,
    /// Validation errors encountered.
    pub errors: Vec<String>,
}

/// Inspect an Opus identification header (`OpusHead` packet or `dOps` box payload).
pub fn diagnose_opus_head(data: &[u8]) -> Result<OpusHeadDiagnostic> {
    let head = if data.len() >= 8 && &data[..8] == b"OpusHead" {
        OpusHead::parse(data)?
    } else {
        OpusHead::from_dops(data)?
    };

    Ok(OpusHeadDiagnostic {
        channels: head.channels,
        pre_skip: head.pre_skip,
        input_sample_rate: head.input_sample_rate,
        output_gain_db: head.output_gain as f32 / 256.0,
        mapping_family: head.mapping_family,
        streams: head.streams,
        coupled_streams: head.coupled_streams,
        channel_mapping: head.mapping,
    })
}

/// Inspect and decode the table-of-contents and framing of a single Opus packet.
pub fn diagnose_opus_packet(data: &[u8]) -> Result<OpusPacketDiagnostic> {
    if data.is_empty() {
        return Err(Error::InvalidPacket("empty packet"));
    }

    let packet = Packet::parse(data)?;
    let toc = packet.toc;

    let mode_str = match toc.mode {
        Mode::SilkOnly => "SilkOnly",
        Mode::Hybrid => "Hybrid",
        Mode::CeltOnly => "CeltOnly",
    };

    let bw_str = match toc.bandwidth {
        crate::packet::Bandwidth::Narrowband => "Narrowband",
        crate::packet::Bandwidth::Mediumband => "Mediumband",
        crate::packet::Bandwidth::Wideband => "Wideband",
        crate::packet::Bandwidth::SuperWideband => "SuperWideband",
        crate::packet::Bandwidth::Fullband => "Fullband",
    };

    let frame_count = packet.frames.len();
    let samples_per_frame = toc.frame_samples_48k();
    let total_samples = samples_per_frame * frame_count;
    let duration_ms = (total_samples as f64 / 48000.0) * 1000.0;

    Ok(OpusPacketDiagnostic {
        mode: mode_str.to_string(),
        bandwidth: bw_str.to_string(),
        is_stereo: toc.stereo,
        frame_count,
        samples_per_frame_48k: samples_per_frame,
        total_samples_48k: total_samples,
        duration_ms,
    })
}

/// Inspect an Opus stream composed of an optional header and a sequence of packets.
pub fn diagnose_opus_stream(head_bytes: Option<&[u8]>, packets: &[&[u8]]) -> OpusDiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let head = match head_bytes {
        Some(hb) => match diagnose_opus_head(hb) {
            Ok(h) => Some(h),
            Err(e) => {
                errors.push(format!("Failed to parse Opus head: {}", e));
                None
            }
        },
        None => None,
    };

    let mut modes = Vec::new();
    let mut bandwidths = Vec::new();

    for (idx, &pkt) in packets.iter().enumerate() {
        match diagnose_opus_packet(pkt) {
            Ok(diag) => {
                if !modes.contains(&diag.mode) {
                    modes.push(diag.mode);
                }
                if !bandwidths.contains(&diag.bandwidth) {
                    bandwidths.push(diag.bandwidth);
                }
            }
            Err(e) => {
                warnings.push(format!("Packet {} invalid: {}", idx, e));
            }
        }
    }

    if packets.is_empty() {
        warnings.push("No packets provided for evaluation".to_string());
    }

    tracing::debug!(
        packets_inspected = packets.len(),
        has_head = head.is_some(),
        "Diagnosed Opus stream"
    );

    OpusDiagnosticReport {
        head,
        packets_inspected: packets.len(),
        modes_detected: modes,
        bandwidths_detected: bandwidths,
        warnings,
        errors,
    }
}
