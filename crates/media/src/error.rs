//! Media error hierarchy.

/// Unified media error hierarchy across all Craft media crates.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MediaError {
    #[error("unsupported format or codec parameter: {0}")]
    Unsupported(String),

    #[error("malformed media bitstream or container header: {0}")]
    Malformed(String),

    #[error("buffer sizing or resource limit exceeded: {0}")]
    ResourceExceeded(String),

    #[error("encoder failed: {0}")]
    EncodeFailed(String),

    #[error("decoder failed: {0}")]
    DecodeFailed(String),

    #[error("underlying I/O error: {0}")]
    Io(String),

    #[error("no {0} stream found")]
    NoStream(&'static str),

    #[error("media offline: {0}")]
    Offline(String),

    #[error("request was cancelled")]
    Cancelled,
}

impl From<std::io::Error> for MediaError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, MediaError>;
