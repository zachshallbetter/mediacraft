//! Preset Execution Planner and Preflight Engine (Milestone 6).
//!
//! A preset is a binding contract between target delivery constraints and
//! execution capabilities. The plan compiler performs preflight verification:
//! - Container vs. codec compatibility checks.
//! - Resolution & subsampling alignment checks (e.g. even dimensions for YUV420p).
//! - Sample rate and channel topology constraints.
//! - Explicit execution route selection (AudioVideo, AudioOnly, VideoOnly).

use mediacraft_protocol::{AudioCodec, ContainerFormat, EncodingPreset, VideoCodec};
use std::path::{Path, PathBuf};

/// Explicit error hierarchy for preflight plan validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("Source file does not exist: {0}")]
    SourceNotFound(PathBuf),

    #[error("Incompatible container '{container:?}' with video '{video:?}' and audio '{audio:?}'")]
    IncompatibleContainerCodec {
        container: ContainerFormat,
        video: VideoCodec,
        audio: AudioCodec,
    },

    #[error("Invalid video dimensions ({width}x{height}): {reason}")]
    InvalidVideoDimensions {
        width: u32,
        height: u32,
        reason: String,
    },

    #[error("Invalid audio parameters ({sample_rate} Hz, {channels} ch): {reason}")]
    InvalidAudioParameters {
        sample_rate: u32,
        channels: u16,
        reason: String,
    },

    #[error("Invalid frame rate ({num}/{den}): numerator and denominator must be non-zero")]
    InvalidFrameRate { num: u32, den: u32 },

    #[error("Delivery contract violation: {0}")]
    ContractViolation(String),
}

/// Execution route determined during plan compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRoute {
    /// Synchronized Audio and Video multiplexed into container.
    AudioVideo,
    /// Audio-only transcode and packaging.
    AudioOnly,
    /// Video-only transcode and packaging.
    VideoOnly,
}

/// A fully validated, executable transcode plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionPlan {
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    pub preset_id: String,
    pub route: ExecutionRoute,
    pub container: ContainerFormat,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub crf: u8,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub audio_bitrate_kbps: u32,
    pub priming_samples: u32,
    pub faststart: bool,
}

/// Compiles an `EncodingPreset` and source/target paths into a validated `ExecutionPlan`.
pub fn compile_execution_plan(
    source_path: &Path,
    output_path: &Path,
    preset: &EncodingPreset,
) -> Result<ExecutionPlan, PlanError> {
    // 1. Source existence check
    if !source_path.exists() {
        return Err(PlanError::SourceNotFound(source_path.to_path_buf()));
    }

    // 2. Determine route based on source file kind and preset codecs
    let source_ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let is_audio_source = matches!(source_ext.as_str(), "wav" | "flac" | "aif" | "aiff" | "m4a");
    let is_video_source = matches!(source_ext.as_str(), "mp4" | "mov" | "mkv" | "mxf" | "avi");

    let route = match (is_audio_source, is_video_source) {
        (true, false) => {
            // Source is audio-only
            if preset.container == ContainerFormat::Wav
                || preset.container == ContainerFormat::M4a
                || preset.video_codec == VideoCodec::None
            {
                ExecutionRoute::AudioOnly
            } else {
                // Audio source targeted to video container: produces slate/black video + audio
                ExecutionRoute::AudioVideo
            }
        }
        (false, true) => {
            // Source is video container (may contain both audio and video)
            if preset.video_codec == VideoCodec::None {
                ExecutionRoute::AudioOnly
            } else if preset.audio_codec == AudioCodec::None {
                ExecutionRoute::VideoOnly
            } else {
                ExecutionRoute::AudioVideo
            }
        }
        _ => {
            // Default to preset-specified structure
            if preset.video_codec == VideoCodec::None {
                ExecutionRoute::AudioOnly
            } else if preset.audio_codec == AudioCodec::None {
                ExecutionRoute::VideoOnly
            } else {
                ExecutionRoute::AudioVideo
            }
        }
    };

    // 3. Container vs Codec compatibility verification
    validate_container_codec_matrix(
        preset.container,
        preset.video_codec,
        preset.audio_codec,
        route,
    )?;

    // 4. Video parameter validation
    let width = preset.width.unwrap_or(1920);
    let height = preset.height.unwrap_or(1080);
    if route != ExecutionRoute::AudioOnly {
        if width == 0 || height == 0 {
            return Err(PlanError::InvalidVideoDimensions {
                width,
                height,
                reason: "Dimensions must be strictly positive".to_string(),
            });
        }
        // Subsampling alignment: 4:2:0 requires even width and height
        if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return Err(PlanError::InvalidVideoDimensions {
                width,
                height,
                reason: "YUV420p requires even width and height".to_string(),
            });
        }
    }

    // 5. Frame rate validation
    let fps_num = preset.frame_rate_num.unwrap_or(24);
    let fps_den = preset.frame_rate_den.unwrap_or(1);
    if route != ExecutionRoute::AudioOnly && (fps_num == 0 || fps_den == 0) {
        return Err(PlanError::InvalidFrameRate {
            num: fps_num,
            den: fps_den,
        });
    }

    // 6. Audio parameter validation
    let audio_sample_rate = preset.audio_sample_rate.unwrap_or(48_000);
    let audio_channels = preset.audio_channels.unwrap_or(2);
    if route != ExecutionRoute::VideoOnly {
        if !(8000..=192000).contains(&audio_sample_rate) {
            return Err(PlanError::InvalidAudioParameters {
                sample_rate: audio_sample_rate,
                channels: audio_channels,
                reason: "Sample rate must be between 8,000 and 192,000 Hz".to_string(),
            });
        }
        if audio_channels == 0 || audio_channels > 8 {
            return Err(PlanError::InvalidAudioParameters {
                sample_rate: audio_sample_rate,
                channels: audio_channels,
                reason: "Channel count must be between 1 and 8".to_string(),
            });
        }
    }

    // 7. Quality rate control (CRF / Bitrate)
    let crf = 23u8; // Standard balance
    let audio_bitrate_kbps = preset.audio_bitrate_kbps.unwrap_or(192);

    Ok(ExecutionPlan {
        source_path: source_path.to_path_buf(),
        output_path: output_path.to_path_buf(),
        preset_id: preset.id.clone(),
        route,
        container: preset.container,
        video_codec: preset.video_codec,
        audio_codec: preset.audio_codec,
        width,
        height,
        fps_num,
        fps_den,
        crf,
        audio_sample_rate,
        audio_channels,
        audio_bitrate_kbps,
        priming_samples: preset.audio_priming_samples,
        faststart: preset.faststart,
    })
}

