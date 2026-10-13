//! H.264 / AVC Video Encoder Adapter
//!
//! Adapts the clean-room `craft-h264enc` pure-Rust engine into MediaCraft's pipeline.
//! Provides progressive 8-bit 4:2:0 SDR encoding with length-prefixed MP4 samples,
//! parameter set (SPS/PPS) extraction, and presentation timestamp preservation.

use craft_h264enc::{Encoder, EncoderConfig, PacketFormat, RateControl, YuvFrame};
use mediacraft_protocol::stream::{MediaPacket, Rational, VideoFrame};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum H264AdapterError {
    #[error("H.264 encoder configuration error: {0}")]
    Config(#[from] craft_h264enc::Error),
    #[error("invalid video dimensions: {0}x{1}")]
    InvalidDimensions(u32, u32),
}

/// H.264 Video Encoder Adapter wrapping pure-Rust `craft-h264enc`.
pub struct H264EncoderAdapter {
    encoder: Encoder,
    width: u32,
    height: u32,
    timebase: Rational,
    stream_index: usize,
}

impl H264EncoderAdapter {
    /// Creates a new H.264 encoder configured for MP4 muxing with length-prefixed NAL units.
    pub fn new(
        width: u32,
        height: u32,
        fps_num: u32,
        fps_den: u32,
        crf: Option<f32>,
        bitrate_kbps: Option<u32>,
        stream_index: usize,
    ) -> Result<Self, H264AdapterError> {
        let mut cfg = EncoderConfig::new(width, height, fps_num, fps_den);
        cfg.format = PacketFormat::LengthPrefixed;

        if let Some(kbps) = bitrate_kbps {
            cfg.rate = RateControl::Cbr { kbps };
        } else if let Some(c) = crf {
            cfg.rate = RateControl::Crf(c);
        } else {
            cfg.rate = RateControl::Crf(23.0);
        }

        let encoder = Encoder::new(cfg)?;
        let timebase = Rational::new(fps_den, fps_num);

        Ok(Self {
            encoder,
            width,
            height,
            timebase,
            stream_index,
        })
    }

    /// Video frame width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Video frame height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the AVCDecoderConfigurationRecord (`avcC`) box payload for MP4 `stsd` / `avc1`.
    pub fn avcc(&self) -> Vec<u8> {
        self.encoder.avcc()
    }

    /// Returns the raw SPS and PPS NAL units.
    pub fn sps_pps(&self) -> (Vec<u8>, Vec<u8>) {
        self.encoder.sps_pps()
    }

    /// Returns the stream index assigned to this video track.
    pub fn stream_index(&self) -> usize {
        self.stream_index
    }

    /// Encodes an uncompressed video frame into compressed MP4 packets.
    pub fn encode(&mut self, frame: &VideoFrame) -> Result<Vec<MediaPacket>, H264AdapterError> {
        let yuv = YuvFrame {
            y: &frame.y,
            u: &frame.u,
            v: &frame.v,
            y_stride: frame.y_stride,
            uv_stride: frame.uv_stride,
        };

        let raw_packets = self.encoder.encode(&yuv, frame.pts)?;
        let mut packets = Vec::with_capacity(raw_packets.len());

        for pkt in raw_packets {
            packets.push(MediaPacket {
                stream_index: self.stream_index,
                data: pkt.data,
                pts: pkt.pts,
                dts: pkt.dts,
                duration: 1, // 1 tick in fps timebase
                timebase: self.timebase,
                is_keyframe: pkt.keyframe,
            });
        }

        Ok(packets)
    }

    /// Flushes any pending lookahead or reordered B-frames from the encoder.
    pub fn flush(&mut self) -> Result<Vec<MediaPacket>, H264AdapterError> {
        let raw_packets = self.encoder.flush();
        let mut packets = Vec::with_capacity(raw_packets.len());

        for pkt in raw_packets {
            packets.push(MediaPacket {
                stream_index: self.stream_index,
                data: pkt.data,
                pts: pkt.pts,
                dts: pkt.dts,
                duration: 1,
                timebase: self.timebase,
                is_keyframe: pkt.keyframe,
            });
        }

        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h264_encode_frames() {
        let mut encoder = H264EncoderAdapter::new(320, 240, 24, 1, Some(28.0), None, 0).unwrap();
        assert!(!encoder.avcc().is_empty());

        let frame = VideoFrame::black(320, 240, 0, Rational::FPS_24);
        let packets = encoder.encode(&frame).unwrap();
        let flushed = encoder.flush().unwrap();

        let total = packets.len() + flushed.len();
        assert!(total >= 1, "Must produce at least 1 video packet");
    }
}
