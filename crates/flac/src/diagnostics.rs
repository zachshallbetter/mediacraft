//! FLAC stream diagnostics and structural validation (RFC 9639).

use serde::{Deserialize, Serialize};

/// Parsed FLAC STREAMINFO metadata block (RFC 9639 §5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlacStreamInfo {
    /// Minimum block size in samples (16..=65535).
    pub min_block_size: u16,
    /// Maximum block size in samples (16..=65535).
    pub max_block_size: u16,
    /// Minimum frame size in bytes (0 if unknown).
    pub min_frame_size: u32,
    /// Maximum frame size in bytes (0 if unknown).
    pub max_frame_size: u32,
    /// Sampling rate in Hz (1..=1,048,575).
    pub sample_rate: u32,
    /// Number of audio channels (1..=8).
    pub channels: u8,
    /// Bit depth per sample (8..=32).
    pub bits_per_sample: u8,
    /// Total inter-channel samples in the stream (0 if unknown).
    pub total_samples: u64,
    /// MD5 checksum of unencoded raw audio data.
    pub md5_signature: [u8; 16],
}

/// Comprehensive diagnostic analysis report for a FLAC audio stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlacDiagnosticReport {
    /// True if the stream begins with the 4-byte `fLaC` stream marker.
    pub valid_marker: bool,
    /// Number of metadata blocks successfully traversed.
    pub metadata_blocks_count: usize,
    /// Parsed STREAMINFO block, if present and valid.
    pub streaminfo: Option<FlacStreamInfo>,
    /// Whether a VORBIS_COMMENT metadata block was detected.
    pub has_vorbis_comment: bool,
    /// Whether a SEEKTABLE metadata block was detected.
    pub has_seektable: bool,
    /// Number of audio frame sync codes identified.
    pub frame_count: usize,
    /// Estimated stream duration in seconds derived from total_samples and sample_rate.
    pub estimated_duration_secs: Option<f64>,
    /// True if the STREAMINFO MD5 signature contains non-zero bytes.
    pub has_md5: bool,
    /// Non-fatal diagnostic warnings discovered during analysis.
    pub warnings: Vec<String>,
    /// Structural validation errors encountered.
    pub errors: Vec<String>,
}

