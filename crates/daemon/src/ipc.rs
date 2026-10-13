//! Cross-process IPC server and client for MediaCraft.
//!
//! Enables external applications (FilmCraft, EffectCraft, SoundCraft, CLI)
//! to send transcode jobs, pause/resume queues, and query status over
//! a Unix domain socket or local stream.

#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufRead, BufReader, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use mediacraft_protocol::{JobId, JobInfo, PauseReason, QueueStatus, Request, Response};

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/mediacraft.sock";

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Server error: {code} - {message}")]
    Server { code: String, message: String },
    #[error("Unexpected response from server")]
    UnexpectedResponse,
}

pub type Result<T> = std::result::Result<T, IpcError>;

#[cfg(not(target_arch = "wasm32"))]
/// Multi-threaded Unix domain socket IPC server.
pub struct IpcServer {
    socket_path: PathBuf,
    engine: Arc<Mutex<crate::QueueEngine>>,
    shutdown_flag: Arc<Mutex<bool>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl IpcServer {
    pub fn new(socket_path: impl Into<PathBuf>, engine: Arc<Mutex<crate::QueueEngine>>) -> Self {
        Self {
            socket_path: socket_path.into(),
            engine,
            shutdown_flag: Arc::new(Mutex::new(false)),
        }
    }

    /// Spawns the IPC server in a background thread and returns the listener handle.
    pub fn start(&self) -> Result<thread::JoinHandle<()>> {
        // Clean up stale socket file if it exists
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        // Set non-blocking so we can check shutdown_flag periodically
        listener.set_nonblocking(true)?;

        let engine = Arc::clone(&self.engine);
        let shutdown = Arc::clone(&self.shutdown_flag);
        let path_clone = self.socket_path.clone();

        let handle = thread::spawn(move || {
            loop {
                // Check if shutdown requested
                if *shutdown.lock().unwrap_or_else(PoisonError::into_inner) {
                    break;
                }

                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let _ = stream.set_nonblocking(false);
                        let engine_ref = Arc::clone(&engine);
                        thread::spawn(move || {
                            let _ = Self::handle_client(stream, engine_ref);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(15));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            // Cleanup socket file on exit
            let _ = std::fs::remove_file(path_clone);
        });

        Ok(handle)
    }

    /// Signals the background server loop to stop and clean up.
    pub fn stop(&self) {
        let mut flag = self
            .shutdown_flag
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *flag = true;
    }

    fn handle_client(stream: UnixStream, engine: Arc<Mutex<crate::QueueEngine>>) -> Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;
        let mut line = String::new();

        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let req: Request = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(err) => {
                        let resp = Response::Error {
                            code: "INVALID_REQUEST".to_string(),
                            message: format!("Malformed request: {err}"),
                        };
                        let mut resp_json = serde_json::to_string(&resp)?;
                        resp_json.push('\n');
                        writer.write_all(resp_json.as_bytes())?;
                        writer.flush()?;
                        line.clear();
                        continue;
                    }
                };

                let resp = {
                    let mut eng = engine.lock().unwrap_or_else(PoisonError::into_inner);
                    eng.handle_request(req)
                };

                let mut resp_json = serde_json::to_string(&resp)?;
                resp_json.push('\n');
                writer.write_all(resp_json.as_bytes())?;
                writer.flush()?;
            }
            line.clear();
        }

        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Synchronous client for communicating with a running MediaCraft daemon or GUI workstation.
pub struct IpcClient {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

#[cfg(not(target_arch = "wasm32"))]
impl IpcClient {
    /// Connect to the daemon socket at the given path.
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_nonblocking(false)?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    /// Connect to the default socket path (`/tmp/mediacraft.sock`).
    pub fn connect_default() -> Result<Self> {
        Self::connect(DEFAULT_SOCKET_PATH)
    }

    /// Send a request and receive a response.
    pub fn call(&mut self, request: Request) -> Result<Response> {
        let mut req_str = serde_json::to_string(&request)?;
        req_str.push('\n');
        self.stream.write_all(req_str.as_bytes())?;
        self.stream.flush()?;

        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Err(IpcError::UnexpectedResponse);
        }

        let resp: Response = serde_json::from_str(line.trim())?;
        Ok(resp)
    }

    pub fn ping(&mut self) -> Result<u32> {
        match self.call(Request::Ping)? {
            Response::Pong { protocol_version } => Ok(protocol_version),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn get_status(&mut self) -> Result<QueueStatus> {
        match self.call(Request::GetQueueStatus)? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn list_jobs(&mut self) -> Result<Vec<JobInfo>> {
        match self.call(Request::ListJobs)? {
            Response::JobList { jobs, .. } => Ok(jobs),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn add_job(
        &mut self,
        source: PathBuf,
        output: PathBuf,
        preset: Option<mediacraft_protocol::EncodingPreset>,
        priority: Option<u32>,
    ) -> Result<JobId> {
        match self.call(Request::AddJob {
            source_path: source,
            output_path: output,
            preset,
            priority,
        })? {
            Response::JobAdded { job_id } => Ok(job_id),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn cancel_job(&mut self, job_id: JobId) -> Result<()> {
        match self.call(Request::CancelJob { job_id })? {
            Response::Ok => Ok(()),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn pause_queue(&mut self, reason: PauseReason) -> Result<QueueStatus> {
        match self.call(Request::PauseQueue { reason })? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn resume_queue(&mut self, reason: PauseReason) -> Result<QueueStatus> {
        match self.call(Request::ResumeQueue { reason })? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }

    pub fn set_playback_active(&mut self, is_playing: bool) -> Result<QueueStatus> {
        match self.call(Request::SetPlaybackActive { is_playing })? {
            Response::QueueStatusUpdated { queue } => Ok(queue),
            Response::Error { code, message } => Err(IpcError::Server { code, message }),
            _ => Err(IpcError::UnexpectedResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediacraft_protocol::PROTOCOL_VERSION;

    #[test]
    fn socket_ipc_roundtrip() {
        let sock_path = format!("/tmp/mediacraft_test_{}.sock", std::process::id());
        let engine = Arc::new(Mutex::new(crate::QueueEngine::new(2)));
        let server = IpcServer::new(&sock_path, Arc::clone(&engine));
        let handle = server.start().unwrap();

        // Give listener thread a brief moment to bind
        thread::sleep(std::time::Duration::from_millis(50));

        let mut client = IpcClient::connect(&sock_path).unwrap();

        // 1. Ping
        let version = client.ping().unwrap();
        assert_eq!(version, PROTOCOL_VERSION);

        // 2. Add job
        let j_id = client
            .add_job(
                PathBuf::from("/Footage/Clip_101.mov"),
                PathBuf::from("/Exports/Clip_101.mp4"),
                None,
                Some(10),
            )
            .unwrap();
        assert_eq!(j_id, JobId(1));

        // 3. Status
        let status = client.get_status().unwrap();
        assert_eq!(status.queued_jobs, 1);

        // 4. Set playback active (NLE hook)
        let s2 = client.set_playback_active(true).unwrap();
        assert_eq!(s2.pause_reason, Some(PauseReason::PlaybackActive));

        // 5. Resume
        let s3 = client.set_playback_active(false).unwrap();
        assert_eq!(s3.pause_reason, None);

        // Cleanup
        server.stop();
        let _ = handle.join();
        let _ = std::fs::remove_file(&sock_path);
    }
}
