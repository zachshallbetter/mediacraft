//! Batch Operations & Safe Renaming Engine for MediaCraft (M4).
//!
//! Provides tokenized filename planning ({name}, {res}, {date}, {seq}), collision preflight,
//! atomic rollback journaling, and multi-asset metadata mutations.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::MediaAsset;

/// Represents a single planned rename item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePlanItem {
    pub asset_id: String,
    pub original_path: PathBuf,
    pub original_filename: String,
    pub new_filename: String,
    pub new_path: PathBuf,
    pub has_collision: bool,
    pub error_message: Option<String>,
}

/// Tokenized batch rename planner.
#[derive(Debug, Clone)]
pub struct BatchRenamePlanner {
    pub pattern: String,
    pub start_index: u32,
}

impl Default for BatchRenamePlanner {
    fn default() -> Self {
        Self {
            pattern: "{name}_{res}_{seq:03}".to_string(),
            start_index: 1,
        }
    }
}

impl BatchRenamePlanner {
    pub fn new(pattern: impl Into<String>, start_index: u32) -> Self {
        Self {
            pattern: pattern.into(),
            start_index,
        }
    }

    /// Evaluates the pattern against an asset to produce the new filename.
    pub fn evaluate_filename(&self, asset: &MediaAsset, seq_num: u32) -> String {
        let stem = asset
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&asset.filename);
        let ext = asset
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let res_str = if let Some(v) = &asset.video {
            if v.width >= 3840 || v.height >= 2160 {
                "4K"
            } else if v.width >= 1920 || v.height >= 1080 {
                "1080p"
            } else if v.width >= 1280 || v.height >= 720 {
                "720p"
            } else {
                "SD"
            }
        } else {
            "Audio"
        };

        // Date token: extract YYYY-MM-DD if available
        let date_str = if asset.date_created.len() >= 10 {
            &asset.date_created[0..10]
        } else {
            "2026-10-12"
        };

        let camera_str = asset.camera_model.as_deref().unwrap_or("Camera");

        let mut out = self.pattern.clone();
        out = out.replace("{name}", stem);
        out = out.replace("{res}", res_str);
        out = out.replace("{date}", date_str);
        out = out.replace("{camera}", camera_str);
        out = out.replace("{seq}", &seq_num.to_string());
        out = out.replace("{seq:02}", &format!("{seq_num:02}"));
        out = out.replace("{seq:03}", &format!("{seq_num:03}"));
        out = out.replace("{seq:04}", &format!("{seq_num:04}"));

        if !ext.is_empty() && !out.ends_with(&format!(".{ext}")) {
            format!("{out}.{ext}")
        } else {
            out
        }
    }

    /// Generates a complete preview plan for the given assets, performing preflight collision detection.
    pub fn plan(&self, assets: &[MediaAsset]) -> Vec<RenamePlanItem> {
        let mut items = Vec::with_capacity(assets.len());
        let mut seen_paths = HashSet::new();

        for (i, asset) in assets.iter().enumerate() {
            let seq_num = self.start_index.saturating_add(i as u32);
            let new_filename = self.evaluate_filename(asset, seq_num);
            let parent = asset.path.parent().unwrap_or_else(|| Path::new("."));
            let new_path = parent.join(&new_filename);

            let mut has_collision = false;
            let mut error_message = None;

            // Check if name is unchanged
            if new_path == asset.path {
                // Identity rename is permitted as no-op
            } else if seen_paths.contains(&new_path) {
                has_collision = true;
                error_message = Some("Duplicate destination path within batch".to_string());
            } else if new_path.exists() {
                has_collision = true;
                error_message = Some("File already exists on filesystem".to_string());
            }

            seen_paths.insert(new_path.clone());

            items.push(RenamePlanItem {
                asset_id: asset.id.clone(),
                original_path: asset.path.clone(),
                original_filename: asset.filename.clone(),
                new_filename,
                new_path,
                has_collision,
                error_message,
            });
        }

        items
    }
}

/// Journal tracking executed renames for safe rollback if any operation fails.
#[derive(Debug, Default, Clone)]
pub struct RollbackJournal {
    /// List of completed (original_path, new_path) moves.
    pub operations: Vec<(PathBuf, PathBuf)>,
}

