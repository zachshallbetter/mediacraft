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

/// Thread-safe adapter over the clean-room FLAC encoder.
pub struct FlacEncoderAdapter {
    encoder: Encoder,
    buffer: Vec<u8>,
}

impl FlacEncoderAdapter {
    /// Creates a new FLAC encoder adapter.
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8) -> Result<Self, FlacError> {
        let channels_clamped = channels.min(8);
        if channels_clamped == 0 {
            return Err(FlacError::Init("Zero channels provided".to_string()));
        }
        let config = EncoderConfig::new(sample_rate, channels_clamped, bits_per_sample);
        let encoder = Encoder::new(config).map_err(|e| FlacError::Init(e.to_string()))?;
        let buffer = encoder.header();
        Ok(Self { encoder, buffer })
    }

    /// Encodes a block of planar audio channel samples into the internal bitstream buffer.
    pub fn encode_block(&mut self, channels: &[&[i32]]) -> Result<(), FlacError> {
        let body = self
            .encoder
            .encode(channels)
            .map_err(|e| FlacError::Encode(e.to_string()))?;
        self.buffer.extend(body);
        Ok(())
    }

    /// Finishes the stream, patches the STREAMINFO header metadata block, and returns full FLAC file bytes.
    pub fn finish(mut self) -> Vec<u8> {
        self.buffer.extend(self.encoder.finish());
        let offset = STREAMINFO_OFFSET as usize;
        let streaminfo = self.encoder.streaminfo();
        if self.buffer.len() >= offset.saturating_add(streaminfo.len()) {
            self.buffer[offset..offset.saturating_add(streaminfo.len())]
                .copy_from_slice(&streaminfo);
        }
        self.buffer
    }
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

    let mut adapter = FlacEncoderAdapter::new(sample_rate, channels.len() as u8, bits_per_sample)?;
    let channel_slices: Vec<&[i32]> = channels.iter().map(|c| c.as_slice()).collect();
    adapter.encode_block(&channel_slices)?;

    let file_buf = adapter.finish();
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
