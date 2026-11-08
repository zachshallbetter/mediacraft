//! The crate's error type.

use crate::Format;

/// Everything that can go wrong reading, converting or writing an image.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The leading bytes match no format this crate knows.
    #[error("unrecognised image format")]
    UnknownFormat,
    /// The format is recognised but this crate cannot decode its pixels.
    #[error("{0} pixels cannot be decoded: {1}")]
    Unsupported(Format, &'static str),
    /// The file's structure is damaged or cut short.
    #[error("malformed {0} data: {1}")]
    Malformed(Format, String),
    /// The image is larger than [`crate::Limits::max_pixels`] allows.
    #[error("image is {width}×{height}, over the {max_pixels}-pixel limit")]
    TooLarge {
        /// Stored width.
        width: u32,
        /// Stored height.
        height: u32,
        /// The limit that refused it.
        max_pixels: u64,
    },
    /// The input is larger than [`crate::Limits::max_input_bytes`] allows.
    #[error("input is {size} bytes, over the {max}-byte limit")]
    InputTooLarge {
        /// Input size in bytes.
        size: u64,
        /// The limit that refused it.
        max: u64,
    },
    /// A pixel decoder rejected the data.
    #[error("{0} decode failed: {1}")]
    Decode(Format, String),
    /// An encoder rejected the image.
    #[error("{0} encode failed: {1}")]
    Encode(Format, String),
    /// A caller argument does not fit the operation.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Reading a file failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
