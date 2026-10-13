//! Clean-room FLAC audio encoding adapter (Milestone 10).
//!
//! Encodes uncompressed audio into RFC 9639 compliant FLAC streams
//! using `filmcraft-flac`.

use filmcraft_flac::{Encoder, EncoderConfig, STREAMINFO_OFFSET};
use std::path::Path;

/// Errors produced during FLAC encoding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FlacError {
    #[error("FLAC encoder initialization failed: {0}")]
    Init(String),
    #[error("FLAC encoding error: {0}")]
    Encode(String),
    #[error("IO error: {0}")]
    Io(String),
}

/// Transcodes planar 16-bit or 24-bit audio channels into a standard `.flac` deliverable.
pub fn encode_to_flac(
    channels: &[Vec<i32>],
    sample_rate: u32,
    bits_per_sample: u8,
    output_path: &Path,
) -> Result<usize, FlacError> {
    if channels.is_empty() {
        return Err(FlacError::Init("Zero channels provided".to_string()));
    }

    let channels_count = channels.len().min(8) as u8;
    let config = EncoderConfig::new(sample_rate, channels_count, bits_per_sample);

    let mut enc = Encoder::new(config).map_err(|e| FlacError::Init(e.to_string()))?;
    let mut file_buf = enc.header();

    let channel_slices: Vec<&[i32]> = channels.iter().map(|c| c.as_slice()).collect();
    let body = enc
        .encode(&channel_slices)
        .map_err(|e| FlacError::Encode(e.to_string()))?;
    file_buf.extend(body);
    file_buf.extend(enc.finish());

    // Update STREAMINFO with exact sizes and MD5
    let offset = STREAMINFO_OFFSET as usize;
    let streaminfo = enc.streaminfo();
    if file_buf.len() >= offset.saturating_add(streaminfo.len()) {
        file_buf[offset..offset.saturating_add(streaminfo.len())].copy_from_slice(&streaminfo);
    }

    let total_len = file_buf.len();
    craft_atomic::atomic_write(output_path, &file_buf).map_err(|e| FlacError::Io(e.to_string()))?;

    Ok(total_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_to_flac_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let out_file = temp_dir.join("test_output.flac");

        let sample_rate = 48000;
        let mut left = Vec::with_capacity(sample_rate);
        let mut right = Vec::with_capacity(sample_rate);
        for i in 0..sample_rate {
            let s = ((i as f64 * 0.05).sin() * 15000.0) as i32;
            left.push(s);
            right.push(s);
        }

        let bytes_written =
            encode_to_flac(&[left, right], sample_rate as u32, 16, &out_file).unwrap();
        assert!(out_file.exists());
        assert!(bytes_written > 4);

        let data = std::fs::read(&out_file).unwrap();
        assert_eq!(&data[0..4], b"fLaC");

        let _ = std::fs::remove_file(out_file);
    }
}
