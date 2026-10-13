//! Background transcoding daemon and queue management engine for MediaCraft.
//!
//! Provides the core job scheduler, concurrency control, and IPC message dispatcher.
//! Implements the media encoding queue lifecycle, worker dispatch, and execution orchestration.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo,
        clippy::unreachable
    )
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod ipc;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use mediacraft_protocol::{
    EncodingPreset, JobId, JobInfo, JobState, PROTOCOL_VERSION, PauseReason, QueueStatus, Request,
    Response,
};

/// Core state machine and queue coordinator.
#[derive(Debug)]
pub struct QueueEngine {
    jobs: Vec<JobInfo>,
    next_job_id: u64,
    max_concurrency: usize,
    pause_reason: Option<PauseReason>,
    is_running: bool,
}

impl QueueEngine {
    /// Create a new queue engine with a specified maximum concurrent transcode count.
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            jobs: Vec::new(),
            next_job_id: 1,
            max_concurrency: max_concurrency.max(1),
            pause_reason: None,
            is_running: true,
        }
    }

    /// Add a new transcoding job to the queue.
    pub fn add_job(
        &mut self,
        source_path: PathBuf,
        output_path: PathBuf,
        preset: EncodingPreset,
        priority: u32,
    ) -> JobId {
        let id = JobId(self.next_job_id);
        self.next_job_id = self.next_job_id.saturating_add(1);

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let job = JobInfo {
            id,
            source_path,
            output_path,
            preset,
            priority,
            state: JobState::Queued,
            created_at_unix_ts: created_at,
        };

        self.jobs.push(job);
        id
    }

    /// Cancel a job by ID.
    pub fn cancel_job(&mut self, id: JobId) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            match job.state {
                JobState::Queued | JobState::Running { .. } | JobState::Paused { .. } => {
                    job.state = JobState::Cancelled;
                    true
                }
                JobState::Completed { .. } | JobState::Failed { .. } | JobState::Cancelled => false,
            }
        } else {
            false
        }
    }

    /// Pause the queue with a specific reason (User or Playback).
    pub fn pause_queue(&mut self, reason: PauseReason) {
        self.pause_reason = Some(reason);
        for job in &mut self.jobs {
            if let JobState::Running { progress_pct, .. } = job.state {
                job.state = JobState::Paused {
                    progress_pct,
                    reason,
                };
            }
        }
    }

    /// Resume the queue, clearing the corresponding pause reason.
    pub fn resume_queue(&mut self, reason: PauseReason) {
        if self.pause_reason == Some(reason) {
            self.pause_reason = None;
            for job in &mut self.jobs {
                if let JobState::Paused {
                    progress_pct,
                    reason: r,
                } = job.state
                    && r == reason
                {
                    job.state = JobState::Running {
                        progress_pct,
                        current_frame: 0,
                        total_frames: 0,
                        encoding_fps: 0.0,
                    };
                }
            }
        }
    }

    /// Update playback state hook (FilmCraft / SoundCraft integration).
    pub fn set_playback_active(&mut self, is_playing: bool) {
        if is_playing {
            self.pause_queue(PauseReason::PlaybackActive);
        } else {
            self.resume_queue(PauseReason::PlaybackActive);
        }
    }

    /// Count of currently running jobs.
    pub fn active_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| matches!(j.state, JobState::Running { .. }))
            .count()
    }

    /// Dispatch the next eligible queued job if capacity is available.
    pub fn poll_dispatch(&mut self) -> Option<JobId> {
        if self.pause_reason.is_some() || !self.is_running {
            return None;
        }

        if self.active_count() >= self.max_concurrency {
            return None;
        }

        // Find the queued job with highest priority, then earliest creation time
        let best_idx = self
            .jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| matches!(j.state, JobState::Queued))
            .max_by(|(_, a), (_, b)| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.created_at_unix_ts.cmp(&a.created_at_unix_ts))
            })
            .map(|(idx, _)| idx);

        if let Some(idx) = best_idx
            && let Some(job) = self.jobs.get_mut(idx)
        {
            job.state = JobState::Running {
                progress_pct: 0.0,
                current_frame: 0,
                total_frames: 0,
                encoding_fps: 0.0,
            };
            return Some(job.id);
        }

        None
    }

    /// Update the progress of an actively running job.
    pub fn update_progress(
        &mut self,
        id: JobId,
        progress_pct: f32,
        current_frame: u64,
        total_frames: u64,
        fps: f32,
    ) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id)
            && matches!(job.state, JobState::Running { .. })
        {
            job.state = JobState::Running {
                progress_pct,
                current_frame,
                total_frames,
                encoding_fps: fps,
            };
            return true;
        }
        false
    }

    /// Mark a running job as completed.
    pub fn complete_job(&mut self, id: JobId, duration_secs: f64, bytes_written: u64) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            job.state = JobState::Completed {
                duration_secs,
                bytes_written,
            };
            true
        } else {
            false
        }
    }

    /// Mark a running job as failed.
    pub fn fail_job(&mut self, id: JobId, error_message: String) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            job.state = JobState::Failed { error_message };
            true
        } else {
            false
        }
    }

    /// Retrieve detailed information for a single job.
    pub fn get_job(&self, id: JobId) -> Option<JobInfo> {
        self.jobs.iter().find(|j| j.id == id).cloned()
    }

    /// Retrieve all tracked jobs.
    pub fn list_jobs(&self) -> Vec<JobInfo> {
        self.jobs.clone()
    }

    /// Clear all completed, failed, or cancelled jobs.
    pub fn clear_completed(&mut self) {
        self.jobs.retain(|j| {
            matches!(
                j.state,
                JobState::Queued | JobState::Running { .. } | JobState::Paused { .. }
            )
        });
    }

    /// Clear all jobs from the queue.
    pub fn clear_all(&mut self) {
        self.jobs.clear();
    }

    /// Remove a specific job by ID regardless of state.
    pub fn remove_job(&mut self, id: JobId) -> bool {
        let before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        self.jobs.len() < before
    }

    /// Retry a failed or cancelled job by resetting it to queued state.
    pub fn retry_job(&mut self, id: JobId) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id)
            && matches!(job.state, JobState::Failed { .. } | JobState::Cancelled)
        {
            job.state = JobState::Queued;
            return true;
        }
        false
    }

    /// Compute the overall queue status.
    pub fn status(&self) -> QueueStatus {
        let mut active = 0;
        let mut queued = 0;
        let mut completed = 0;
        let mut failed = 0;

        for j in &self.jobs {
            match j.state {
                JobState::Running { .. } => active += 1,
                JobState::Queued => queued += 1,
                JobState::Completed { .. } => completed += 1,
                JobState::Failed { .. } => failed += 1,
                JobState::Paused { .. } | JobState::Cancelled => {}
            }
        }

        QueueStatus {
            is_running: self.is_running && self.pause_reason.is_none(),
            pause_reason: self.pause_reason,
            active_jobs: active,
            queued_jobs: queued,
            completed_jobs: completed,
            failed_jobs: failed,
        }
    }

    /// Process a typed IPC request and produce the matching response.
    pub fn handle_request(&mut self, request: Request) -> Response {
        match request {
            Request::Ping => Response::Pong {
                protocol_version: PROTOCOL_VERSION,
            },
            Request::AddJob {
                source_path,
                output_path,
                preset,
                priority,
            } => {
                let id = self.add_job(
                    source_path,
                    output_path,
                    preset.unwrap_or_default(),
                    priority.unwrap_or(0),
                );
                Response::JobAdded { job_id: id }
            }
            Request::CancelJob { job_id } => {
                if self.cancel_job(job_id) {
                    Response::Ok
                } else {
                    Response::Error {
                        code: "JOB_NOT_CANCELLABLE".to_string(),
                        message: format!("Job {job_id:?} cannot be cancelled or does not exist"),
                    }
                }
            }
            Request::PauseQueue { reason } => {
                self.pause_queue(reason);
                Response::QueueStatusUpdated {
                    queue: self.status(),
                }
            }
            Request::ResumeQueue { reason } => {
                self.resume_queue(reason);
                Response::QueueStatusUpdated {
                    queue: self.status(),
                }
            }
            Request::SetPlaybackActive { is_playing } => {
                self.set_playback_active(is_playing);
                Response::QueueStatusUpdated {
                    queue: self.status(),
                }
            }
            Request::GetJob { job_id } => match self.get_job(job_id) {
                Some(job) => Response::JobDetails { job },
                None => Response::Error {
                    code: "JOB_NOT_FOUND".to_string(),
                    message: format!("Job {job_id:?} does not exist"),
                },
            },
            Request::ListJobs => Response::JobList {
                jobs: self.list_jobs(),
                queue: self.status(),
            },
            Request::GetQueueStatus => Response::QueueStatusUpdated {
                queue: self.status(),
            },
            Request::Shutdown => {
                self.is_running = false;
                Response::Ok
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_lifecycle_and_dispatch() {
        let mut engine = QueueEngine::new(2);

        let j1 = engine.add_job(
            PathBuf::from("/in/1.mov"),
            PathBuf::from("/out/1.mp4"),
            EncodingPreset::default(),
            5,
        );
        let j2 = engine.add_job(
            PathBuf::from("/in/2.mov"),
            PathBuf::from("/out/2.mp4"),
            EncodingPreset::default(),
            10, // Higher priority
        );

        // j2 should be dispatched first because priority is higher
        assert_eq!(engine.poll_dispatch(), Some(j2));
        assert_eq!(engine.poll_dispatch(), Some(j1));
        // Capacity reached (max_concurrency = 2)
        assert_eq!(engine.poll_dispatch(), None);

        // Complete j2
        assert!(engine.complete_job(j2, 12.5, 1_048_576));
        let details = engine.get_job(j2).unwrap_or_else(|| panic!("missing job"));
        assert!(matches!(details.state, JobState::Completed { .. }));
    }

    #[test]
    fn playback_pause_hook() {
        let mut engine = QueueEngine::new(2);
        let j1 = engine.add_job(
            PathBuf::from("/in/1.mov"),
            PathBuf::from("/out/1.mp4"),
            EncodingPreset::default(),
            0,
        );
        assert_eq!(engine.poll_dispatch(), Some(j1));

        // NLE starts playback -> queue pauses automatically
        engine.set_playback_active(true);
        let s = engine.status();
        assert!(!s.is_running);
        assert_eq!(s.pause_reason, Some(PauseReason::PlaybackActive));

        let job = engine.get_job(j1).unwrap_or_else(|| panic!("missing job"));
        assert!(matches!(
            job.state,
            JobState::Paused {
                reason: PauseReason::PlaybackActive,
                ..
            }
        ));

        // NLE stops playback -> queue resumes automatically
        engine.set_playback_active(false);
        let s = engine.status();
        assert!(s.is_running);
        assert_eq!(s.pause_reason, None);
    }

    #[test]
    fn queue_cleanup_and_retry() {
        let mut engine = QueueEngine::new(2);
        let j1 = engine.add_job(
            PathBuf::from("/in/1.mov"),
            PathBuf::from("/out/1.mp4"),
            EncodingPreset::default(),
            0,
        );
        let j2 = engine.add_job(
            PathBuf::from("/in/2.mov"),
            PathBuf::from("/out/2.mp4"),
            EncodingPreset::default(),
            0,
        );
        let _ = engine.complete_job(j1, 10.0, 5000);
        let _ = engine.fail_job(j2, "Disk full".to_string());

        assert!(engine.retry_job(j2));
        assert!(matches!(
            engine.get_job(j2).unwrap().state,
            JobState::Queued
        ));

        assert!(engine.remove_job(j2));
        assert_eq!(engine.get_job(j2), None);

        engine.clear_completed();
        assert_eq!(engine.list_jobs().len(), 0);
    }
}