/// Inspect and diagnose a FLAC stream, validating marker, metadata blocks, and audio frames.
pub fn diagnose_flac_stream(data: &[u8]) -> FlacDiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if data.len() < 4 {
        errors.push("Stream too short to contain FLAC 4-byte marker".to_string());
        return FlacDiagnosticReport {
            valid_marker: false,
            metadata_blocks_count: 0,
            streaminfo: None,
            has_vorbis_comment: false,
            has_seektable: false,
            frame_count: 0,
            estimated_duration_secs: None,
            has_md5: false,
            warnings,
            errors,
        };
    }

    let valid_marker = &data[..4] == b"fLaC";
    if !valid_marker {
        errors.push("Invalid FLAC stream marker: expected 'fLaC'".to_string());
    }

    let mut offset = 4;
    let mut metadata_blocks_count = 0;
    let mut streaminfo = None;
    let mut has_vorbis_comment = false;
    let mut has_seektable = false;
    let mut is_last_block = false;

    // Parse metadata blocks
    while offset < data.len() && !is_last_block {
        let block_header = match data.get(offset..offset + 4) {
            Some(b) => b,
            None => {
                warnings.push(format!(
                    "Truncated metadata block header at byte offset {}",
                    offset
                ));
                break;
            }
        };

        is_last_block = (block_header[0] & 0x80) != 0;
        let block_type = block_header[0] & 0x7F;
        let block_len =
            u32::from_be_bytes([0, block_header[1], block_header[2], block_header[3]]) as usize;
        offset += 4;

        let block_data = match data.get(offset..offset.saturating_add(block_len)) {
            Some(d) if d.len() == block_len => d,
            _ => {
                warnings.push(format!(
                    "Truncated metadata block type {} at offset {} (expected {} bytes)",
                    block_type, offset, block_len
                ));
                break;
            }
        };

        metadata_blocks_count += 1;

        match block_type {
            0 => {
                // STREAMINFO block (mandatory first block, exactly 34 bytes)
                if metadata_blocks_count != 1 {
                    warnings.push("STREAMINFO must be the first metadata block".to_string());
                }
                if block_len != 34 {
                    errors.push(format!(
                        "Invalid STREAMINFO length: expected 34 bytes, got {}",
                        block_len
                    ));
                } else if let Some(info) = parse_streaminfo_body(block_data, &mut warnings) {
                    streaminfo = Some(info);
                }
            }
            3 => {
                has_seektable = true;
            }
            4 => {
                has_vorbis_comment = true;
            }
            127 => {
                errors.push("Invalid metadata block type 127 (reserved/illegal)".to_string());
            }
            _ => {
                // Other valid metadata types: 1 (PADDING), 2 (APPLICATION), 5 (CUESHEET), 6 (PICTURE)
            }
        }

        offset = offset.saturating_add(block_len);
    }

    if metadata_blocks_count == 0 {
        errors.push("Stream contains no metadata blocks".to_string());
    } else if streaminfo.is_none() {
        errors.push("Stream missing mandatory STREAMINFO metadata block".to_string());
    }

    // Inspect audio frames after metadata blocks
    let mut frame_count = 0;
    let mut frame_offset = offset;
    while frame_offset + 2 <= data.len() {
        let b0 = data[frame_offset];
        let b1 = data[frame_offset + 1];

        // FLAC frame sync code is 14 bits: 0b11111111111110 (0xFF followed by high 6 bits = 0xFC/0xF8)
        if b0 == 0xFF && (b1 & 0xFC) == 0xF8 {
            frame_count += 1;
            // Frame found; advance at least 2 bytes
            frame_offset += 2;
        } else {
            frame_offset += 1;
        }
    }

    let has_md5 = streaminfo
        .as_ref()
        .is_some_and(|info| info.md5_signature != [0u8; 16]);

    let estimated_duration_secs = streaminfo.as_ref().and_then(|info| {
        if info.sample_rate > 0 && info.total_samples > 0 {
            Some(info.total_samples as f64 / info.sample_rate as f64)
        } else {
            None
        }
    });

    tracing::debug!(
        valid_marker,
        metadata_blocks_count,
        frame_count,
        has_md5,
        "Diagnosed FLAC stream"
    );

    FlacDiagnosticReport {
        valid_marker,
        metadata_blocks_count,
        streaminfo,
        has_vorbis_comment,
        has_seektable,
        frame_count,
        estimated_duration_secs,
        has_md5,
        warnings,
        errors,
    }
}

fn parse_streaminfo_body(data: &[u8], warnings: &mut Vec<String>) -> Option<FlacStreamInfo> {
    if data.len() < 34 {
        return None;
    }

    let min_block_size = u16::from_be_bytes([data[0], data[1]]);
    let max_block_size = u16::from_be_bytes([data[2], data[3]]);
    let min_frame_size = u32::from_be_bytes([0, data[4], data[5], data[6]]);
    let max_frame_size = u32::from_be_bytes([0, data[7], data[8], data[9]]);

    let sr_raw = u32::from_be_bytes([0, data[10], data[11], data[12]]);
    let sample_rate = sr_raw >> 4;
    let channels = ((data[12] >> 1) & 0x07) + 1;
    let bits_per_sample = (((data[12] & 0x01) << 4) | (data[13] >> 4)) + 1;

    let total_high = u64::from(data[13] & 0x0F);
    let total_low = u64::from(u32::from_be_bytes([data[14], data[15], data[16], data[17]]));
    let total_samples = (total_high << 32) | total_low;

    let mut md5_signature = [0u8; 16];
    md5_signature.copy_from_slice(&data[18..34]);

    if min_block_size < 16 {
        warnings.push(format!(
            "min_block_size {} is below specification minimum of 16",
            min_block_size
        ));
    }
    if max_block_size < min_block_size {
        warnings.push(format!(
            "max_block_size {} is less than min_block_size {}",
            max_block_size, min_block_size
        ));
    }
    if sample_rate == 0 || sample_rate > 1_048_575 {
        warnings.push(format!(
            "sample_rate {} is outside valid RFC range 1..=1048575",
            sample_rate
        ));
    }
    if !(8..=32).contains(&bits_per_sample) {
        warnings.push(format!(
            "bits_per_sample {} is outside valid range 8..=32",
            bits_per_sample
        ));
    }

    Some(FlacStreamInfo {
        min_block_size,
        max_block_size,
        min_frame_size,
        max_frame_size,
        sample_rate,
        channels,
        bits_per_sample,
        total_samples,
        md5_signature,
    })
}
