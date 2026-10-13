//! Audio buffer views, channel configurations, and sample formats.

use serde::{Deserialize, Serialize};

/// Audio channel layout configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Surround30,
    Surround40,
    Surround51,
    Surround71,
    Custom(u16),
}

impl ChannelLayout {
    pub const fn channels(&self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Surround30 => 3,
            Self::Surround40 => 4,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
            Self::Custom(n) => *n as usize,
        }
    }

    pub fn from_channels(channels: u16) -> Self {
        match channels {
            1 => Self::Mono,
            2 => Self::Stereo,
            3 => Self::Surround30,
            4 => Self::Surround40,
            6 => Self::Surround51,
            8 => Self::Surround71,
            other => Self::Custom(other),
        }
    }
}

/// Supported audio sample formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleFormat {
    F32Planar,
    F32Interleaved,
    S16Planar,
    S16Interleaved,
    S24Planar,
    S24Interleaved,
    S32Planar,
    S32Interleaved,
}

impl SampleFormat {
    pub const fn is_planar(&self) -> bool {
        matches!(
            self,
            Self::F32Planar | Self::S16Planar | Self::S24Planar | Self::S32Planar
        )
    }

    pub const fn bytes_per_sample(&self) -> usize {
        match self {
            Self::S16Planar | Self::S16Interleaved => 2,
            Self::S24Planar | Self::S24Interleaved => 3,
            Self::F32Planar | Self::F32Interleaved | Self::S32Planar | Self::S32Interleaved => 4,
        }
    }
}

/// Generic trait for accessing audio samples in planar `f32` representation.
pub trait AudioBufferRef {
    fn channels(&self) -> usize;
    fn frames(&self) -> usize;
    fn sample_rate(&self) -> u32;
    /// Channel slice in planar [-1.0, 1.0] float range.
    fn channel(&self, ch: usize) -> Option<&[f32]>;
}

/// Owned planar `f32` audio buffer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioBuffer {
    channels: Vec<Vec<f32>>,
    sample_rate: u32,
    frames: usize,
}

impl AudioBuffer {
    /// Creates an empty audio buffer.
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        Self {
            channels: vec![Vec::new(); channels],
            sample_rate,
            frames: 0,
        }
    }

    /// Creates an audio buffer with preallocated silence.
    pub fn silence(channels: usize, frames: usize, sample_rate: u32) -> Self {
        Self {
            channels: vec![vec![0.0f32; frames]; channels],
            sample_rate,
            frames,
        }
    }

    /// Constructs an audio buffer from existing channel vectors.
    pub fn from_channels(channels: Vec<Vec<f32>>, sample_rate: u32) -> Self {
        let frames = channels.first().map(|c| c.len()).unwrap_or(0);
        Self {
            channels,
            sample_rate,
            frames,
        }
    }

    /// Access mutable slice of samples for a specific channel.
    pub fn channel_mut(&mut self, ch: usize) -> Option<&mut [f32]> {
        self.channels.get_mut(ch).map(|c| c.as_mut_slice())
    }

    /// Interleaves planar samples into a contiguous destination vector.
    pub fn interleave_into(&self, dest: &mut Vec<f32>) {
        dest.clear();
        dest.reserve(self.channels.len().saturating_mul(self.frames));
        for frame_idx in 0..self.frames {
            for ch in &self.channels {
                dest.push(*ch.get(frame_idx).unwrap_or(&0.0f32));
            }
        }
    }
}

impl AudioBufferRef for AudioBuffer {
    fn channels(&self) -> usize {
        self.channels.len()
    }

    fn frames(&self) -> usize {
        self.frames
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channel(&self, ch: usize) -> Option<&[f32]> {
        self.channels.get(ch).map(|c| c.as_slice())
    }
}
