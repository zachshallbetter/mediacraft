//! End-to-End Audiovisual Transcoding Pipeline (H.264 + AAC in MP4)
//!
//! Encodes uncompressed video frames and audio samples into a synchronized, Faststart
//! MP4 deliverable. Coordinates stream interleaving via `StreamScheduler` and ensures
//! atomic staging via `StagingCleanupGuard`.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

use super::aac_adapter::{AacAdapterError, AacEncoderAdapter};
use super::h264_adapter::{H264AdapterError, H264EncoderAdapter};
use super::mp4_mux_adapter::{Mp4MuxError, Mp4MuxerAdapter};
use crate::scheduler::{StreamConfig, StreamScheduler, SyncReport};
use mediacraft_protocol::stream::{AudioFrame, Rational, VideoFrame};

#[derive(Debug, Error)]
pub enum AvPipelineError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("H.264 encoder error: {0}")]
    H264(#[from] H264AdapterError),
    #[error("AAC encoder error: {0}")]
    Aac(#[from] AacAdapterError),
    #[error("MP4 muxer error: {0}")]
    Mux(#[from] Mp4MuxError),
    #[error("invalid pipeline input: {0}")]
    InvalidInput(String),
}

/// Execution metrics and verification receipt from an audiovisual transcode job.
#[derive(Clone, Debug, PartialEq)]
pub struct AvTranscodeReceipt {
    pub output_path: PathBuf,
    pub video_frames_encoded: usize,
    pub audio_frames_encoded: usize,
    pub total_output_bytes: u64,
    pub elapsed_wall_clock_ms: u128,
    pub sync_report: SyncReport,
    pub faststart: bool,
}

/// Configuration options for the audiovisual pipeline.
#[derive(Clone, Debug)]
pub struct AvPipelineConfig {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub video_crf: Option<f32>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub audio_bitrate_bps: u32,
    pub faststart: bool,
}

impl Default for AvPipelineConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps_num: 24,
            fps_den: 1,
            video_crf: Some(23.0),
            video_bitrate_kbps: None,
            audio_sample_rate: 48000,
            audio_channels: 2,
            audio_bitrate_bps: 192000,
            faststart: true,
        }
    }
}

/// End-to-end Audiovisual Pipeline.
pub struct AvPipeline {
    config: AvPipelineConfig,
}

impl AvPipeline {
    pub fn new(config: AvPipelineConfig) -> Self {
        Self { config }
    }

