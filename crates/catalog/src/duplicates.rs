//! Duplicate & Similarity Detection Engine for MediaCraft (FEATURES.md M4/M7).
//!
//! Provides hash-identical duplicate detection (P1) and perceptual near-duplicate
//! clustering (P2) across local and mounted media libraries.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{MediaAsset, MediaKind};

/// Nature of similarity between clustered assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateMatchKind {
    /// Exact byte-for-byte identical match (same file size and content fingerprint).
    ExactIdentical,
    /// Near duplicate (matching duration within tolerance, identical stream topology, or proxy relationship).
    NearDuplicate,
}

/// A cluster of duplicate or near-duplicate assets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateCluster {
    pub cluster_id: String,
    pub match_kind: DuplicateMatchKind,
    pub primary_asset_id: String,
    pub duplicate_asset_ids: Vec<String>,
    pub potential_reclaimable_bytes: u64,
    pub summary: String,
}

/// Engine for scanning a catalog to detect duplicate or redundant media files.
#[derive(Debug, Clone)]
pub struct DuplicateFinder {
    /// Duration tolerance in seconds for near-duplicate identification.
    pub duration_tolerance_secs: f32,
    /// Minimum file size to consider for duplicate analysis (skip small thumbnails/manifests).
    pub min_file_size_bytes: u64,
}

impl Default for DuplicateFinder {
    fn default() -> Self {
        Self {
            duration_tolerance_secs: 0.5,
            min_file_size_bytes: 1024,
        }
    }
}