fn validate_container_codec_matrix(
    container: ContainerFormat,
    video: VideoCodec,
    audio: AudioCodec,
    route: ExecutionRoute,
) -> Result<(), PlanError> {
    match container {
        ContainerFormat::Mp4 | ContainerFormat::M4a | ContainerFormat::Mov => {
            if route != ExecutionRoute::AudioOnly
                && !matches!(
                    video,
                    VideoCodec::H264
                        | VideoCodec::Hevc
                        | VideoCodec::ProRes422
                        | VideoCodec::ProRes4444
                        | VideoCodec::PassThrough
                        | VideoCodec::None
                )
            {
                return Err(PlanError::IncompatibleContainerCodec {
                    container,
                    video,
                    audio,
                });
            }
            if route != ExecutionRoute::VideoOnly
                && !matches!(
                    audio,
                    AudioCodec::AacLc
                        | AudioCodec::Flac
                        | AudioCodec::Opus
                        | AudioCodec::Pcm16Le
                        | AudioCodec::Pcm24Le
                        | AudioCodec::PassThrough
                        | AudioCodec::None
                )
            {
                return Err(PlanError::IncompatibleContainerCodec {
                    container,
                    video,
                    audio,
                });
            }
        }
        ContainerFormat::Wav => {
            if route != ExecutionRoute::AudioOnly || video != VideoCodec::None {
                return Err(PlanError::IncompatibleContainerCodec {
                    container,
                    video,
                    audio,
                });
            }
        }
        ContainerFormat::Mxf => {
            if route != ExecutionRoute::AudioOnly
                && !matches!(
                    video,
                    VideoCodec::ProRes422
                        | VideoCodec::ProRes4444
                        | VideoCodec::H264
                        | VideoCodec::None
                )
            {
                return Err(PlanError::IncompatibleContainerCodec {
                    container,
                    video,
                    audio,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_preset_compilation() {
        let temp_dir = std::env::temp_dir();
        let src = temp_dir.join("plan_test_src.wav");
        std::fs::write(&src, b"RIFF....").unwrap();
        let out = temp_dir.join("plan_test_out.m4a");

        let preset = EncodingPreset {
            container: ContainerFormat::M4a,
            video_codec: VideoCodec::None,
            audio_codec: AudioCodec::AacLc,
            ..Default::default()
        };

        let plan = compile_execution_plan(&src, &out, &preset).unwrap();
        assert_eq!(plan.route, ExecutionRoute::AudioOnly);
        assert_eq!(plan.container, ContainerFormat::M4a);

        let _ = std::fs::remove_file(src);
    }

    #[test]
    fn test_rejects_odd_video_dimensions_for_yuv420p() {
        let temp_dir = std::env::temp_dir();
        let src = temp_dir.join("plan_odd_dim.mp4");
        std::fs::write(&src, b"dummy").unwrap();
        let out = temp_dir.join("out.mp4");

        let preset = EncodingPreset {
            width: Some(1920),
            height: Some(1081), // Odd height!
            ..Default::default()
        };

        let err = compile_execution_plan(&src, &out, &preset).unwrap_err();
        assert!(matches!(err, PlanError::InvalidVideoDimensions { .. }));

        let _ = std::fs::remove_file(src);
    }

    #[test]
    fn test_rejects_incompatible_wav_with_video() {
        let temp_dir = std::env::temp_dir();
        let src = temp_dir.join("plan_wav_vid.mp4");
        std::fs::write(&src, b"dummy").unwrap();
        let out = temp_dir.join("out.wav");

        let preset = EncodingPreset {
            container: ContainerFormat::Wav,
            video_codec: VideoCodec::H264, // WAV cannot hold H.264!
            ..Default::default()
        };

        let err = compile_execution_plan(&src, &out, &preset).unwrap_err();
        assert!(matches!(err, PlanError::IncompatibleContainerCodec { .. }));

        let _ = std::fs::remove_file(src);
    }
}
