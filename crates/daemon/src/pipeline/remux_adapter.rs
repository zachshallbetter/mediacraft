//! Stream-Copy Remux Pipeline (Milestone 10 / Track 2).
//!
//! Provides zero-decode container repackaging (e.g. MOV -> MP4, MP4 -> MOV)
//! without decompressing or re-encoding media samples. Preserves exact bitstreams,
//! bitrates, and timestamps while reorganizing into progressive Faststart containers.

use std::fs::File;
use std::path::{Path, PathBuf};

use craft_isobmff::{CodecConfig, TrackKind, open as isobmff_open};
use mediacraft_protocol::stream::{MediaPacket, Rational};
use thiserror::Error;

use super::mp4_mux_adapter::{Mp4MuxError, Mp4MuxerAdapter};
use crate::scheduler::{StreamConfig, StreamScheduler};

#[derive(Debug, Error)]
pub enum RemuxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ISO BMFF error: {0}")]
    Iso(#[from] craft_isobmff::Error),
    #[error("Multiplexer error: {0}")]
    Mux(#[from] Mp4MuxError),
    #[error("No compatible video or audio tracks found for stream-copy")]
    NoCompatibleTracks,
    #[error("Unsupported codec for stream copy: {0}")]
    UnsupportedCodec(String),
}

/// Execution receipt detailing remux output statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct RemuxReceipt {
    pub output_path: PathBuf,
    pub total_bytes: u64,
    pub duration_seconds: f64,
    pub video_packets: usize,
    pub audio_packets: usize,
}

