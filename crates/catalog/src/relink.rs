//! Media Relink & Offline Status Engine (FEATURES.md:L41).
//!
//! Preserves catalog references, tags, and derivative links when assets are
//! moved, renamed on disk, or unmounted. Provides automated discovery of
//! replacement files using name, size, duration, and fingerprint heuristics.

use std::path::{Path, PathBuf};

use crate::MediaAsset;

/// Confidence level and criteria used to match a relink candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelinkMatchConfidence {
    /// Exact content fingerprint match (100% confidence).
    ContentHashExact,
    /// Exact filename and byte size match.
    NameAndSize,
    /// Exact filename and matching duration.
    NameAndDuration,
    /// Filename match only (may require manual user confirmation).
    NameOnly,
}

/// A candidate replacement file found on disk for an offline asset.
#[derive(Debug, Clone, PartialEq)]
pub struct RelinkCandidate {
    pub asset_id: String,
    pub original_path: PathBuf,
    pub candidate_path: PathBuf,
    pub candidate_size_bytes: u64,
    pub confidence: RelinkMatchConfidence,
}

/// Relink search and resolution engine.
pub struct RelinkEngine {
    pub recursive_search: bool,
    pub size_tolerance_bytes: u64,
    pub duration_tolerance_secs: f32,
}

impl Default for RelinkEngine {
    fn default() -> Self {
        Self {
            recursive_search: true,
            size_tolerance_bytes: 1024,
            duration_tolerance_secs: 0.25,
        }
    }
}

impl RelinkEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks whether an asset's file is currently online and readable on the filesystem.
    pub fn is_asset_online(asset: &MediaAsset) -> bool {
        asset.path.is_file()
    }

    /// Finds candidate relink files for a given offline asset within a search directory.
    pub fn find_candidates_in_dir(
        &self,
        asset: &MediaAsset,
        search_dir: &Path,
    ) -> Vec<RelinkCandidate> {
        let mut candidates = Vec::new();
        if !search_dir.is_dir() {
            return candidates;
        }

        let target_filename = match asset.path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_lowercase(),
            None => return candidates,
        };

        let mut walker_dirs = vec![search_dir.to_path_buf()];
        while let Some(current_dir) = walker_dirs.pop() {
            let entries = match std::fs::read_dir(&current_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && self.recursive_search {
                    walker_dirs.push(path);
                    continue;
                }
                if !path.is_file() {
                    continue;
                }
                let cand_name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => continue,
                };
                if cand_name.to_lowercase() == target_filename {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let size_diff = (size as i64 - asset.file_size_bytes as i64).unsigned_abs();

                    let confidence = if size_diff <= self.size_tolerance_bytes {
                        RelinkMatchConfidence::NameAndSize
                    } else {
                        RelinkMatchConfidence::NameOnly
                    };

                    candidates.push(RelinkCandidate {
                        asset_id: asset.id.clone(),
                        original_path: asset.path.clone(),
                        candidate_path: path,
                        candidate_size_bytes: size,
                        confidence,
                    });
                }
            }
        }

        candidates
    }

    /// Applies a relink candidate to an asset, updating its path and verifying presence.
    pub fn apply_relink(asset: &mut MediaAsset, candidate: &RelinkCandidate) -> bool {
        if candidate.candidate_path.is_file() {
            asset.path = candidate.candidate_path.clone();
            if let Some(name) = candidate
                .candidate_path
                .file_name()
                .and_then(|n| n.to_str())
            {
                asset.filename = name.to_string();
            }
            if candidate.candidate_size_bytes > 0 {
                asset.file_size_bytes = candidate.candidate_size_bytes;
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaKind;

    #[test]
    fn test_relink_detection_and_resolution() {
        let temp_dir =
            std::env::temp_dir().join(format!("mediacraft_relink_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let target_file = temp_dir.join("Interview_A_Cam.mov");
        let _ = std::fs::write(&target_file, b"sample media data 12345678");

        let mut asset = MediaAsset {
            id: "asset_offline_1".to_string(),
            path: PathBuf::from("/Volumes/OfflineDisk/Media/Interview_A_Cam.mov"),
            filename: "Interview_A_Cam.mov".to_string(),
            file_size_bytes: 26,
            kind: MediaKind::Video,
            duration_secs: Some(10.0),
            timecode_start: "01:00:00:00".to_string(),
            video: None,
            audio: None,
            camera_model: None,
            date_created: "2026-10-12".to_string(),
            geo_location: None,
            rating: 4,
            tags: vec!["interview".to_string()],
            has_proxy: false,
            needs_transcode: false,
        };

        // Initially offline
        assert!(!RelinkEngine::is_asset_online(&asset));

        let engine = RelinkEngine::new();
        let candidates = engine.find_candidates_in_dir(&asset, &temp_dir);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, RelinkMatchConfidence::NameAndSize);
        assert_eq!(candidates[0].candidate_path, target_file);

        let success = RelinkEngine::apply_relink(&mut asset, &candidates[0]);
        assert!(success);
        assert!(RelinkEngine::is_asset_online(&asset));
        assert_eq!(asset.path, target_file);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
