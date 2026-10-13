//! MediaCraft Delivery & Ingestion Receipts using craft-atomic
//!
//! Enforces crash-safe persistence ordering (sync_file < rename < sync_dir)
//! so external readers never observe partially written receipts or metadata sidecars.

use craft_atomic::atomic_write;
use std::path::Path;

/// Writes delivery receipt or metadata sidecar atomically using craft-atomic.
pub fn write_delivery_receipt(path: &Path, receipt_data: &str) -> Result<(), std::io::Error> {
    atomic_write(path, receipt_data.as_bytes())
}
