//! Video frame representations, pixel formats, and plane accessors.

use serde::{Deserialize, Serialize};

/// Standard video pixel formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelFormat {
    Yuv420p,
    Yuv422p,
    Yuv444p,
    Rgba,
    Bgra,
}

impl PixelFormat {
    pub const fn plane_count(&self) -> usize {
        match self {
            Self::Yuv420p | Self::Yuv422p | Self::Yuv444p => 3,
            Self::Rgba | Self::Bgra => 1,
        }
    }
}

/// Generic trait for accessing video frame dimensions and raw pixel planes.
pub trait VideoFrameRef {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> PixelFormat;
    fn plane(&self, idx: usize) -> Option<&[u8]>;
}

/// Owned video frame buffer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub planes: Vec<Vec<u8>>,
}

impl VideoFrame {
    /// Creates a black frame in YUV420p.
    pub fn black_yuv420p(width: u32, height: u32) -> Self {
        let y_len = (width as usize).saturating_mul(height as usize);
        let uv_w = (width as usize).div_ceil(2);
        let uv_h = (height as usize).div_ceil(2);
        let uv_len = uv_w.saturating_mul(uv_h);

        let y = vec![16u8; y_len]; // Studio black
        let u = vec![128u8; uv_len]; // Neutral chroma
        let v = vec![128u8; uv_len];

        Self {
            width,
            height,
            format: PixelFormat::Yuv420p,
            planes: vec![y, u, v],
        }
    }
}

impl VideoFrameRef for VideoFrame {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn format(&self) -> PixelFormat {
        self.format
    }

    fn plane(&self, idx: usize) -> Option<&[u8]> {
        self.planes.get(idx).map(|p| p.as_slice())
    }
}
