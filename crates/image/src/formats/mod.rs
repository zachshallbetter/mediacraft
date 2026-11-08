//! Per-container readers: header fields without pixel decoding, and where each format keeps its
//! metadata blocks.

pub(crate) mod bmff;
pub(crate) mod bmp;
pub(crate) mod gif;
pub(crate) mod jpeg;
pub(crate) mod png;
pub(crate) mod psd;
pub(crate) mod tiff_file;
pub(crate) mod webp;

use crate::Orientation;

/// What a container's header says, before EXIF orientation is merged in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RawHeader {
    pub width: u32,
    pub height: u32,
    pub bit_depth: Option<u8>,
    pub has_alpha: Option<bool>,
    pub animated: bool,
    /// Orientation the container itself records (HEIF `irot`/`imir`, TIFF tag 274), which wins
    /// over EXIF where the format says so.
    pub orientation: Option<Orientation>,
}