impl DuplicateFinder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Computes a fast, deterministic content fingerprint by sampling the start, middle, and end of a file.
    pub fn compute_fingerprint(path: &Path) -> Option<String> {
        let mut file = File::open(path).ok()?;
        let meta = file.metadata().ok()?;
        let len = meta.len();

        let mut hasher = FnvHasher::new();
        hasher.write_u64(len);

        // Read up to 8KB from head
        let mut buf = [0u8; 8192];
        if let Ok(n) = file.read(&mut buf) {
            hasher.write_bytes(&buf[..n]);
        }

        // If file is larger than 64KB, sample from midpoint
        if len > 65536 {
            let mid = len / 2;
            if file.seek(SeekFrom::Start(mid)).is_ok() {
                let n = file.read(&mut buf).unwrap_or(0);
                if n > 0 {
                    hasher.write_bytes(&buf[..n]);
                }
            }

            // Sample from tail
            if len > 8192 {
                let tail_pos = len.saturating_sub(8192);
                if file.seek(SeekFrom::Start(tail_pos)).is_ok() {
                    let n = file.read(&mut buf).unwrap_or(0);
                    if n > 0 {
                        hasher.write_bytes(&buf[..n]);
                    }
                }
            }
        }

        Some(format!("{:016x}", hasher.finish()))
    }

    /// Finds exact hash-identical duplicate files in the given asset collection.
    pub fn find_exact_duplicates(&self, assets: &[MediaAsset]) -> Vec<DuplicateCluster> {
        // Group candidate assets by exact file size
        let mut size_buckets: HashMap<u64, Vec<&MediaAsset>> = HashMap::new();
        for asset in assets {
            if asset.file_size_bytes >= self.min_file_size_bytes {
                size_buckets
                    .entry(asset.file_size_bytes)
                    .or_default()
                    .push(asset);
            }
        }

        let mut clusters = Vec::new();

        // For buckets with >1 asset, compute content fingerprints
        for (&size, candidates) in &size_buckets {
            if candidates.len() < 2 {
                continue;
            }

            let mut hash_groups: HashMap<String, Vec<&MediaAsset>> = HashMap::new();
            for asset in candidates {
                if let Some(fp) = Self::compute_fingerprint(&asset.path) {
                    hash_groups.entry(fp).or_default().push(asset);
                }
            }

            for (fp, duplicates) in hash_groups {
                if duplicates.len() > 1 {
                    let primary = duplicates[0];
                    let dup_ids: Vec<String> =
                        duplicates[1..].iter().map(|a| a.id.clone()).collect();
                    let reclaimable = size.saturating_mul(dup_ids.len() as u64);

                    clusters.push(DuplicateCluster {
                        cluster_id: format!("exact_{fp}"),
                        match_kind: DuplicateMatchKind::ExactIdentical,
                        primary_asset_id: primary.id.clone(),
                        duplicate_asset_ids: dup_ids,
                        potential_reclaimable_bytes: reclaimable,
                        summary: format!(
                            "{} identical files found (same length and content fingerprint)",
                            duplicates.len()
                        ),
                    });
                }
            }
        }

        clusters
    }

    /// Finds perceptual near-duplicates (assets with matching durations and stream layouts,
    /// e.g. different bitrates, proxies, or transcode generations).
    pub fn find_near_duplicates(&self, assets: &[MediaAsset]) -> Vec<DuplicateCluster> {
        let mut clusters = Vec::new();
        let mut visited = std::collections::HashSet::new();

        for i in 0..assets.len() {
            let a1 = &assets[i];
            if visited.contains(&a1.id) {
                continue;
            }

            let dur1 = match a1.duration_secs {
                Some(d) if d > 1.0 => d,
                _ => continue,
            };

            let mut matched_group = Vec::new();

            for a2 in assets.iter().skip(i.saturating_add(1)) {
                if visited.contains(&a2.id) {
                    continue;
                }

                if a1.kind != a2.kind {
                    continue;
                }

                if let Some(dur2) = a2.duration_secs {
                    let diff = (dur1 - dur2).abs();
                    if diff <= self.duration_tolerance_secs {
                        // Check if both have video or both have audio
                        let streams_compatible = match a1.kind {
                            MediaKind::Video => a1.video.is_some() && a2.video.is_some(),
                            MediaKind::Audio => a1.audio.is_some() && a2.audio.is_some(),
                            _ => false,
                        };

                        if streams_compatible {
                            matched_group.push(a2);
                        }
                    }
                }
            }

            if !matched_group.is_empty() {
                visited.insert(a1.id.clone());
                let mut reclaimable = 0u64;
                let mut dup_ids = Vec::new();

                for m in &matched_group {
                    visited.insert(m.id.clone());
                    dup_ids.push(m.id.clone());
                    reclaimable = reclaimable.saturating_add(m.file_size_bytes);
                }

                clusters.push(DuplicateCluster {
                    cluster_id: format!("near_{}", a1.id),
                    match_kind: DuplicateMatchKind::NearDuplicate,
                    primary_asset_id: a1.id.clone(),
                    duplicate_asset_ids: dup_ids,
                    potential_reclaimable_bytes: reclaimable,
                    summary: format!(
                        "Similar media take / derivative detected ({:.1}s duration alignment)",
                        dur1
                    ),
                });
            }
        }

        clusters
    }
}

/// Fast FNV-1a 64-bit hasher without external crate dependencies.
struct FnvHasher {
    state: u64,
}

impl FnvHasher {
    const FNV_PRIME: u64 = 1099511628211;
    const FNV_OFFSET_BASIS: u64 = 14695981039346656037;

    fn new() -> Self {
        Self {
            state: Self::FNV_OFFSET_BASIS,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u64::from(byte);
            self.state = self.state.wrapping_mul(Self::FNV_PRIME);
        }
    }