/// Remuxes compatible video and audio tracks from source container to target container.
pub fn execute_stream_copy_remux(
    source_path: &Path,
    output_path: &Path,
    faststart: bool,
) -> Result<RemuxReceipt, RemuxError> {
    let src_file = File::open(source_path)?;
    let src_mp4 = isobmff_open(&src_file)?;

    // Atomic staging file
    let staging_path = output_path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let out_file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&staging_path)?;

    // Use source movie timescale or default to 1000
    let movie_timescale = src_mp4.timescale.max(1);
    let mut muxer = Mp4MuxerAdapter::new(out_file, movie_timescale)?;

    let mut dest_video_idx = None;
    let mut src_video_idx = None;
    let mut v_timescale = 24u32;
    let mut dest_audio_idx = None;
    let mut src_audio_idx = None;
    let mut a_timescale = 48000u32;

    // 1. Inspect and map video track
    if let Some(v_idx) = src_mp4.track_of_kind(TrackKind::Video) {
        let v_track = &src_mp4.tracks[v_idx];
        match v_track.codec() {
            Some(CodecConfig::Avc(avc)) => {
                let sps = avc.sps.first().map(|s| s.as_slice()).unwrap_or(&[]);
                let pps = avc.pps.first().map(|p| p.as_slice()).unwrap_or(&[]);
                v_timescale = v_track.timescale.max(1);
                let mux_v_idx =
                    muxer.add_video_track(v_track.width, v_track.height, v_timescale, sps, pps)?;
                dest_video_idx = Some(mux_v_idx);
                src_video_idx = Some(v_idx);
            }
            Some(other) => {
                return Err(RemuxError::UnsupportedCodec(format!("{other:?}")));
            }
            None => {}
        }
    }

    // 2. Inspect and map audio track
    if let Some(a_idx) = src_mp4.track_of_kind(TrackKind::Audio) {
        let a_track = &src_mp4.tracks[a_idx];
        if let Some(CodecConfig::Aac(aac)) = a_track.codec() {
            let priming = a_track
                .edits
                .first()
                .and_then(|e| {
                    if e.media_time > 0 {
                        Some(e.media_time as usize)
                    } else {
                        None
                    }
                })
                .unwrap_or(1024);
            let channels = aac.channel_config.max(1) as u16;
            a_timescale = if aac.sample_rate > 0 {
                aac.sample_rate
            } else {
                a_track.timescale.max(1)
            };

            let mux_a_idx =
                muxer.add_audio_track(a_timescale, channels, &aac.asc, priming, None)?;
            dest_audio_idx = Some(mux_a_idx);
            src_audio_idx = Some(a_idx);
        }
    }

    if dest_video_idx.is_none() && dest_audio_idx.is_none() {
        let _ = std::fs::remove_file(&staging_path);
        return Err(RemuxError::NoCompatibleTracks);
    }

    // Configure stream scheduler
    let mut configs = Vec::new();
    if let Some(v_idx) = dest_video_idx {
        configs.push(StreamConfig {
            stream_index: v_idx,
            timebase: Rational::new(1, v_timescale),
            is_video: true,
        });
    }
    if let Some(a_idx) = dest_audio_idx {
        configs.push(StreamConfig {
            stream_index: a_idx,
            timebase: Rational::new(1, a_timescale),
            is_video: false,
        });
    }
    let mut scheduler = StreamScheduler::new(configs);

    // 3. Populate stream scheduler with source media samples
    let mut total_v_pkts: usize = 0;
    let mut total_a_pkts: usize = 0;

    if let (Some(v_idx), Some(mux_v_idx)) = (src_video_idx, dest_video_idx) {
        let v_track = &src_mp4.tracks[v_idx];
        for (sample_idx, sample) in v_track.samples.iter().enumerate() {
            let raw_bytes = src_mp4.read_sample(&src_file, v_idx, sample_idx)?;
            let pkt = MediaPacket {
                stream_index: mux_v_idx,
                pts: sample.pts,
                dts: sample.dts,
                duration: sample.duration as u64,
                timebase: Rational::new(1, v_timescale),
                is_keyframe: sample.is_sync,
                data: raw_bytes,
            };
            scheduler.push_packet(pkt);
            total_v_pkts = total_v_pkts.saturating_add(1);
        }
    }

    if let (Some(a_idx), Some(mux_a_idx)) = (src_audio_idx, dest_audio_idx) {
        let a_track = &src_mp4.tracks[a_idx];
        for (sample_idx, sample) in a_track.samples.iter().enumerate() {
            let raw_bytes = src_mp4.read_sample(&src_file, a_idx, sample_idx)?;
            let pkt = MediaPacket {
                stream_index: mux_a_idx,
                pts: sample.pts,
                dts: sample.dts,
                duration: sample.duration as u64,
                timebase: Rational::new(1, a_timescale),
                is_keyframe: sample.is_sync,
                data: raw_bytes,
            };
            scheduler.push_packet(pkt);
            total_a_pkts = total_a_pkts.saturating_add(1);
        }
    }

    // 4. Drain scheduled packets in interleaved monotonic order
    while let Some(packet) = scheduler.pop_next_packet() {
        if let Err(e) = muxer.write_packet(&packet) {
            let _ = std::fs::remove_file(&staging_path);
            return Err(RemuxError::Mux(e));
        }
    }

    // 5. Finalize with Faststart
    if let Err(e) = muxer.finish(faststart) {
        let _ = std::fs::remove_file(&staging_path);
        return Err(RemuxError::Mux(e));
    }

    // 6. Complete atomic commit
    if let Err(_e) = std::fs::rename(&staging_path, output_path) {
        // Fallback to copy + remove for cross-mount moves
        if let Err(cp_err) = std::fs::copy(&staging_path, output_path) {
            let _ = std::fs::remove_file(&staging_path);
            return Err(RemuxError::Io(cp_err));
        }
        let _ = std::fs::remove_file(&staging_path);
    }

    let total_bytes = std::fs::metadata(output_path)?.len();
    let duration_seconds = if src_mp4.timescale > 0 {
        src_mp4.duration as f64 / src_mp4.timescale as f64
    } else {
        0.0
    };

    Ok(RemuxReceipt {
        output_path: output_path.to_path_buf(),
        total_bytes,
        duration_seconds,
        video_packets: total_v_pkts,
        audio_packets: total_a_pkts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remux_rejects_missing_tracks() {
        let temp_dir = std::env::temp_dir().join("mediacraft_test_remux_missing");
        let _ = std::fs::create_dir_all(&temp_dir);
        let non_existent = temp_dir.join("non_existent.mp4");
        let output = temp_dir.join("out.mp4");
        let res = execute_stream_copy_remux(&non_existent, &output, true);
        assert!(res.is_err());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
