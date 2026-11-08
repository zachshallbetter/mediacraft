//! Pixel buffers and resource limits.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// An 8-bit RGBA image, rows top to bottom, straight (not premultiplied) alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8Image {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Rgba8Image {
    /// Wrap a buffer of exactly `width × height × 4` bytes.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        let expected = rgba_len(width, height)?;
        if data.len() != expected {
            return Err(Error::InvalidArgument(format!(
                "RGBA buffer holds {} bytes, {width}×{height} needs {expected}",
                data.len()
            )));
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// An image of one colour.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Result<Self> {
        let len = rgba_len(width, height)?;
        let mut data = Vec::new();
        data.try_reserve_exact(len)
            .map_err(|e| Error::InvalidArgument(e.to_string()))?;
        for _ in 0..len / 4 {
            data.extend_from_slice(&rgba);
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, `width × height × 4` bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable pixels.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// The buffer, consuming the image.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// The pixel at `(x, y)`, `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.data.get(i..i + 4)?;
        Some([p[0], p[1], p[2], p[3]])
    }

    /// Whether every pixel is fully opaque.
    pub fn is_opaque(&self) -> bool {
        self.data.as_chunks::<4>().0.iter().all(|p| p[3] == 255)
    }
}

/// `width × height × 4`, refusing zero sizes and overflow.
pub(crate) fn rgba_len(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(Error::InvalidArgument(format!(
            "{width}×{height} has no pixels"
        )));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| Error::InvalidArgument(format!("{width}×{height} overflows a buffer")))
}

/// Resource ceilings for untrusted input. Every decode checks the stored size against these
/// before it allocates pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// Largest stored `width × height` a decode accepts (default 100 megapixels).
    pub max_pixels: u64,
    /// Largest single allocation a decoder may make (default 1 GiB).
    pub max_alloc: u64,
    /// Largest input file in bytes (default 512 MiB).
    pub max_input_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pixels: 100_000_000,
            max_alloc: 1 << 30,
            max_input_bytes: 512 << 20,
        }
    }
}

impl Limits {
    /// Refuse an input larger than [`Limits::max_input_bytes`].
    pub(crate) fn check_input(&self, len: usize) -> Result<()> {
        let size = len as u64;
        if size > self.max_input_bytes {
            return Err(Error::InputTooLarge {
                size,
                max: self.max_input_bytes,
            });
        }
        Ok(())
    }

    /// Refuse a stored size larger than [`Limits::max_pixels`].
    pub(crate) fn check_size(&self, width: u32, height: u32) -> Result<()> {
        let n = u64::from(width) * u64::from(height);
        if n > self.max_pixels {
            return Err(Error::TooLarge {
                width,
                height,
                max_pixels: self.max_pixels,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_refuses_wrong_lengths_and_empty_sizes() {
        assert!(Rgba8Image::new(2, 2, vec![0; 16]).is_ok());
        assert!(Rgba8Image::new(2, 2, vec![0; 15]).is_err());
        assert!(Rgba8Image::new(0, 2, vec![]).is_err());
        assert!(Rgba8Image::filled(u32::MAX, u32::MAX, [0; 4]).is_err());
    }

    #[test]
    fn pixel_and_opacity() {
        let mut img = Rgba8Image::filled(3, 2, [10, 20, 30, 255]).unwrap();
        assert_eq!(img.pixel(2, 1), Some([10, 20, 30, 255]));
        assert_eq!(img.pixel(3, 0), None);
        assert!(img.is_opaque());
        img.data_mut()[3] = 0;
        assert!(!img.is_opaque());
    }

    #[test]
    fn limits_refuse_oversized_input() {
        let l = Limits {
            max_pixels: 10,
            max_alloc: 1,
            max_input_bytes: 4,
        };
        assert!(l.check_size(3, 3).is_ok());
        assert!(matches!(l.check_size(4, 3), Err(Error::TooLarge { .. })));
        assert!(matches!(l.check_input(5), Err(Error::InputTooLarge { .. })));
    }
}
