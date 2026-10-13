//! Media stream descriptors, codec parameters, and compressed packets.

use serde::{Deserialize, Serialize};

use crate::audio::{ChannelLayout, SampleFormat};
use crate::time::{MediaTime, Rational};
use crate::video::PixelFormat;

/// Classification of media elementary stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamKind {
    Audio,
    Video,
    Data,
    Subtitle,
}

/// Detailed descriptor for an audio elementary stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStreamDescriptor {
    pub stream_index: usize,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub channel_layout: ChannelLayout,
    pub sample_format: SampleFormat,
    pub bitrate_bps: Option<u32>,
    pub extra_data: Vec<u8>,
}

/// Detailed descriptor for a video elementary stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStreamDescriptor {
    pub stream_index: usize,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub frame_rate: Rational,
    pub bitrate_bps: Option<u32>,
    pub extra_data: Vec<u8>,
}

/// Codec identification and initialization parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodecParameters {
    pub fourcc: [u8; 4],
    pub extra_data: Vec<u8>,
}

/// Compressed elementary stream packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Packet {
    pub stream_index: usize,
    pub time: MediaTime,
    pub is_keyframe: bool,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(
        stream_index: usize,
        pts: i64,
        dts: i64,
        duration: u64,
        timebase: Rational,
        is_keyframe: bool,
        data: Vec<u8>,
    ) -> Self {
        Self {
            stream_index,
            time: MediaTime::with_dts(pts, dts, duration, timebase),
            is_keyframe,
            data,
        }
    }
}
