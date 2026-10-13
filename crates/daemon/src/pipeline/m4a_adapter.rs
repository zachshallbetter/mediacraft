//! ISO BMFF / M4A Multiplexer Adapter
//!
//! Adapts the clean-room `filmcraft-isobmff` engine into MediaCraft's pipeline.
//! Writes progressive MPEG-4 audio (`.m4a`) containers with `ftyp`, `esds` AAC descriptions,
//! sample-accurate priming compensation via edit list (`elst`) atoms, and FastStart
//! in-place `moov` relocation.

use filmcraft_isobmff::{
    AacConfig, Brand, CodecConfig, FourCc, Mp4Writer, SampleEntry, TrackConfig, WriteSample,
    WriterOptions,
};
use std::io::{Read, Seek, Write};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum M4aAdapterError {
    #[error("ISO BMFF muxer error: {0}")]
    Iso(#[from] filmcraft_isobmff::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no active audio track configured")]
    NoAudioTrack,
}

/// M4A Container Writer wrapping the pure-Rust ISO BMFF multiplexer.
pub struct M4aMuxerAdapter<W: Read + Write + Seek> {
    writer: Mp4Writer<W>,
    audio_track_idx: Option<usize>,
}

impl<W: Read + Write + Seek> M4aMuxerAdapter<W> {
    /// Creates a new M4A multiplexer targeting an arbitrary `Read + Write + Seek` output stream.
    pub fn new(output: W) -> Result<Self, M4aAdapterError> {
        let options = WriterOptions::new(Brand::Mp4);
        let writer = Mp4Writer::new(output, options)?;
        Ok(Self {
            writer,
            audio_track_idx: None,
        })
    }

    /// Configures the primary MPEG-4 AAC-LC audio track with AudioSpecificConfig and priming edit.
    pub fn setup_audio_track(
        &mut self,
        sample_rate: u32,
        channels: u16,
        asc: &[u8],
        priming_samples: usize,
    ) -> Result<usize, M4aAdapterError> {
        let aac_cfg = AacConfig::from_asc(asc.to_vec());
        let entry = SampleEntry::audio(
            FourCc(*b"mp4a"),
            CodecConfig::Aac(aac_cfg),
            channels as u32,
            sample_rate as f64,
            16,
        );

        let mut track_cfg = TrackConfig::new(entry, sample_rate);
        // Convenience edit list: skip encoder priming delay (1024 samples)
        track_cfg.media_start = Some(priming_samples as i64);

        let track_idx = self.writer.add_track(track_cfg)?;
        self.audio_track_idx = Some(track_idx);
        Ok(track_idx)
    }

    /// Writes a single AAC access unit sample (1024 audio frames).
    pub fn write_access_unit(&mut self, data: &[u8]) -> Result<(), M4aAdapterError> {
        let track_idx = self.audio_track_idx.ok_or(M4aAdapterError::NoAudioTrack)?;
        let sample = WriteSample {
            data,
            duration: 1024,
            composition_offset: 0,
            is_sync: true,
        };
        self.writer.write_sample(track_idx, sample)?;
        Ok(())
    }

    /// Finalizes container layout. When `faststart` is true, relocates the `moov` atom ahead of `mdat`.
    pub fn finish(self, faststart: bool) -> Result<(), M4aAdapterError> {
        if faststart {
            self.writer.finish_faststart()?;
        } else {
            self.writer.finish()?;
        }
        Ok(())
    }
}