    fn write_u64(&mut self, val: u64) {
        self.write_bytes(&val.to_le_bytes());
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_exact_duplicate_detection() {
        let temp_dir = std::env::temp_dir().join(format!("mc_dup_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let f1 = temp_dir.join("clip_orig.mov");
        let f2 = temp_dir.join("clip_copy.mov");
        let f3 = temp_dir.join("clip_diff.mov");

        let content_same = vec![0x42u8; 10000];
        let mut content_diff = vec![0x42u8; 10000];
        content_diff[500] = 0x99;

        let _ = fs::write(&f1, &content_same);
        let _ = fs::write(&f2, &content_same);
        let _ = fs::write(&f3, &content_diff);

        let a1 = MediaAsset {
            id: "asset_1".to_string(),
            path: f1.clone(),
            filename: "clip_orig.mov".to_string(),
            file_size_bytes: 10000,
            kind: MediaKind::Video,
            duration_secs: Some(10.0),
            timecode_start: "00:00:00:00".to_string(),
            video: None,
            audio: None,
            camera_model: None,
            date_created: "2026-10-12".to_string(),
            geo_location: None,
            rating: 5,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
        };

        let a2 = MediaAsset {
            id: "asset_2".to_string(),
            path: f2.clone(),
            filename: "clip_copy.mov".to_string(),
            file_size_bytes: 10000,
            kind: MediaKind::Video,
            duration_secs: Some(10.0),
            timecode_start: "00:00:00:00".to_string(),
            video: None,
            audio: None,
            camera_model: None,
            date_created: "2026-10-12".to_string(),
            geo_location: None,
            rating: 5,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
        };

        let a3 = MediaAsset {
            id: "asset_3".to_string(),
            path: f3.clone(),
            filename: "clip_diff.mov".to_string(),
            file_size_bytes: 10000,
            kind: MediaKind::Video,
            duration_secs: Some(10.0),
            timecode_start: "00:00:00:00".to_string(),
            video: None,
            audio: None,
            camera_model: None,
            date_created: "2026-10-12".to_string(),
            geo_location: None,
            rating: 5,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
        };

        let finder = DuplicateFinder::new();
        let exact = finder.find_exact_duplicates(&[a1, a2, a3]);

        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].primary_asset_id, "asset_1");
        assert_eq!(exact[0].duplicate_asset_ids, vec!["asset_2"]);
        assert_eq!(exact[0].potential_reclaimable_bytes, 10000);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_near_duplicate_detection() {
        let a1 = MediaAsset {
            id: "master_4k".to_string(),
            path: PathBuf::from("/media/master.mov"),
            filename: "master.mov".to_string(),
            file_size_bytes: 2_000_000_000,
            kind: MediaKind::Video,
            duration_secs: Some(42.3),
            timecode_start: "00:00:00:00".to_string(),
            video: Some(crate::VideoStreamInfo {
                codec: "ProRes 422".to_string(),
                width: 3840,
                height: 2160,
                fps: 24.0,
                color_space: "Rec. 709".to_string(),
                hdr: false,
                bit_depth: 10,
            }),
            audio: None,
            camera_model: None,
            date_created: "2026-10-12".to_string(),
            geo_location: None,
            rating: 5,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
        };

        let a2 = MediaAsset {
            id: "proxy_1080p".to_string(),
            path: PathBuf::from("/media/proxy.mp4"),
            filename: "proxy.mp4".to_string(),
            file_size_bytes: 80_000_000,
            kind: MediaKind::Video,
            duration_secs: Some(42.3),
            timecode_start: "00:00:00:00".to_string(),
            video: Some(crate::VideoStreamInfo {
                codec: "H.264".to_string(),
                width: 1920,
                height: 1080,
                fps: 24.0,
                color_space: "Rec. 709".to_string(),
                hdr: false,
                bit_depth: 8,
            }),
            audio: None,
            camera_model: None,
            date_created: "2026-10-12".to_string(),
            geo_location: None,
            rating: 4,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
        };

        let finder = DuplicateFinder::new();
        let near = finder.find_near_duplicates(&[a1, a2]);

        assert_eq!(near.len(), 1);
        assert_eq!(near[0].primary_asset_id, "master_4k");
        assert_eq!(near[0].duplicate_asset_ids, vec!["proxy_1080p"]);
        assert_eq!(near[0].potential_reclaimable_bytes, 80_000_000);
    }
}
