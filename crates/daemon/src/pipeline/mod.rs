//! Media Transcoding Pipeline Module
//!
//! Provides concrete, clean-room media pipelines connecting source audio/video
//! decoders, format converters, encoders, and multiplexers into verified deliverables.

pub mod aac_adapter;
pub mod m4a_adapter;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

use crate::wav::{WavAudio, WavError, read_wav_file};
use aac_adapter::{AacAdapterError, AacEncoderAdapter};
use m4a_adapter::{M4aAdapterError, M4aMuxerAdapter};

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("WAV decode error: {0}")]
    Wav(#[from] WavError),
    #[error("AAC encoder error: {0}")]
    Aac(#[from] AacAdapterError),
    #[error("M4A muxer error: {0}")]
    M4a(#[from] M4aAdapterError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Execution metrics and verification receipt from an audio transcode job.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioTranscodeReceipt {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    pub total_input_frames: usize,
    pub total_output_bytes: u64,
    pub elapsed_wall_clock_ms: u128,
    pub realtime_multiplier: f64,
    pub priming_samples: usize,
    pub faststart: bool,
}

/// End-to-end WAV to AAC/M4A Transcode Pipeline.
pub struct WavToM4aPipeline;

impl WavToM4aPipeline {
    /// Transcodes a Linear PCM WAV file to a progressive Faststart MPEG-4 Audio (.m4a) file.
    pub fn transcode<P: AsRef<Path>, Q: AsRef<Path>, F: FnMut(f32)>(
        input_wav: P,
        output_m4a: Q,
        bitrate_bps: u32,
        faststart: bool,
        mut progress_cb: Option<F>,
    ) -> Result<AudioTranscodeReceipt, PipelineError> {
        let start_time = Instant::now();
        let input_path = input_wav.as_ref().to_path_buf();
        let output_path = output_m4a.as_ref().to_path_buf();

        if let Some(ref mut cb) = progress_cb {
            cb(0.05);
        }

        // 1. Decode WAV input
        let wav: WavAudio = read_wav_file(&input_path)?;
        let total_frames = wav.frame_count();

        if let Some(ref mut cb) = progress_cb {
            cb(0.20);
        }

        // 2. Initialize AAC-LC Encoder Adapter
        let mut encoder = AacEncoderAdapter::new(wav.sample_rate, wav.channels, bitrate_bps)?;
        let priming_samples = encoder.priming_samples();
        let asc = encoder.audio_specific_config().to_vec();

        // 3. Initialize M4A Multiplexer Adapter
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let out_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&output_path)?;
        let mut muxer = M4aMuxerAdapter::new(out_file)?;
        muxer.setup_audio_track(wav.sample_rate, wav.channels, &asc, priming_samples)?;

        if let Some(ref mut cb) = progress_cb {
            cb(0.35);
        }

        // 4. Encode audio in streaming chunks to maintain progress telemetry
        let chunk_size = 48_000; // 1-second chunks for progress notification
        let mut frame_cursor = 0;

        while frame_cursor < total_frames {
            let end = (frame_cursor + chunk_size).min(total_frames);
            let mut channel_slices: Vec<&[f32]> = Vec::with_capacity(wav.channels as usize);
            for ch in &wav.samples {
                channel_slices.push(&ch[frame_cursor..end]);
            }

            let access_units = encoder.encode(&channel_slices);
            for au in access_units {
                muxer.write_access_unit(&au)?;
            }

            frame_cursor = end;
            if let Some(ref mut cb) = progress_cb {
                let frac = 0.35 + 0.50 * (frame_cursor as f32 / total_frames.max(1) as f32);
                cb(frac);
            }
        }

        // 5. Flush trailing encoder frames (reservoir)
        let flushed = encoder.flush();
        for au in flushed {
            muxer.write_access_unit(&au)?;
        }

        if let Some(ref mut cb) = progress_cb {
            cb(0.90);
        }

        // 6. Finalize Faststart layout (moov before mdat)
        muxer.finish(faststart)?;

        if let Some(ref mut cb) = progress_cb {
            cb(1.0);
        }

        let elapsed = start_time.elapsed();
        let output_bytes = std::fs::metadata(&output_path)?.len();
        let audio_duration_s = if wav.sample_rate > 0 {
            total_frames as f64 / wav.sample_rate as f64
        } else {
            0.0
        };
        let realtime_multiplier = if elapsed.as_secs_f64() > 0.0 {
            audio_duration_s / elapsed.as_secs_f64()
        } else {
            100.0
        };

        Ok(AudioTranscodeReceipt {
            input_path,
            output_path,
            sample_rate: wav.sample_rate,
            channels: wav.channels,
            total_input_frames: total_frames,
            total_output_bytes: output_bytes,
            elapsed_wall_clock_ms: elapsed.as_millis(),
            realtime_multiplier,
            priming_samples,
            faststart,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::write_wav_file;

    #[test]
    fn test_wav_to_m4a_pipeline_execution() {
        let temp_dir = std::env::temp_dir().join("mediacraft_test_pipeline");
        let _ = std::fs::create_dir_all(&temp_dir);

        let input_wav = temp_dir.join("test_tone.wav");
        let output_m4a = temp_dir.join("test_tone.m4a");

        let sample_rate = 48_000;
        let channels = 2;
        let frames = 48_000; // 1.0 second of audio

        let left: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.05).sin() * 0.7).collect();
        let right: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.10).cos() * 0.7).collect();

        write_wav_file(&input_wav, channels, sample_rate, &[&left, &right]).unwrap();

        let mut progress_calls = 0;
        let receipt = WavToM4aPipeline::transcode(
            &input_wav,
            &output_m4a,
            128_000, // 128 kbps
            true,    // Faststart
            Some(|_p| {
                progress_calls += 1;
            }),
        )
        .unwrap();

        assert_eq!(receipt.sample_rate, 48_000);
        assert_eq!(receipt.channels, 2);
        assert_eq!(receipt.total_input_frames, frames);
        assert_eq!(receipt.priming_samples, 1024);
        assert!(receipt.total_output_bytes > 0);
        assert!(progress_calls > 0);
        assert!(output_m4a.exists());

        // Clean up test files
        let _ = std::fs::remove_file(&input_wav);
        let _ = std::fs::remove_file(&output_m4a);
    }
}
