//! MediaCraft Client Library
//!
//! Enables consumer applications (FilmCraft, SoundCraft, EffectCraft, CLI, or GUI)
//! to delegate background audio/video transcoding, remuxing, and deliverable verification
//! to the `mediacraft-daemon` over IPC.
//!
//! # Example
//!
//! ```no_run
//! use mediacraft_client::MediaCraftClient;
//! use mediacraft_protocol::EncodingPreset;
//! use std::path::PathBuf;
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = MediaCraftClient::connect_default()?;
//! let version = client.ping()?;
//! println!("Connected to MediaCraft daemon (protocol v{version})");
//!
//! let job_id = client.submit_job(
//!     PathBuf::from("raw_interview.wav"),
//!     PathBuf::from("export_web.m4a"),
//!     Some(EncodingPreset::default()),
//!     Some(10),
//! )?;
//! println!("Submitted export job #{job_id}");
//! # Ok(())
//! # }
//! ```

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

#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufRead, BufReader, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use mediacraft_protocol::{
    AudioCodec, ContainerFormat, DeliveryContract, EncodingPreset, JobId, JobInfo, JobState,
    PROTOCOL_VERSION, PauseReason, QueueStatus, Request, Response, ValidationFinding,
    ValidationReport, ValidationSeverity, VideoCodec,
};
use thiserror::Error;

/// Default socket path used by the background daemon.
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/mediacraft.sock";

/// Client errors when communicating with the MediaCraft daemon.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("I/O error communicating with MediaCraft daemon: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON protocol serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Daemon rejected request ({code}): {message}")]
    DaemonError { code: String, message: String },

    #[error("Unexpected response from daemon: received {0}")]
    UnexpectedResponse(String),

    #[error("Daemon is not reachable at socket path: {0}")]
    DaemonNotReachable(PathBuf),

    #[error("Timed out waiting for job #{0:?} to complete after {1:?}")]
    Timeout(JobId, Duration),

    #[error("Unsupported platform: Unix domain sockets require non-WASM target")]
    UnsupportedPlatform,
}

/// High-level client for communicating with the MediaCraft daemon.
pub struct MediaCraftClient {
    socket_path: PathBuf,
    #[cfg(not(target_arch = "wasm32"))]
    stream: UnixStream,
    #[cfg(not(target_arch = "wasm32"))]
    reader: BufReader<UnixStream>,
}

