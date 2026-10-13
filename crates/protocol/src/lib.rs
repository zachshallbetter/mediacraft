//! IPC protocol definitions for MediaCraft background media encoding service.
//!
//! Provides typed requests, responses, and state models used across:
//! - Background transcoding daemon (`mediacraft-daemon`)
//! - Client applications (FilmCraft export, EffectCraft render queue, MediaCraft CLI)
//! - JSON-RPC over Unix domain sockets or Windows named pipes.

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

pub mod stream;
pub use stream::{AudioFrame, MediaFrame, MediaPacket, PixelFormat, Rational, VideoFrame};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The protocol major version.
pub const PROTOCOL_VERSION: u32 = 1;

/// Unique identifier for a transcoding job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobId(pub u64);

/// Target container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerFormat {
    Mp4,
    Mov,
    M4a,
    Wav,
    Mxf,
}

/// Target video codec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    Hevc,
    ProRes422,
    ProRes4444,
    DnxHr,
    PassThrough,
    None,
}

/// Target audio codec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    AacLc,
    Pcm16Le,
    Pcm24Le,
    PassThrough,
    None,
}

/// Encoding preset specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncodingPreset {
    pub id: String,
    pub name: String,
    pub container: ContainerFormat,
    pub video_codec: VideoCodec,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate_num: Option<u32>,
    pub frame_rate_den: Option<u32>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_codec: AudioCodec,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u16>,
    pub audio_bitrate_kbps: Option<u32>,
    /// Edit List priming compensation in samples (e.g. 1024 for AAC-LC).
    pub audio_priming_samples: u32,
    /// Faststart moov placement before mdat.
    pub faststart: bool,
    /// Optional delivery contract with acceptance constraints.
    #[serde(default)]
    pub contract: Option<DeliveryContract>,
}

/// Target delivery acceptance contract containing testable constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryContract {
    pub name: String,
    pub require_faststart: bool,
    pub max_bitrate_kbps: Option<u32>,
    pub max_true_peak_dbfs: Option<f32>,
    pub target_lufs: Option<f32>,
    pub lufs_tolerance: Option<f32>,
    pub require_audio_priming: bool,
    pub max_file_size_bytes: Option<u64>,
}

impl Default for DeliveryContract {
    fn default() -> Self {
        Self {
            name: "Broadcast & Web Standards".to_string(),
            require_faststart: true,
            max_bitrate_kbps: Some(50_000),
            max_true_peak_dbfs: Some(-1.0),
            target_lufs: Some(-24.0),
            lufs_tolerance: Some(1.0),
            require_audio_priming: true,
            max_file_size_bytes: None,
        }
    }
}

/// Severity classification for delivery contract validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Accept,
    Warn,
    Reject,
}

impl ValidationSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::Warn => "Warn",
            Self::Reject => "Reject",
        }
    }
}

/// A specific finding evaluated during delivery contract verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub criterion: String,
    pub observed: String,
    pub expected: String,
    pub severity: ValidationSeverity,
    pub passed: bool,
}

/// A machine-readable preflight / delivery validation report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub target_file: PathBuf,
    pub source_file: PathBuf,
    pub contract_name: String,
    pub overall_status: ValidationSeverity,
    pub findings: Vec<ValidationFinding>,
    pub validated_at_unix_ts: u64,
}

impl Default for EncodingPreset {
    fn default() -> Self {
        Self {
            id: "match-source-h264".to_string(),
            name: "Match Source - High Bitrate (H.264/AAC)".to_string(),
            container: ContainerFormat::Mp4,
            video_codec: VideoCodec::H264,
            width: None,
            height: None,
            frame_rate_num: None,
            frame_rate_den: None,
            video_bitrate_kbps: Some(15_000),
            audio_codec: AudioCodec::AacLc,
            audio_sample_rate: Some(48_000),
            audio_channels: Some(2),
            audio_bitrate_kbps: Some(256),
            audio_priming_samples: 1024,
            faststart: true,
            contract: Some(DeliveryContract::default()),
        }
    }
}

/// Reason why the queue is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// Paused explicitly by the user or client command.
    UserRequested,
    /// Paused automatically because an NLE/DAW timeline is actively playing back.
    PlaybackActive,
}

/// State of an individual transcoding job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "details", rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running {
        progress_pct: f32,
        current_frame: u64,
        total_frames: u64,
        encoding_fps: f32,
    },
    Paused {
        progress_pct: f32,
        reason: PauseReason,
    },
    Completed {
        duration_secs: f64,
        bytes_written: u64,
    },
    Failed {
        error_message: String,
    },
    Cancelled,
}

/// Complete descriptor of a transcoding job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: JobId,
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    pub preset: EncodingPreset,
    pub priority: u32,
    pub state: JobState,
    pub created_at_unix_ts: u64,
}

/// Queue-level status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueStatus {
    pub is_running: bool,
    pub pause_reason: Option<PauseReason>,
    pub active_jobs: usize,
    pub queued_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
}

/// Requests sent from client applications (FilmCraft, EffectCraft, CLI) to daemon.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Ping,
    AddJob {
        source_path: PathBuf,
        output_path: PathBuf,
        preset: Option<EncodingPreset>,
        priority: Option<u32>,
    },
    CancelJob {
        job_id: JobId,
    },
    PauseQueue {
        reason: PauseReason,
    },
    ResumeQueue {
        reason: PauseReason,
    },
    SetPlaybackActive {
        is_playing: bool,
    },
    GetJob {
        job_id: JobId,
    },
    ListJobs,
    GetQueueStatus,
    Shutdown,
}

/// Responses sent from daemon to client applications.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
    Pong {
        protocol_version: u32,
    },
    JobAdded {
        job_id: JobId,
    },
    JobDetails {
        job: JobInfo,
    },
    JobList {
        jobs: Vec<JobInfo>,
        queue: QueueStatus,
    },
    QueueStatusUpdated {
        queue: QueueStatus,
    },
    Ok,
    Error {
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_request() {
        let req = Request::AddJob {
            source_path: PathBuf::from("/sources/interview.mov"),
            output_path: PathBuf::from("/exports/interview.mp4"),
            preset: Some(EncodingPreset::default()),
            priority: Some(10),
        };
        let json = serde_json::to_string(&req).unwrap_or_else(|_| String::new());
        assert!(!json.is_empty());
        let back: Request = serde_json::from_str(&json).unwrap_or(Request::Ping);
        assert_eq!(req, back);
    }

    #[test]
    fn serde_roundtrip_response() {
        let resp = Response::JobAdded { job_id: JobId(42) };
        let json = serde_json::to_string(&resp).unwrap_or_else(|_| String::new());
        assert!(!json.is_empty());
        let back: Response = serde_json::from_str(&json).unwrap_or(Response::Ok);
        assert_eq!(resp, back);
    }
}
