//! Clean-room Apple ProRes video encoding adapter (Milestone 10).
//!
//! Encodes video frames into SMPTE RDD 36 compliant ProRes bitstreams
//! using `filmcraft-prores`.

use filmcraft_prores::{ChromaFormat, Encoder, Frame as ProResFrame, Profile};
use mediacraft_protocol::stream::VideoFrame;

/// Errors produced during ProRes encoding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProResError {
    #[error("ProRes encoding error: {0}")]
    Encode(String),
}

/// ProRes encoder adapter converting universal `VideoFrame` buffers to ProRes 422 frames.
pub struct ProResEncoderAdapter {
    encoder: Encoder,
    width: u32,
    height: u32,
}

impl ProResEncoderAdapter {
    pub fn new(width: u32, height: u32) -> Self {
        let encoder = Encoder::new(Profile::Standard, width, height);
        Self {
            encoder,
            width,
            height,
        }
    }

    /// Converts a YUV420p `VideoFrame` (8-bit) to a 10-bit YUV422 `ProResFrame` and encodes it.
    pub fn encode_frame(&mut self, frame: &VideoFrame) -> Result<Vec<u8>, ProResError> {
        let w = self.width as usize;
        let h = self.height as usize;
        let cw = ChromaFormat::Yuv422.chroma_width(self.width) as usize;

        // Convert 8-bit Y to 10-bit Y (scale by << 2)
        let mut y10 = Vec::with_capacity(w * h);
        for &sample in &frame.y {
            y10.push((sample as u16) << 2);
        }
        if y10.len() < w * h {
            y10.resize(w * h, 64); // 16 << 2
        }

        // Interpolate 4:2:0 UV to 4:2:2 (double vertical lines for chroma)
        let mut cb10 = Vec::with_capacity(cw * h);
        let mut cr10 = Vec::with_capacity(cw * h);

        let uv_w = frame.uv_stride;
        let uv_h = frame.height as usize / 2;

        for y in 0..h {
            let uv_row = (y / 2).min(uv_h.saturating_sub(1));
            for x in 0..cw {
                let uv_col = x.min(uv_w.saturating_sub(1));
                let uv_idx = uv_row * uv_w + uv_col;
                let u_val = frame.u.get(uv_idx).copied().unwrap_or(128) as u16;
                let v_val = frame.v.get(uv_idx).copied().unwrap_or(128) as u16;
                cb10.push(u_val << 2);
                cr10.push(v_val << 2);
            }
        }

        let mut prores_frame =
            ProResFrame::new(self.width, self.height, ChromaFormat::Yuv422, 10, false);
        prores_frame.y = y10;
        prores_frame.cb = cb10;
        prores_frame.cr = cr10;

        self.encoder
            .encode(&prores_frame)
            .map_err(|e| ProResError::Encode(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prores_encode_black_frame() {
        let mut adapter = ProResEncoderAdapter::new(64, 64);
        let frame = VideoFrame::black(64, 64, 0, mediacraft_protocol::stream::Rational::FPS_24);

        let encoded_bytes = adapter.encode_frame(&frame).unwrap();
        assert!(!encoded_bytes.is_empty());
        // ProRes frames start with a 4-byte frame size followed by 'icpf' (0x69, 0x63, 0x70, 0x66)
        assert_eq!(&encoded_bytes[4..8], b"icpf");
    }
}
