//! Session supervisor.
//!
//! Owns the runtime map of live sessions, spawns each provider, pumps provider
//! output to the frontend as ordered, batched events, and guarantees cleanup of
//! child processes and reader threads on close (PRD FR-06 / section 7.3-7.4).

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::domain::{
    ConnectionState, Protocol, SerialSpec, SessionSpec, SshSpec, TelnetSpec, TerminalDimensions,
};
use crate::error::{AppError, AppResult};
use crate::logging::LogManager;
use crate::providers::{
    local_pty, openssh_pty, russh_ssh, serial, sftp_pty, telnet, ConnectionProvider,
};

/// A batch of raw remote bytes, base64 encoded so the JSON event payload can
/// carry arbitrary binary without corruption.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub session_id: String,
    pub seq: u64,
    pub data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStateEvent {
    pub session_id: String,
    pub state: ConnectionState,
    pub message: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitEvent {
    pub session_id: String,
    pub reason: String,
}

/// A bounded ring buffer of the most recent raw output bytes for a session.
/// Used by the automation API (`waitFor`/`readUntil`) so scripts never scan an
/// unbounded terminal buffer (FR-20).
pub struct RingBuffer {
    data: VecDeque<u8>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity.min(4096)),
            capacity,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.data.len() == self.capacity {
                self.data.pop_front();
            }
            self.data.push_back(b);
        }
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.data.iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Result of an automation `waitFor`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitResult {
    pub matched: bool,
    /// Matched text when `matched`, otherwise the recent output tail.
    pub text: String,
}

fn wait_timeout(text: &str) -> WaitResult {
    let mut text = text.to_string();
    crate::text::keep_tail(&mut text, 4096);
    WaitResult {
        matched: false,
        text,
    }
}

struct Session {
    id: String,
    #[allow(dead_code)]
    protocol: Protocol,
    spec: SessionSpec,
    provider: Mutex<Arc<dyn ConnectionProvider>>,
    reader_thread: Mutex<Option<JoinHandle<()>>>,
    seq: AtomicU64,
    closed: AtomicBool,
    output: Arc<Mutex<RingBuffer>>,
}

impl Session {
    fn provider(&self) -> Arc<dyn ConnectionProvider> {
        self.provider.lock().clone()
    }
}

/// Runtime registry of sessions. Lives in Tauri managed state.
pub struct SessionSupervisor {
    app: AppHandle,
    logs: Arc<LogManager>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl SessionSupervisor {
    pub fn new(app: AppHandle, logs: Arc<LogManager>) -> Self {
        Self {
            app,
            logs,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn open_local(&self, dims: TerminalDimensions) -> AppResult<String> {
        self.open(SessionSpec::Local, dims)
    }

    pub fn open_ssh(&self, spec: SshSpec, dims: TerminalDimensions) -> AppResult<String> {
        self.open(SessionSpec::Ssh(spec), dims)
    }

    pub fn open_sftp(&self, spec: SshSpec, dims: TerminalDimensions) -> AppResult<String> {
        self.open(SessionSpec::Sftp(spec), dims)
    }

    pub fn open_telnet(&self, spec: TelnetSpec, dims: TerminalDimensions) -> AppResult<String> {
        self.open(SessionSpec::Telnet(spec), dims)
    }

    pub fn open_serial(&self, spec: SerialSpec, dims: TerminalDimensions) -> AppResult<String> {
        self.open(SessionSpec::Serial(spec), dims)
    }

    pub fn send_break(&self, id: &str) -> AppResult<()> {
        self.get(id)?.provider().send_break()
    }

    fn open(&self, spec: SessionSpec, dims: TerminalDimensions) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.emit_state(&id, ConnectionState::Connecting, None);

        let protocol = match spec {
            SessionSpec::Local => Protocol::Local,
            SessionSpec::Ssh(_) => Protocol::Ssh,
            SessionSpec::Sftp(_) => Protocol::Sftp,
            SessionSpec::Telnet(_) => Protocol::Telnet,
            SessionSpec::Serial(_) => Protocol::Serial,
        };
        let provider = self.build_provider(&spec, dims)?;
        let reader = provider.take_reader()?;

        let session = Arc::new(Session {
            id: id.clone(),
            protocol,
            spec,
            provider: Mutex::new(Arc::from(provider)),
            reader_thread: Mutex::new(None),
            seq: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            output: Arc::new(Mutex::new(RingBuffer::new(64 * 1024))),
        });

        let handle = self.spawn_reader(session.clone(), reader);
        *session.reader_thread.lock() = Some(handle);
        self.sessions.lock().insert(id.clone(), session);

        self.emit_state(&id, ConnectionState::Connected, None);
        Ok(id)
    }

    fn build_provider(
        &self,
        spec: &SessionSpec,
        dims: TerminalDimensions,
    ) -> AppResult<Box<dyn ConnectionProvider>> {
        match spec {
            SessionSpec::Local => local_pty::spawn_local_shell(dims),
            SessionSpec::Ssh(s) if s.engine.as_deref() == Some("native") => {
                russh_ssh::spawn_russh(s, dims)
            }
            SessionSpec::Ssh(s) => openssh_pty::spawn_openssh(s, dims),
            SessionSpec::Sftp(s) => sftp_pty::spawn_sftp(s, dims),
            SessionSpec::Telnet(s) => telnet::spawn_telnet(s, dims),
            SessionSpec::Serial(s) => serial::spawn_serial(s, dims),
        }
    }

    fn spawn_reader(
        &self,
        session: Arc<Session>,
        mut reader: Box<dyn Read + Send>,
    ) -> JoinHandle<()> {
        let app = self.app.clone();
        let logs = self.logs.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        logs.write(&session.id, &buf[..n]);
                        session.output.lock().push(&buf[..n]);
                        let seq = session.seq.fetch_add(1, Ordering::SeqCst);
                        let data = B64.encode(&buf[..n]);
                        let _ = app.emit(
                            "terminal-output",
                            TerminalOutput {
                                session_id: session.id.clone(),
                                seq,
                                data,
                            },
                        );
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            session.closed.store(true, Ordering::SeqCst);
            let _ = app.emit(
                "terminal-exit",
                TerminalExitEvent {
                    session_id: session.id.clone(),
                    reason: "process exited".into(),
                },
            );
            let _ = app.emit(
                "connection-state",
                ConnectionStateEvent {
                    session_id: session.id.clone(),
                    state: ConnectionState::Disconnected,
                    message: None,
                },
            );
        })
    }

    fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    pub fn write(&self, id: &str, data: &[u8]) -> AppResult<()> {
        let session = self.get(id)?;
        if session.closed.load(Ordering::SeqCst) {
            return Err(AppError::Validation("session is not connected".into()));
        }
        session.provider().write(data)
    }

    pub fn resize(&self, id: &str, dims: TerminalDimensions) -> AppResult<()> {
        let session = self.get(id)?;
        session.provider().resize(dims)
    }

    /// Snapshot the bounded recent-output buffer for a session.
    pub fn snapshot(&self, id: &str) -> AppResult<Vec<u8>> {
        Ok(self.get(id)?.output.lock().snapshot())
    }

    /// Wait until `pattern` (a bounded regex) appears in the session's recent
    /// output, or the timeout elapses (FR-20 `waitFor`/`readUntil`).
    pub fn wait_for(&self, id: &str, pattern: &str, timeout_ms: u64) -> AppResult<WaitResult> {
        if pattern.len() > 4096 {
            return Err(AppError::Validation("pattern is too long".into()));
        }
        let re = regex::Regex::new(pattern)
            .map_err(|e| AppError::Validation(format!("invalid regex: {e}")))?;
        let session = self.get(id)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.min(600_000));
        loop {
            let snap = session.output.lock().snapshot();
            let text = String::from_utf8_lossy(&snap);
            if let Some(m) = re.find(&text) {
                return Ok(WaitResult {
                    matched: true,
                    text: m.as_str().to_string(),
                });
            }
            if Instant::now() >= deadline || !session.provider().is_alive() {
                return Ok(wait_timeout(&text));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Terminate a session and reclaim its child process and reader thread.
    /// Idempotent: closing an unknown/already-closed id is not an error.
    pub fn close(&self, id: &str) -> AppResult<()> {
        let session = self.sessions.lock().remove(id);
        if let Some(session) = session {
            let _ = session.provider().kill();
            if let Some(handle) = session.reader_thread.lock().take() {
                let _ = handle.join();
            }
            self.emit_state(id, ConnectionState::Closed, None);
        }
        Ok(())
    }

    /// Establish a genuinely new provider under the same runtime id, keeping
    /// the session's identity in the UI while replacing the process/socket.
    pub fn reconnect(&self, id: &str, dims: TerminalDimensions) -> AppResult<()> {
        let session = self.get(id)?;
        self.emit_state(id, ConnectionState::Reconnecting, None);

        let _ = session.provider().kill();
        if let Some(handle) = session.reader_thread.lock().take() {
            let _ = handle.join();
        }

        let provider = self.build_provider(&session.spec, dims)?;
        let reader = provider.take_reader()?;
        *session.provider.lock() = Arc::from(provider);
        session.closed.store(false, Ordering::SeqCst);

        let handle = self.spawn_reader(session.clone(), reader);
        *session.reader_thread.lock() = Some(handle);

        self.emit_state(id, ConnectionState::Connected, None);
        Ok(())
    }

    fn emit_state(&self, id: &str, state: ConnectionState, message: Option<String>) {
        let _ = self.app.emit(
            "connection-state",
            ConnectionStateEvent {
                session_id: id.to_string(),
                state,
                message,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::RingBuffer;

    #[test]
    fn timeout_tail_handles_unicode_larger_than_4096_bytes() {
        let text = "界".repeat(1400);
        let result = super::wait_timeout(&text);
        assert!(!result.matched);
        assert!(result.text.len() <= 4096);
        assert!(text.ends_with(&result.text));
        assert!(!result.text.is_empty());
    }

    #[test]
    fn ring_buffer_keeps_most_recent_bytes() {
        let mut rb = RingBuffer::new(8);
        rb.push(b"abc");
        rb.push(b"defgh");
        // Total 8 bytes: exactly fits.
        assert_eq!(rb.snapshot(), b"abcdefgh");
        rb.push(b"XY");
        // Oldest bytes evicted first.
        assert_eq!(rb.snapshot(), b"cdefghXY");
        assert_eq!(rb.len(), 8);
    }
}