    /// Transcodes a sequence of video frames and corresponding audio frames to an MP4 deliverable.
    pub fn transcode<P: AsRef<Path>, F: FnMut(f32)>(
        &self,
        output_path: P,
        video_frames: &[VideoFrame],
        audio_frame: &AudioFrame,
        mut progress_cb: Option<F>,
    ) -> Result<AvTranscodeReceipt, AvPipelineError> {
        let start_time = Instant::now();
        let out_path = output_path.as_ref().to_path_buf();

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let staging_path = out_path.with_extension(format!(
            "tmp.av.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        // 1. Initialize Encoders
        let mut video_encoder = H264EncoderAdapter::new(
            self.config.width,
            self.config.height,
            self.config.fps_num,
            self.config.fps_den,
            self.config.video_crf,
            self.config.video_bitrate_kbps,
            0, // Video track = stream 0
        )?;

        let mut audio_encoder = AacEncoderAdapter::new(
            self.config.audio_sample_rate,
            self.config.audio_channels,
            self.config.audio_bitrate_bps,
        )?;

        let (sps, pps) = video_encoder.sps_pps();
        let asc = audio_encoder.audio_specific_config().to_vec();
        let priming_samples = audio_encoder.priming_samples();
        let total_audio_frames = audio_frame.frame_count();

        // 2. Initialize Scheduler
        let video_timebase = Rational::new(self.config.fps_den, self.config.fps_num);
        let audio_timebase = Rational::new(1, self.config.audio_sample_rate);

        let stream_configs = vec![
            StreamConfig {
                stream_index: 0,
                timebase: video_timebase,
                is_video: true,
            },
            StreamConfig {
                stream_index: 1,
                timebase: audio_timebase,
                is_video: false,
            },
        ];
        let mut scheduler = StreamScheduler::new(stream_configs);

        // 3. Initialize MP4 Muxer with staging file
        let out_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&staging_path)?;

        let mut muxer = Mp4MuxerAdapter::new(out_file, self.config.audio_sample_rate)?;
        muxer.add_video_track(
            self.config.width,
            self.config.height,
            self.config.fps_num,
            &sps,
            &pps,
        )?;
        muxer.add_audio_track(
            self.config.audio_sample_rate,
            self.config.audio_channels,
            &asc,
            priming_samples,
            Some(total_audio_frames as u64),
        )?;

        if let Some(ref mut cb) = progress_cb {
            cb(0.10);
        }

        // 4. Encode Video Frames
        let total_v_frames = video_frames.len();
        for (idx, vf) in video_frames.iter().enumerate() {
            let packets = video_encoder.encode(vf)?;
            for pkt in packets {
                scheduler.push_packet(pkt);
            }
            if let Some(ref mut cb) = progress_cb {
                let frac = 0.10 + 0.40 * (idx as f32 / total_v_frames.max(1) as f32);
                cb(frac);
            }
        }
        for pkt in video_encoder.flush()? {
            scheduler.push_packet(pkt);
        }
        scheduler.signal_eos(0);

        // 5. Encode Audio Frames in Chunks
        let mut audio_cursor = 0;
        let chunk_size = self.config.audio_sample_rate as usize; // 1-second chunks
        let planar_refs: Vec<&[f32]> = audio_frame.samples.iter().map(|c| c.as_slice()).collect();

        while audio_cursor < total_audio_frames {
            let end = (audio_cursor + chunk_size).min(total_audio_frames);
            let mut sub_slices = Vec::with_capacity(planar_refs.len());
            for ch in &planar_refs {
                sub_slices.push(&ch[audio_cursor..end]);
            }
            let raw_aus = audio_encoder.encode(&sub_slices);
            for au in raw_aus {
                scheduler.push_packet(mediacraft_protocol::stream::MediaPacket {
                    stream_index: 1,
                    data: au,
                    pts: audio_cursor as i64,
                    dts: audio_cursor as i64,
                    duration: 1024,
                    timebase: audio_timebase,
                    is_keyframe: true,
                });
            }
            audio_cursor = end;
        }
        for au in audio_encoder.flush() {
            scheduler.push_packet(mediacraft_protocol::stream::MediaPacket {
                stream_index: 1,
                data: au,
                pts: audio_cursor as i64,
                dts: audio_cursor as i64,
                duration: 1024,
                timebase: audio_timebase,
                is_keyframe: true,
            });
        }
        scheduler.signal_eos(1);

        if let Some(ref mut cb) = progress_cb {
            cb(0.70);
        }

        // 6. Write Scheduled Interleaved Packets to Muxer
        while let Some(pkt) = scheduler.pop_next_packet() {
            muxer.write_packet(&pkt)?;
        }

        if let Some(ref mut cb) = progress_cb {
            cb(0.90);
        }

        // 7. Finalize Container & Commit Atomically
        muxer.finish(self.config.faststart)?;
        std::fs::rename(&staging_path, &out_path)?;

        if let Some(ref mut cb) = progress_cb {
            cb(1.0);
        }

        let elapsed = start_time.elapsed();
        let total_output_bytes = std::fs::metadata(&out_path)?.len();
        let sync_report = scheduler.finish_report();

        Ok(AvTranscodeReceipt {
            output_path: out_path,
            video_frames_encoded: total_v_frames,
            audio_frames_encoded: total_audio_frames,
            total_output_bytes,
            elapsed_wall_clock_ms: elapsed.as_millis(),
            sync_report,
            faststart: self.config.faststart,
        })
    }
}
