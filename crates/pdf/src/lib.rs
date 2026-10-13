#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod content;
pub mod cos;
pub mod crypt;
pub mod filters;
pub mod inspect;

pub use cos::{
    Bytes, CosError, Dict, Document, Name, ObjRef, Object, PdfString, SaveOptions, Stream,
    write_full, write_incremental,
};
pub use filters::{Filter, FilterError};
pub use inspect::{PdfDiagnosticReport, PdfInspector};
