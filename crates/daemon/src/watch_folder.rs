//! Watch Folder background monitoring and automated ingest worker for MediaCraft daemon.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::QueueEngine;
use mediacraft_protocol::EncodingPreset;

/// Configuration for an automated watch folder.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchFolderRule {
    pub id: String,
    pub name: String,
    pub watch_dir: PathBuf,
    pub output_dir: PathBuf,
    pub preset_id: String,
    pub active: bool,
    pub file_filter_exts: Vec<String>,
    pub min_stability_duration: Duration,
}

impl Default for WatchFolderRule {
    fn default() -> Self {
        Self {
            id: "wf-camera-card-ingest".to_string(),
            name: "Camera Card Auto-Ingest".to_string(),
            watch_dir: PathBuf::from("/Users/zachshallbetter/Movies/WatchFolders/Ingest"),
            output_dir: PathBuf::from("/Users/zachshallbetter/Movies/WatchFolders/Deliveries"),
            preset_id: "match-source-hq".to_string(),
            active: true,
            file_filter_exts: vec![
                "mov".to_string(),
                "mp4".to_string(),
                "m4v".to_string(),
                "mxf".to_string(),
                "wav".to_string(),
            ],
            min_stability_duration: Duration::from_secs(1),
        }
    }
}

/// An observed file in a watch folder being tracked for stability.
#[derive(Debug, Clone)]
struct TrackedFile {
    path: PathBuf,
    last_size: u64,
    first_seen: SystemTime,
    last_modified: SystemTime,
}

/// Engine managing multiple watch folders and automatically queueing jobs.
#[derive(Debug, Default)]
pub struct WatchFolderEngine {
    rules: Vec<WatchFolderRule>,
    processed_paths: HashSet<PathBuf>,
    in_flight_files: Vec<TrackedFile>,
}

impl WatchFolderEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_rule(&mut self, rule: WatchFolderRule) {
        self.rules.push(rule);
    }

    pub fn list_rules(&self) -> &[WatchFolderRule] {
        &self.rules
    }

    pub fn list_rules_mut(&mut self) -> &mut [WatchFolderRule] {
        &mut self.rules
    }

    pub fn is_processed(&self, path: &Path) -> bool {
        self.processed_paths.contains(path)
    }

    /// Scan all active watch folders, check file stability, and automatically enqueue ready media.
    /// Returns the list of (source_path, target_path, preset_id) that were enqueued.
    pub fn scan_and_enqueue(
        &mut self,
        queue: &mut QueueEngine,
        presets: &[EncodingPreset],
    ) -> Vec<(PathBuf, PathBuf, String)> {
        let mut enqueued = Vec::new();

        for rule in &self.rules {
            if !rule.active || !rule.watch_dir.exists() {
                continue;
            }

            let entries = match std::fs::read_dir(&rule.watch_dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }

                // Check file extension filter
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !rule.file_filter_exts.is_empty() && !rule.file_filter_exts.contains(&ext) {
                    continue;
                }

                // Ignore files already completely processed
                if self.processed_paths.contains(&path) {
                    continue;
                }

                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let file_len = meta.len();
                let modified = meta.modified().unwrap_or(SystemTime::now());

                // Check or update in-flight tracking
                if let Some(idx) = self.in_flight_files.iter().position(|f| f.path == path) {
                    let tracked = &mut self.in_flight_files[idx];
                    if tracked.last_size == file_len {
                        let elapsed = SystemTime::now()
                            .duration_since(tracked.first_seen)
                            .unwrap_or(Duration::ZERO);

                        // File size has remained stable for minimum duration
                        if elapsed >= rule.min_stability_duration {
                            self.in_flight_files.remove(idx);
                            self.processed_paths.insert(path.clone());

                            // Find target preset or fallback to default
                            let preset = presets
                                .iter()
                                .find(|p| p.id == rule.preset_id)
                                .cloned()
                                .unwrap_or_default();

                            let out_filename = format!(
                                "{}_{}.mp4",
                                path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("output"),
                                preset.name.replace(' ', "_")
                            );
                            let out_path = rule.output_dir.join(out_filename);

                            queue.add_job(path.clone(), out_path.clone(), preset, 10);
                            enqueued.push((path, out_path, rule.preset_id.clone()));
                        }
                    } else {
                        tracked.last_size = file_len;
                        tracked.last_modified = modified;
                    }
                } else {
                    self.in_flight_files.push(TrackedFile {
                        path,
                        last_size: file_len,
                        first_seen: SystemTime::now(),
                        last_modified: modified,
                    });
                }
            }
        }

        enqueued
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_folder_automatic_enqueue() {
        let temp_base =
            std::env::temp_dir().join(format!("mediacraft_wf_test_{}", std::process::id()));
        let watch_dir = temp_base.join("watch");
        let output_dir = temp_base.join("output");
        let _ = std::fs::create_dir_all(&watch_dir);
        let _ = std::fs::create_dir_all(&output_dir);

        let mut queue = QueueEngine::new(1);
        let mut engine = WatchFolderEngine::new();

        engine.add_rule(WatchFolderRule {
            id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            watch_dir: watch_dir.clone(),
            output_dir: output_dir.clone(),
            preset_id: "match-source-h264".to_string(),
            active: true,
            file_filter_exts: vec!["mov".to_string()],
            min_stability_duration: Duration::from_millis(10),
        });

        // Drop a test media file into the watch directory
        let media_file = watch_dir.join("clip_01.mov");
        let _ = std::fs::write(&media_file, b"DUMMY_MEDIA_BYTES_FOR_WATCH_FOLDER");

        let presets = vec![EncodingPreset::default()];

        // First scan registers the file in in_flight_files
        let first_res = engine.scan_and_enqueue(&mut queue, &presets);
        assert_eq!(first_res.len(), 0);
        assert_eq!(queue.list_jobs().len(), 0);

        // Sleep to satisfy min_stability_duration
        std::thread::sleep(Duration::from_millis(20));

        // Second scan detects stability and enqueues job
        let second_res = engine.scan_and_enqueue(&mut queue, &presets);
        assert_eq!(second_res.len(), 1);
        assert_eq!(queue.list_jobs().len(), 1);

        // Third scan is idempotent and doesn't re-enqueue
        let third_res = engine.scan_and_enqueue(&mut queue, &presets);
        assert_eq!(third_res.len(), 0);
        assert_eq!(queue.list_jobs().len(), 1);

        // Cleanup
        let _ = std::fs::remove_file(&media_file);
        let _ = std::fs::remove_dir_all(&temp_base);
    }
}