impl MediaCraftClient {
    /// Connects to the daemon at the specified Unix domain socket path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let path = socket_path.as_ref().to_path_buf();
        let stream = UnixStream::connect(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::DaemonNotReachable(path.clone())
            } else {
                ClientError::Io(e)
            }
        })?;

        let read_stream = stream.try_clone()?;
        let reader = BufReader::new(read_stream);

        Ok(Self {
            socket_path: path,
            stream,
            reader,
        })
    }

    /// Stub connect for WebAssembly target.
    #[cfg(target_arch = "wasm32")]
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let _ = socket_path;
        Err(ClientError::UnsupportedPlatform)
    }

    /// Connects to the daemon using the environment variable `MEDIACRAFT_SOCKET`
    /// or falls back to `DEFAULT_SOCKET_PATH`.
    pub fn connect_default() -> Result<Self, ClientError> {
        let path_str =
            std::env::var("MEDIACRAFT_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());
        Self::connect(path_str)
    }

    /// Checks if a running daemon is responding at the default socket path.
    pub fn is_daemon_running() -> bool {
        let path_str =
            std::env::var("MEDIACRAFT_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());
        let path = PathBuf::from(path_str);
        if !path.exists() {
            return false;
        }
        match Self::connect(&path) {
            Ok(mut client) => client.ping().is_ok(),
            Err(_) => false,
        }
    }

    /// Returns the socket path this client is connected to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Sends a low-level IPC request to the daemon and returns the response.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn call(&mut self, req: Request) -> Result<Response, ClientError> {
        let mut json = serde_json::to_string(&req)?;
        json.push('\n');
        self.stream.write_all(json.as_bytes())?;
        self.stream.flush()?;

        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Err(ClientError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "MediaCraft daemon closed the connection unexpectedly",
            )));
        }

        let resp: Response = serde_json::from_str(&line)?;
        match resp {
            Response::Error { code, message } => Err(ClientError::DaemonError { code, message }),
            other => Ok(other),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn call(&mut self, _req: Request) -> Result<Response, ClientError> {
        Err(ClientError::UnsupportedPlatform)
    }

    /// Pings the daemon, returning the supported protocol version.
    pub fn ping(&mut self) -> Result<u32, ClientError> {
        match self.call(Request::Ping)? {
            Response::Pong { protocol_version } => Ok(protocol_version),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Submits a background media export / transcode job.
    pub fn submit_job(
        &mut self,
        source_path: impl Into<PathBuf>,
        output_path: impl Into<PathBuf>,
        preset: Option<EncodingPreset>,
        priority: Option<u32>,
    ) -> Result<JobId, ClientError> {
        match self.call(Request::AddJob {
            source_path: source_path.into(),
            output_path: output_path.into(),
            preset,
            priority,
        })? {
            Response::JobAdded { job_id } => Ok(job_id),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Helper to submit an export with an explicit preset.
    pub fn submit_export(
        &mut self,
        source_path: impl Into<PathBuf>,
        output_path: impl Into<PathBuf>,
        preset: EncodingPreset,
    ) -> Result<JobId, ClientError> {
        self.submit_job(source_path, output_path, Some(preset), None)
    }

    /// Cancels a running or queued job.
    pub fn cancel_job(&mut self, job_id: JobId) -> Result<(), ClientError> {
        match self.call(Request::CancelJob { job_id })? {
            Response::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Retrieves detailed status for a specific job.
    pub fn get_job(&mut self, job_id: JobId) -> Result<JobInfo, ClientError> {
        match self.call(Request::GetJob { job_id })? {
            Response::JobDetails { job } => Ok(job),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Lists all jobs currently registered in the daemon queue.
    pub fn list_jobs(&mut self) -> Result<Vec<JobInfo>, ClientError> {
        match self.call(Request::ListJobs)? {
            Response::JobList { jobs, .. } => Ok(jobs),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Retrieves aggregate queue execution metrics.
    pub fn get_queue_status(&mut self) -> Result<QueueStatus, ClientError> {
        match self.call(Request::GetQueueStatus)? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Pauses background processing in the daemon queue.
    pub fn pause_queue(&mut self, reason: PauseReason) -> Result<QueueStatus, ClientError> {
        match self.call(Request::PauseQueue { reason })? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Resumes background processing in the daemon queue.
    pub fn resume_queue(&mut self, reason: PauseReason) -> Result<QueueStatus, ClientError> {
        match self.call(Request::ResumeQueue { reason })? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Communicates NLE/DAW timeline playback state to automatically pause or resume.
    pub fn set_playback_active(&mut self, is_playing: bool) -> Result<QueueStatus, ClientError> {
        match self.call(Request::SetPlaybackActive { is_playing })? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Gracefully instructs the daemon to shut down.
    pub fn shutdown(&mut self) -> Result<(), ClientError> {
        match self.call(Request::Shutdown)? {
            Response::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }

    /// Polls the daemon until the specified job reaches a terminal state
    /// (`Completed`, `Failed`, or `Cancelled`), or until the timeout expires.
    pub fn wait_for_job(
        &mut self,
        job_id: JobId,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<JobState, ClientError> {
        let start = Instant::now();
        loop {
            let job = self.get_job(job_id)?;
            match job.state {
                JobState::Completed { .. } | JobState::Failed { .. } | JobState::Cancelled => {
                    return Ok(job.state);
                }
                _ => {}
            }

            if start.elapsed() >= timeout {
                return Err(ClientError::Timeout(job_id, timeout));
            }

            std::thread::sleep(poll_interval);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediacraft_daemon::QueueEngine;
    use mediacraft_daemon::ipc::IpcServer;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_client_lifecycle_and_ipc_calls() {
        let sock_path = format!("/tmp/mediacraft_client_test_{}.sock", std::process::id());
        let engine = Arc::new(Mutex::new(QueueEngine::new(2)));
        let server = IpcServer::new(&sock_path, Arc::clone(&engine));
        let handle = server
            .start()
            .unwrap_or_else(|e| panic!("server start: {e}"));

        std::thread::sleep(Duration::from_millis(50));

        let mut client =
            MediaCraftClient::connect(&sock_path).unwrap_or_else(|e| panic!("client connect: {e}"));

        // 1. Ping
        let version = client.ping().unwrap_or_else(|e| panic!("ping: {e}"));
        assert_eq!(version, PROTOCOL_VERSION);

        // 2. Submit job
        let job_id = client
            .submit_job(
                PathBuf::from("/sources/interview.mov"),
                PathBuf::from("/exports/interview.mp4"),
                Some(EncodingPreset::default()),
                Some(5),
            )
            .unwrap_or_else(|e| panic!("submit: {e}"));
        assert_eq!(job_id, JobId(1));

        // 3. Get job details
        let job = client
            .get_job(job_id)
            .unwrap_or_else(|e| panic!("get_job: {e}"));
        assert_eq!(job.id, job_id);
        assert_eq!(job.priority, 5);

        // 4. List jobs
        let jobs = client
            .list_jobs()
            .unwrap_or_else(|e| panic!("list_jobs: {e}"));
        assert_eq!(jobs.len(), 1);

        // 5. Queue status
        let q_status = client
            .get_queue_status()
            .unwrap_or_else(|e| panic!("queue_status: {e}"));
        assert_eq!(q_status.queued_jobs, 1);

        // 6. Pause & resume
        let paused = client
            .pause_queue(PauseReason::UserRequested)
            .unwrap_or_else(|e| panic!("pause: {e}"));
        assert_eq!(paused.pause_reason, Some(PauseReason::UserRequested));

        let resumed = client
            .resume_queue(PauseReason::UserRequested)
            .unwrap_or_else(|e| panic!("resume: {e}"));
        assert_eq!(resumed.pause_reason, None);

        // 7. Cancel job
        client
            .cancel_job(job_id)
            .unwrap_or_else(|e| panic!("cancel: {e}"));
        let cancelled_job = client
            .get_job(job_id)
            .unwrap_or_else(|e| panic!("get_job after cancel: {e}"));
        assert_eq!(cancelled_job.state, JobState::Cancelled);

        // Cleanup
        server.stop();
        let _ = handle.join();
        let _ = std::fs::remove_file(&sock_path);
    }
}
