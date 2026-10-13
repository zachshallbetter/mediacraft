//! AAC-LC Encoder Adapter
//!
//! Adapts the clean-room `filmcraft-aac` engine into MediaCraft's pipeline.
//! Handles planar float audio sample ingestion, psychoacoustic analysis,
//! quantization, and raw access unit packetization with 1024-sample priming calculation.

use filmcraft_aac::{Encoder, EncoderConfig};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AacAdapterError {
    #[error("AAC configuration error: {0}")]
    Config(#[from] filmcraft_aac::Error),
    #[error("invalid channel count: {0} (supported: 1..=8)")]
    InvalidChannels(u16),
}

/// Thread-safe adapter over the pure-Rust AAC-LC encoder.
pub struct AacEncoderAdapter {
    encoder: Encoder,
    priming_samples: usize,
    asc: Vec<u8>,
}

impl AacEncoderAdapter {
    /// Creates a new AAC-LC encoder configured for Constant Bitrate (CBR).
    pub fn new(sample_rate: u32, channels: u16, bitrate_bps: u32) -> Result<Self, AacAdapterError> {
        if channels == 0 || channels > 8 {
            return Err(AacAdapterError::InvalidChannels(channels));
        }

        let cfg = EncoderConfig::cbr(sample_rate, channels as usize, bitrate_bps);
        let encoder = Encoder::new(cfg)?;
        let priming_samples = encoder.priming_samples();
        let asc = encoder.audio_specific_config();

        Ok(Self {
            encoder,
            priming_samples: priming_samples as usize,
            asc,
        })
    }

    /// Feeds planar audio samples (`[&[f32]]`) and returns produced raw access units.
    pub fn encode(&mut self, planar: &[&[f32]]) -> Vec<Vec<u8>> {
        self.encoder.encode(planar)
    }

    /// Flushes any pending buffered frames in the psychoacoustic window reservoir.
    pub fn flush(&mut self) -> Vec<Vec<u8>> {
        self.encoder.flush()
    }

    /// Returns the exact encoder priming delay in audio samples (typically 1024 for AAC-LC).
    pub fn priming_samples(&self) -> usize {
        self.priming_samples
    }

    /// Returns the MPEG-4 AudioSpecificConfig bytes for the MP4 `esds` atom.
    pub fn audio_specific_config(&self) -> &[u8] {
        &self.asc
    }
}