impl RollbackJournal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Safely executes a batch rename plan.
    /// If any step encounters an I/O error, all prior operations in this batch are rolled back.
    pub fn execute(&mut self, plan: &[RenamePlanItem]) -> Result<usize, String> {
        let mut executed_in_batch = Vec::new();

        for item in plan {
            if item.has_collision {
                let err = item
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Collision detected".to_string());
                self.rollback_batch(&executed_in_batch);
                return Err(format!(
                    "Aborting rename: {err} for {}",
                    item.original_filename
                ));
            }

            if item.original_path == item.new_path {
                continue;
            }

            if let Err(e) = fs::rename(&item.original_path, &item.new_path) {
                self.rollback_batch(&executed_in_batch);
                return Err(format!(
                    "Failed to rename {} -> {}: {e}",
                    item.original_path.display(),
                    item.new_path.display()
                ));
            }

            executed_in_batch.push((item.original_path.clone(), item.new_path.clone()));
        }

        let count = executed_in_batch.len();
        self.operations.extend(executed_in_batch);
        Ok(count)
    }

    fn rollback_batch(&self, batch: &[(PathBuf, PathBuf)]) {
        for (orig, curr) in batch.iter().rev() {
            let _ = fs::rename(curr, orig);
        }
    }

    /// Reverts all operations in the journal in reverse order.
    pub fn rollback_all(&mut self) -> Result<usize, String> {
        let mut restored = 0;
        let mut errors = Vec::new();

        while let Some((orig, curr)) = self.operations.pop() {
            if curr.exists() {
                if let Err(e) = fs::rename(&curr, &orig) {
                    errors.push(format!(
                        "Failed to revert {} -> {}: {e}",
                        curr.display(),
                        orig.display()
                    ));
                } else {
                    restored += 1;
                }
            }
        }

        if !errors.is_empty() {
            Err(errors.join("; "))
        } else {
            Ok(restored)
        }
    }
}

/// Batch metadata mutations for multiple assets.
#[derive(Debug, Default, Clone)]
pub struct BatchMetadataMutation {
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    pub set_rating: Option<u8>,
}

impl BatchMetadataMutation {
    pub fn apply(&self, assets: &mut [MediaAsset]) {
        for asset in assets.iter_mut() {
            if let Some(r) = self.set_rating {
                asset.rating = r;
            }
            for tag in &self.add_tags {
                if !asset.tags.contains(tag) {
                    asset.tags.push(tag.clone());
                }
            }
            if !self.remove_tags.is_empty() {
                asset.tags.retain(|t| !self.remove_tags.contains(t));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_rename_tokens_and_preflight() {
        let planner = BatchRenamePlanner::new("Project_{date}_{seq:03}_{name}", 1);

        let asset1 = MediaAsset {
            id: "A001".to_string(),
            path: PathBuf::from("/media/clip_a.mov"),
            filename: "clip_a.mov".to_string(),
            file_size_bytes: 1000,
            kind: crate::MediaKind::Video,
            duration_secs: Some(10.0),
            timecode_start: "00:00:00:00".to_string(),
            video: Some(crate::VideoStreamInfo {
                codec: "ProRes".to_string(),
                width: 3840,
                height: 2160,
                fps: 24.0,
                color_space: "Rec. 709".to_string(),
                hdr: false,
                bit_depth: 10,
            }),
            audio: None,
            camera_model: None,
            date_created: "2026-10-12 10:00:00".to_string(),
            geo_location: None,
            rating: 5,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
            document: None,
            archive: None,
        };

        let asset2 = MediaAsset {
            id: "A002".to_string(),
            path: PathBuf::from("/media/clip_b.mov"),
            filename: "clip_b.mov".to_string(),
            file_size_bytes: 1000,
            kind: crate::MediaKind::Video,
            duration_secs: Some(10.0),
            timecode_start: "00:00:00:00".to_string(),
            video: None,
            audio: None,
            camera_model: None,
            date_created: "2026-10-12 10:00:00".to_string(),
            geo_location: None,
            rating: 4,
            tags: vec![],
            has_proxy: false,
            needs_transcode: false,
            is_offline: false,
            document: None,
            archive: None,
        };

        let plan = planner.plan(&[asset1, asset2]);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].new_filename, "Project_2026-10-12_001_clip_a.mov");
        assert_eq!(plan[1].new_filename, "Project_2026-10-12_002_clip_b.mov");
        assert!(!plan[0].has_collision);
        assert!(!plan[1].has_collision);
    }

    #[test]
    fn batch_rename_atomic_rollback() {
        let temp_dir = std::env::temp_dir().join(format!("mc_rename_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let f1 = temp_dir.join("orig_1.txt");
        let f2 = temp_dir.join("orig_2.txt");
        let _ = fs::write(&f1, "1");
        let _ = fs::write(&f2, "2");

        let target1 = temp_dir.join("renamed_1.txt");
        let target2 = temp_dir.join("renamed_2.txt");

        let plan = vec![
            RenamePlanItem {
                asset_id: "1".to_string(),
                original_path: f1.clone(),
                original_filename: "orig_1.txt".to_string(),
                new_filename: "renamed_1.txt".to_string(),
                new_path: target1.clone(),
                has_collision: false,
                error_message: None,
            },
            RenamePlanItem {
                asset_id: "2".to_string(),
                original_path: f2.clone(),
                original_filename: "orig_2.txt".to_string(),
                new_filename: "renamed_2.txt".to_string(),
                new_path: target2.clone(),
                has_collision: false,
                error_message: None,
            },
        ];

        let mut journal = RollbackJournal::new();
        let res = journal.execute(&plan);
        assert_eq!(res, Ok(2));
        assert!(target1.exists());
        assert!(target2.exists());
        assert!(!f1.exists());
        assert!(!f2.exists());

        // Rollback
        let rolled = journal.rollback_all();
        assert_eq!(rolled, Ok(2));
        assert!(f1.exists());
        assert!(f2.exists());
        assert!(!target1.exists());
        assert!(!target2.exists());

        let _ = fs::remove_file(&f1);
        let _ = fs::remove_file(&f2);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
