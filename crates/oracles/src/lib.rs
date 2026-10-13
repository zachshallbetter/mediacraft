//! Synthetic test patterns and AV synchronization verification oracles.
//!
//! Generates mathematical reference signals (frame flashes and audio impulses)
//! to test container priming delay compensation (1024-sample AAC edit lists)
//! and faststart atom ordering (`moov` before `mdat`).

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo,
        clippy::unreachable
    )
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleError {
    InvalidTimebase,
    BufferTooShort,
    MalformedContainer(String),
}

impl fmt::Display for OracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OracleError::InvalidTimebase => f.write_str("invalid timebase or zero framerate"),
            OracleError::BufferTooShort => f.write_str("byte buffer too short for box header"),
            OracleError::MalformedContainer(msg) => write!(f, "malformed container: {msg}"),
        }
    }
}
impl std::error::Error for OracleError {}

pub type Result<T> = std::result::Result<T, OracleError>;

/// Generates deterministic AV sync test signals.
#[derive(Debug, Clone)]
pub struct AvSyncGenerator {
    pub fps: u32,
    pub sample_rate: u32,
    pub interval_frames: u64,
}

impl AvSyncGenerator {
    pub fn new(fps: u32, sample_rate: u32, interval_seconds: u32) -> Result<Self> {
        if fps == 0 || sample_rate == 0 || interval_seconds == 0 {
            return Err(OracleError::InvalidTimebase);
        }
        let interval_frames = (fps as u64).saturating_mul(interval_seconds as u64);
        Ok(Self {
            fps,
            sample_rate,
            interval_frames,
        })
    }

    /// Determines if a given video frame index is a white flash marker.
    pub fn is_flash_frame(&self, frame_index: u64) -> bool {
        if self.interval_frames == 0 {
            return false;
        }
        frame_index.is_multiple_of(self.interval_frames)
    }

    /// Calculates the exact expected starting audio sample corresponding to a video frame.
    pub fn audio_sample_for_frame(&self, frame_index: u64) -> u64 {
        // sample = frame * sample_rate / fps
        let total_samples = frame_index.saturating_mul(self.sample_rate as u64);
        total_samples / (self.fps as u64)
    }

    /// Generates 1-channel 16-bit PCM samples for a given duration.
    /// Emits silence (0) except for a 1-sample impulse (32767) at each flash frame boundary.
    pub fn generate_pcm16_samples(&self, total_frames: u64) -> Vec<i16> {
        let total_samples = self.audio_sample_for_frame(total_frames) as usize;
        let mut samples = vec![0i16; total_samples];

        let mut f = 0u64;
        while f < total_frames {
            if self.is_flash_frame(f) {
                let s = self.audio_sample_for_frame(f) as usize;
                if let Some(target) = samples.get_mut(s) {
                    *target = i16::MAX;
                }
            }
            f = f.saturating_add(1);
        }

        samples
    }
}

/// Validation result for AAC-LC priming compensation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimingValidation {
    pub priming_samples: u32,
    pub edit_list_media_time: Option<i64>,
    pub sync_drift_samples: i64,
    pub is_synchronized: bool,
}

/// Validate whether an edit list (`elst`) correctly cancels out encoder priming delay.
pub fn validate_priming_compensation(
    encoder_priming_samples: u32,
    edit_list_media_time: Option<i64>,
) -> PrimingValidation {
    match edit_list_media_time {
        Some(media_time) => {
            let drift = media_time - (encoder_priming_samples as i64);
            PrimingValidation {
                priming_samples: encoder_priming_samples,
                edit_list_media_time: Some(media_time),
                sync_drift_samples: drift,
                is_synchronized: drift == 0,
            }
        }
        None => {
            // No edit list: playback starts at sample 0 of the bitstream, which is 1024 samples of delay
            PrimingValidation {
                priming_samples: encoder_priming_samples,
                edit_list_media_time: None,
                sync_drift_samples: -(encoder_priming_samples as i64),
                is_synchronized: false,
            }
        }
    }
}

/// Simple box header from an ISOBMFF stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxHeader {
    pub offset: usize,
    pub size: u64,
    pub fourcc: [u8; 4],
}

impl BoxHeader {
    pub fn fourcc_str(&self) -> String {
        self.fourcc.iter().map(|&b| b as char).collect()
    }
}

