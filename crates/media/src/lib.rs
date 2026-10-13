//! Shared Media Infrastructure (`craft-media`)
//!
//! Provides fundamental media representations, audio/video buffer views,
//! rational timebase operations, elementary stream descriptors, and error
//! taxonomy shared across all Craft media crates.

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

pub mod audio;
pub mod descriptor;
pub mod error;
pub mod time;
pub mod video;

pub use audio::{AudioBuffer, AudioBufferRef, ChannelLayout, SampleFormat};
pub use descriptor::{
    AudioStreamDescriptor, CodecParameters, Packet, StreamKind, VideoStreamDescriptor,
};
pub use error::{MediaError, Result};
pub use time::{MediaTime, Rational};
pub use video::{PixelFormat, VideoFrame, VideoFrameRef};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_buffer_interleave() {
        let mut buf = AudioBuffer::silence(2, 4, 48000);
        if let Some(ch0) = buf.channel_mut(0) {
            ch0[0] = 0.1;
            ch0[1] = 0.2;
            ch0[2] = 0.3;
            ch0[3] = 0.4;
        }
        if let Some(ch1) = buf.channel_mut(1) {
            ch1[0] = -0.1;
            ch1[1] = -0.2;
            ch1[2] = -0.3;
            ch1[3] = -0.4;
        }

        let mut interleaved = Vec::new();
        buf.interleave_into(&mut interleaved);
        assert_eq!(interleaved.len(), 8);
        assert_eq!(interleaved[0], 0.1);
        assert_eq!(interleaved[1], -0.1);
        assert_eq!(interleaved[2], 0.2);
        assert_eq!(interleaved[3], -0.2);
    }

    #[test]
    fn test_rational_rescale() {
        let tb_24 = Rational::new(1, 24);
        let tb_48k = Rational::new(1, 48000);
        let frames_24 = 24i64; // 1 second
        let audio_samples = tb_24.rescale(frames_24, tb_48k);
        assert_eq!(audio_samples, 48000);
    }

    #[test]
    fn test_signed_media_time() {
        let t = MediaTime::new(-2400, 2400, Rational::new(1, 48000));
        assert_eq!(t.pts, -2400);
        assert_eq!(t.pts_seconds(), -0.05);
        assert_eq!(t.duration_seconds(), 0.05);
    }
}
