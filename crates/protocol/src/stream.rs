//! Universal Stream, Packet, Frame, and Timestamp Contracts.
//!
//! Provides codec- and container-agnostic abstractions for linear media processing:
//! - Exact rational timebases (`Rational`, `Timebase`)
//! - Uncompressed frame representation (`VideoFrame`, `AudioFrame`, `MediaFrame`)
//! - Compressed bitstream packet representation (`MediaPacket`)
//! - Monotonic stream scheduling and timestamp synchronization primitives.

use serde::{Deserialize, Serialize};

/// Exact rational number representing timebase or frame rate (`num / den`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rational {
    pub num: u32,
    pub den: u32,
}

impl Rational {
    /// Creates a new rational fraction, normalizing zero denominator to 1.
    pub const fn new(num: u32, den: u32) -> Self {
        Self {
            num,
            den: if den == 0 { 1 } else { den },
        }
    }

    /// Common rational timebases.
    pub const HZ_48000: Rational = Rational::new(1, 48_000);
    pub const HZ_44100: Rational = Rational::new(1, 44_100);
    pub const HZ_96000: Rational = Rational::new(1, 96_000);
    pub const FPS_24: Rational = Rational::new(1, 24);
    pub const FPS_23_976: Rational = Rational::new(1001, 24_000);
    pub const FPS_25: Rational = Rational::new(1, 25);
    pub const FPS_29_97: Rational = Rational::new(1001, 30_000);
    pub const FPS_30: Rational = Rational::new(1, 30);
    pub const FPS_50: Rational = Rational::new(1, 50);
    pub const FPS_59_94: Rational = Rational::new(1001, 60_000);
    pub const FPS_60: Rational = Rational::new(1, 60);

    /// Converts a timestamp from one rational timebase to another without precision loss.
    pub fn rescale(val: i64, from: Rational, to: Rational) -> i64 {
        if from == to || from.num == 0 || from.den == 0 || to.num == 0 || to.den == 0 {
            return val;
        }
        // val * (from.num / from.den) / (to.num / to.den)
        // = val * from.num * to.den / (from.den * to.num)
        let num_prod = (val as i128)
            .saturating_mul(from.num as i128)
            .saturating_mul(to.den as i128);
        let den_prod = (from.den as i128).saturating_mul(to.num as i128);
        if den_prod == 0 {
            0
        } else {
            (num_prod / den_prod) as i64
        }
    }

    /// Converts timestamp value in this timebase into seconds.
    pub fn to_seconds(self, val: i64) -> f64 {
        if self.den == 0 {
            0.0
        } else {
            val as f64 * (self.num as f64 / self.den as f64)
        }
    }

    /// Converts seconds into timestamp units of this timebase.
    pub fn from_seconds(self, secs: f64) -> i64 {
        if self.num == 0 {
            0
        } else {
            (secs * (self.den as f64 / self.num as f64)).round() as i64
        }
    }
}

/// Pixel format representation for uncompressed video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Yuv420p,
    Yuv422p,
    Yuv444p,
    Rgba8,
    Bgra8,
}

/// Uncompressed 8-bit planar YUV 4:2:0 video picture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    pub y_stride: usize,
    pub uv_stride: usize,
    pub pts: i64,
    pub timebase: Rational,
    pub is_keyframe: bool,
}

impl VideoFrame {
    /// Creates a black 4:2:0 frame with specified dimensions.
    pub fn black(width: u32, height: u32, pts: i64, timebase: Rational) -> Self {
        let y_len = (width as usize) * (height as usize);
        let uv_w = (width as usize).div_ceil(2);
        let uv_h = (height as usize).div_ceil(2);
        let uv_len = uv_w * uv_h;

        Self {
            width,
            height,
            y: vec![16u8; y_len],   // Studio black Y=16
            u: vec![128u8; uv_len], // Neutral chroma U=128
            v: vec![128u8; uv_len], // Neutral chroma V=128
            y_stride: width as usize,
            uv_stride: uv_w,
            pts,
            timebase,
            is_keyframe: true,
        }
    }

    /// Calculates presentation time in seconds.
    pub fn pts_seconds(&self) -> f64 {
        self.timebase.to_seconds(self.pts)
    }
}

/// Uncompressed planar audio buffer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioFrame {
    /// Normalized planar audio channels: `samples[ch][sample_idx]`.
    pub samples: Vec<Vec<f32>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub pts: i64,
    pub timebase: Rational,
}

impl AudioFrame {
    /// Creates an empty audio frame for given parameters.
    pub fn silence(sample_rate: u32, channels: u16, frame_count: usize, pts: i64) -> Self {
        let ch_vec = vec![vec![0.0f32; frame_count]; channels as usize];
        Self {
            samples: ch_vec,
            sample_rate,
            channels,
            pts,
            timebase: Rational::new(1, sample_rate),
        }
    }

    /// Number of audio frames per channel.
    pub fn frame_count(&self) -> usize {
        self.samples.first().map_or(0, |ch| ch.len())
    }

    /// Presentation time in seconds.
    pub fn pts_seconds(&self) -> f64 {
        self.timebase.to_seconds(self.pts)
    }
}

/// Unified uncompressed media frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MediaFrame {
    Video(VideoFrame),
    Audio(AudioFrame),
}

/// Compressed bitstream access unit emitted by an encoder or container demuxer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaPacket {
    /// Stream index in the multiplexer / container (e.g. 0 = video, 1 = audio).
    pub stream_index: usize,
    /// Raw compressed payload (Annex B NAL units for H.264, raw access unit for AAC).
    pub data: Vec<u8>,
    /// Presentation timestamp in stream timebase.
    pub pts: i64,
    /// Decode timestamp in stream timebase.
    pub dts: i64,
    /// Duration of packet in stream timebase ticks.
    pub duration: u64,
    /// Stream timebase.
    pub timebase: Rational,
    /// True if packet is a random access point / keyframe (IDR frame, audio sample).
    pub is_keyframe: bool,
}

impl MediaPacket {
    /// Composition time offset (`pts - dts`).
    pub fn composition_offset(&self) -> i64 {
        self.pts.saturating_sub(self.dts)
    }

    /// Presentation timestamp in seconds.
    pub fn pts_seconds(&self) -> f64 {
        self.timebase.to_seconds(self.pts)
    }

    /// Decode timestamp in seconds.
    pub fn dts_seconds(&self) -> f64 {
        self.timebase.to_seconds(self.dts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_rescale_exact() {
        // 24 fps frame 48 -> 2.0s -> 96,000 samples at 48kHz
        let frame_ts = 48i64;
        let audio_samples = Rational::rescale(frame_ts, Rational::FPS_24, Rational::HZ_48000);
        assert_eq!(audio_samples, 96_000);

        // Roundtrip back to 24 fps
        let roundtrip_frames =
            Rational::rescale(audio_samples, Rational::HZ_48000, Rational::FPS_24);
        assert_eq!(roundtrip_frames, 48);

        // 23.976 fps duration conversion
        let secs = Rational::FPS_24.to_seconds(24);
        assert!((secs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn video_frame_black_properties() {
        let frame = VideoFrame::black(1920, 1080, 0, Rational::FPS_24);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.y.len(), 1920 * 1080);
        assert_eq!(frame.u.len(), 960 * 540);
        assert_eq!(frame.v.len(), 960 * 540);
        assert_eq!(frame.y[0], 16);
        assert_eq!(frame.u[0], 128);
        assert_eq!(frame.v[0], 128);
    }
}