/// Inspects top-level boxes in an MP4/MOV byte stream and verifies faststart order.
pub fn verify_faststart_layout(data: &[u8]) -> Result<bool> {
    let mut pos = 0usize;
    let mut moov_offset = None;
    let mut mdat_offset = None;

    while pos + 8 <= data.len() {
        let size_u32 = u32::from_be_bytes([
            *data.get(pos).unwrap_or(&0),
            *data.get(pos + 1).unwrap_or(&0),
            *data.get(pos + 2).unwrap_or(&0),
            *data.get(pos + 3).unwrap_or(&0),
        ]) as u64;

        let fourcc = [
            *data.get(pos + 4).unwrap_or(&0),
            *data.get(pos + 5).unwrap_or(&0),
            *data.get(pos + 6).unwrap_or(&0),
            *data.get(pos + 7).unwrap_or(&0),
        ];

        let (header_len, total_size) = if size_u32 == 1 {
            // 64-bit extended size
            if pos + 16 > data.len() {
                return Err(OracleError::BufferTooShort);
            }
            let mut s = [0u8; 8];
            for (idx, b) in s.iter_mut().enumerate() {
                *b = *data.get(pos + 8 + idx).unwrap_or(&0);
            }
            (16usize, u64::from_be_bytes(s))
        } else if size_u32 == 0 {
            // box extends to EOF
            (8usize, (data.len() - pos) as u64)
        } else {
            (8usize, size_u32)
        };

        if &fourcc == b"moov" && moov_offset.is_none() {
            moov_offset = Some(pos);
        } else if &fourcc == b"mdat" && mdat_offset.is_none() {
            mdat_offset = Some(pos);
        }

        if total_size < header_len as u64 {
            return Err(OracleError::MalformedContainer(
                "invalid box size".to_string(),
            ));
        }

        pos = pos.saturating_add(total_size as usize);
    }

    match (moov_offset, mdat_offset) {
        (Some(moov), Some(mdat)) => Ok(moov < mdat),
        _ => Err(OracleError::MalformedContainer(
            "missing moov or mdat atom".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_generator_intervals() {
        let generator = AvSyncGenerator::new(24, 48_000, 1).unwrap();
        assert!(generator.is_flash_frame(0));
        assert!(!generator.is_flash_frame(1));
        assert!(generator.is_flash_frame(24));
        assert_eq!(generator.audio_sample_for_frame(0), 0);
        assert_eq!(generator.audio_sample_for_frame(24), 48_000);

        let pcm = generator.generate_pcm16_samples(48);
        assert_eq!(pcm[0], i16::MAX);
        assert_eq!(pcm[1], 0);
        assert_eq!(pcm[48_000], i16::MAX);
    }

    #[test]
    fn priming_delay_compensation() {
        // Uncompensated AAC introduces 1024 samples of delay
        let uncompensated = validate_priming_compensation(1024, None);
        assert!(!uncompensated.is_synchronized);
        assert_eq!(uncompensated.sync_drift_samples, -1024);

        // Edit list media_time = 1024 perfectly zeroes the delay
        let compensated = validate_priming_compensation(1024, Some(1024));
        assert!(compensated.is_synchronized);
        assert_eq!(compensated.sync_drift_samples, 0);
    }

    #[test]
    fn faststart_verification() {
        // Construct a synthetic minimal MP4 with ftyp -> moov -> mdat
        let mut buf = Vec::new();
        // ftyp (16 bytes)
        buf.extend_from_slice(&16u32.to_be_bytes());
        buf.extend_from_slice(b"ftyp");
        buf.extend_from_slice(b"isom\0\0\x02\0");

        // moov (12 bytes)
        buf.extend_from_slice(&12u32.to_be_bytes());
        buf.extend_from_slice(b"moov");
        buf.extend_from_slice(&[0u8; 4]);

        // mdat (16 bytes)
        buf.extend_from_slice(&16u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");
        buf.extend_from_slice(&[0u8; 8]);

        assert!(verify_faststart_layout(&buf).unwrap());
    }

    #[test]
    fn end_to_end_m4a_faststart_reorganization() {
        use mediacraft_bitstream::isobmff::{build_synthetic_m4a, reorganize_to_faststart};

        let fake_pcm = vec![127u8; 1024];
        // 1. Build naive stream (mdat before moov)
        let naive = build_synthetic_m4a(&fake_pcm, 48_000, 2, 1024, false);
        let naive_faststart = verify_faststart_layout(&naive).unwrap();
        assert!(!naive_faststart, "naive layout must have mdat before moov");

        // 2. Reorganize with faststart engine
        let faststart = reorganize_to_faststart(&naive).unwrap();
        let is_faststart = verify_faststart_layout(&faststart).unwrap();
        assert!(
            is_faststart,
            "reorganized layout must have moov before mdat"
        );

        // 3. Directly built faststart container
        let direct_faststart = build_synthetic_m4a(&fake_pcm, 48_000, 2, 1024, true);
        assert!(verify_faststart_layout(&direct_faststart).unwrap());
    }
}
