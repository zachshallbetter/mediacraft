//! Container Demuxer & H.264 Video Decoder Adapter
//!
//! Provides progressive demuxing of MP4/MOV containers and frame-accurate decoding
//! of H.264 bitstreams into uncompressed `VideoFrame` buffers.

use filmcraft_h264::Decoder as H264Decoder;
use filmcraft_isobmff::{CodecConfig, TrackKind, open as isobmff_open};
use mediacraft_protocol::stream::{Rational, VideoFrame};
use std::fs::File;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VideoDemuxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ISO BMFF demuxer error: {0}")]
    Iso(#[from] filmcraft_isobmff::Error),
    #[error("H.264 decoder error: {0}")]
    H264(#[from] filmcraft_h264::Error),
    #[error("no video track found in container")]
    NoVideoTrack,
    #[error("unsupported video codec in track: {0}")]
    UnsupportedCodec(String),
}

/// Metadata extracted from a video container.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoStreamInfo {
    pub track_id: u32,
    pub width: u32,
    pub height: u32,
    pub timescale: u32,
    pub duration_ticks: u64,
    pub duration_seconds: f64,
    pub sample_count: usize,
    pub codec_name: String,
}

/// Demuxer and decoder for MP4/MOV H.264 containers.
pub struct VideoDemuxer;

impl VideoDemuxer {
    /// Inspects container and extracts video stream metadata.
    pub fn inspect<P: AsRef<Path>>(path: P) -> Result<VideoStreamInfo, VideoDemuxError> {
        let file = File::open(path)?;
        let mp4 = isobmff_open(&file)?;

        let track = mp4
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .ok_or(VideoDemuxError::NoVideoTrack)?;

        let codec_name = match track.codec() {
            Some(CodecConfig::Avc(_)) => "h264",
            Some(CodecConfig::Hevc(_)) => "hevc",
            Some(CodecConfig::ProRes { .. }) => "prores",
            _ => "unknown",
        };

        let duration_seconds = if track.timescale > 0 {
            track.duration as f64 / track.timescale as f64
        } else {
            0.0
        };

        Ok(VideoStreamInfo {
            track_id: track.id,
            width: track.width,
            height: track.height,
            timescale: track.timescale,
            duration_ticks: track.duration,
            duration_seconds,
            sample_count: track.samples.len(),
            codec_name: codec_name.to_string(),
        })
    }

    /// Decodes all video frames from an MP4/MOV container.
    pub fn decode_all_frames<P: AsRef<Path>>(path: P) -> Result<Vec<VideoFrame>, VideoDemuxError> {
        let file = File::open(path)?;
        let mp4 = isobmff_open(&file)?;

        let track_idx = mp4
            .track_of_kind(TrackKind::Video)
            .ok_or(VideoDemuxError::NoVideoTrack)?;
        let track = &mp4.tracks[track_idx];

        let avc_cfg = match track.codec() {
            Some(CodecConfig::Avc(c)) => c,
            other => {
                return Err(VideoDemuxError::UnsupportedCodec(format!("{other:?}")));
            }
        };

        let avcc_bytes = avc_cfg.to_bytes();
        let mut decoder = H264Decoder::from_avcc(&avcc_bytes)?;
        let timebase = Rational::new(1, track.timescale.max(1));

        let mut decoded_frames = Vec::new();

        for (sample_idx, sample) in track.samples.iter().enumerate() {
            let data = mp4.read_sample(&file, track_idx, sample_idx)?;
            let pictures = decoder.decode(&data, sample.pts)?;
            for pic in pictures {
                decoded_frames.push(VideoFrame {
                    width: pic.width,
                    height: pic.height,
                    y: pic.y,
                    u: pic.u,
                    v: pic.v,
                    y_stride: pic.y_stride,
                    uv_stride: pic.uv_stride,
                    pts: pic.pts,
                    timebase,
                    is_keyframe: pic.key,
                });
            }
        }

        let flushed = decoder.flush();
        for pic in flushed {
            decoded_frames.push(VideoFrame {
                width: pic.width,
                height: pic.height,
                y: pic.y,
                u: pic.u,
                v: pic.v,
                y_stride: pic.y_stride,
                uv_stride: pic.uv_stride,
                pts: pic.pts,
                timebase,
                is_keyframe: pic.key,
            });
        }

        Ok(decoded_frames)
    }

    /// Extracts a single preview frame nearest to a requested presentation timestamp (in seconds).
    pub fn decode_preview_frame<P: AsRef<Path>>(
        path: P,
        target_pts_sec: f64,
    ) -> Result<Option<VideoFrame>, VideoDemuxError> {
        let frames = Self::decode_all_frames(path)?;
        if frames.is_empty() {
            return Ok(None);
        }

        let best = frames.into_iter().min_by(|a, b| {
            let diff_a = (a.pts_seconds() - target_pts_sec).abs();
            let diff_b = (b.pts_seconds() - target_pts_sec).abs();
            diff_a
                .partial_cmp(&diff_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(best)
    }
}
