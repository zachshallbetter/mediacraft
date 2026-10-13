//! Unified MP4 / MOV Multiplexer Adapter
//!
//! Provides simultaneous multiplexing of H.264 video tracks and AAC audio tracks into
//! progressive ISO Base Media File Format containers (`.mp4`, `.mov`) with Faststart layout.

use filmcraft_isobmff::{
    AacConfig, AvcConfig, Brand, CodecConfig, Edit, FourCc, Mp4Writer, SampleEntry, TrackConfig,
    WriteSample, WriterOptions,
};
use mediacraft_protocol::stream::MediaPacket;
use std::io::{Read, Seek, Write};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Mp4MuxError {
    #[error("ISO BMFF muxer error: {0}")]
    Iso(#[from] filmcraft_isobmff::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid track index: {0}")]
    InvalidTrack(usize),
}

/// Multiplexer supporting concurrent video and audio tracks in an MP4 container.
pub struct Mp4MuxerAdapter<W: Read + Write + Seek> {
    writer: Mp4Writer<W>,
    video_track_idx: Option<usize>,
    audio_track_idx: Option<usize>,
    video_timescale: u32,
    audio_timescale: u32,
    movie_timescale: u32,
}

impl<W: Read + Write + Seek> Mp4MuxerAdapter<W> {
    /// Creates a new MP4 multiplexer with a declared movie timescale.
    pub fn new(output: W, movie_timescale: u32) -> Result<Self, Mp4MuxError> {
        let mut options = WriterOptions::new(Brand::Mp4);
        options.movie_timescale = movie_timescale.max(1);
        let writer = Mp4Writer::new(output, options)?;

        Ok(Self {
            writer,
            video_track_idx: None,
            audio_track_idx: None,
            video_timescale: 24,
            audio_timescale: 48_000,
            movie_timescale: movie_timescale.max(1),
        })
    }

    /// Adds an H.264 video track configured with `avcC` parameter sets (SPS/PPS).
    pub fn add_video_track(
        &mut self,
        width: u32,
        height: u32,
        timescale: u32,
        sps: &[u8],
        pps: &[u8],
    ) -> Result<usize, Mp4MuxError> {
        let avc_cfg = AvcConfig::new(vec![sps.to_vec()], vec![pps.to_vec()], 4);
        let entry = SampleEntry::avc(avc_cfg, width as u16, height as u16);

        let track_cfg = TrackConfig::new(entry, timescale);
        let track_idx = self.writer.add_track(track_cfg)?;
        self.video_track_idx = Some(track_idx);
        self.video_timescale = timescale;

        Ok(track_idx)
    }

    /// Adds an AAC-LC audio track with priming delay compensation and sample-exact trimming.
    pub fn add_audio_track(
        &mut self,
        sample_rate: u32,
        channels: u16,
        asc: &[u8],
        priming_samples: usize,
        exact_duration_frames: Option<u64>,
    ) -> Result<usize, Mp4MuxError> {
        let aac_cfg = AacConfig::from_asc(asc.to_vec());
        let entry = SampleEntry::audio(
            FourCc(*b"mp4a"),
            CodecConfig::Aac(aac_cfg),
            channels as u32,
            sample_rate as f64,
            16,
        );

        let mut track_cfg = TrackConfig::new(entry, sample_rate);
        if let Some(target_frames) = exact_duration_frames {
            let segment_duration = if sample_rate > 0 {
                (target_frames as u128 * self.movie_timescale as u128 / sample_rate as u128) as u64
            } else {
                target_frames
            };
            track_cfg.edits = vec![Edit {
                segment_duration,
                media_time: priming_samples as i64,
                media_rate: 0x0001_0000,
            }];
        } else {
            track_cfg.media_start = Some(priming_samples as i64);
        }

        let track_idx = self.writer.add_track(track_cfg)?;
        self.audio_track_idx = Some(track_idx);
        self.audio_timescale = sample_rate;

        Ok(track_idx)
    }

    /// Writes a scheduled packet to its target track.
    pub fn write_packet(&mut self, packet: &MediaPacket) -> Result<(), Mp4MuxError> {
        let is_video = self.video_track_idx == Some(packet.stream_index);
        let comp_offset = if is_video {
            packet.pts.saturating_sub(packet.dts) as i32
        } else {
            0
        };

        let sample = WriteSample {
            data: &packet.data,
            duration: packet.duration as u32,
            composition_offset: comp_offset,
            is_sync: packet.is_keyframe,
        };

        self.writer.write_sample(packet.stream_index, sample)?;
        Ok(())
    }

    /// Finalizes container layout, optionally relocating the `moov` atom ahead of `mdat`.
    pub fn finish(self, faststart: bool) -> Result<(), Mp4MuxError> {
        if faststart {
            self.writer.finish_faststart()?;
        } else {
            self.writer.finish()?;
        }
        Ok(())
    }
}
