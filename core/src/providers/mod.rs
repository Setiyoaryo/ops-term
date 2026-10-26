//! Connection providers.
//!
//! A `ConnectionProvider` is the replaceable seam between the session
//! supervisor and whatever actually carries bytes. M1 ships two
//! implementations — a local PTY shell and an OpenSSH client running under a
//! PTY. A future native SSH engine (e.g. `russh`) can be added here without
//! touching the UI (PRD FR-01 / section 7.2).

pub mod local_pty;
pub mod openssh_pty;
pub mod russh_ssh;
pub mod serial;
pub mod sftp_pty;
pub mod telnet;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::domain::TerminalDimensions;
use crate::error::{AppError, AppResult};

/// A live, resizable byte pipe to a remote device or local shell.
pub trait ConnectionProvider: Send + Sync {
    /// Write raw bytes to the child's stdin/PTY.
    fn write(&self, data: &[u8]) -> AppResult<()>;
    /// Resize the underlying PTY so the child is told the new grid size.
    fn resize(&self, dims: TerminalDimensions) -> AppResult<()>;
    /// Terminate the child process.
    fn kill(&self) -> AppResult<()>;
    /// Take ownership of the output stream. Callable exactly once.
    fn take_reader(&self) -> AppResult<Box<dyn Read + Send>>;
    /// Whether the child process is still running.
    fn is_alive(&self) -> bool;
    /// Send a serial break if the transport supports it. Default: unsupported.
    fn send_break(&self) -> AppResult<()> {
        Err(AppError::Validation(
            "break is only supported on serial sessions".into(),
        ))
    }
}

/// Shared PTY plumbing used by both M1 providers. Owns the master side of a
/// pseudo-terminal, a writer handle, the child process, and the not-yet-taken
/// reader.
pub(crate) struct PtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    reader: Mutex<Option<Box<dyn Read + Send>>>,
    alive: AtomicBool,
    /// A temporary directory (e.g. the askpass helper) to delete when the
    /// session is closed.
    cleanup: Mutex<Option<PathBuf>>,
}

impl PtySession {
    /// Spawn `cmd` attached to a freshly allocated PTY sized to `dims`.
    pub(crate) fn spawn(cmd: CommandBuilder, dims: TerminalDimensions) -> AppResult<Self> {
        dims.validate()?;
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: dims.rows,
                cols: dims.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::Pty(e.to_string()))?;

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AppError::Pty(e.to_string()))?;
        // Drop the slave handle so the child sees EOF when it exits.
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| AppError::Pty(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| AppError::Pty(e.to_string()))?;

        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            reader: Mutex::new(Some(reader)),
            alive: AtomicBool::new(true),
            cleanup: Mutex::new(None),
        })
    }

    /// Register a temporary directory to remove when the session is killed.
    pub(crate) fn set_cleanup(&self, dir: PathBuf) {
        *self.cleanup.lock() = Some(dir);
    }
}

impl ConnectionProvider for PtySession {
    fn write(&self, data: &[u8]) -> AppResult<()> {
        let mut writer = self.writer.lock();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    fn resize(&self, dims: TerminalDimensions) -> AppResult<()> {
        dims.validate()?;
        self.master
            .lock()
            .resize(PtySize {
                rows: dims.rows,
                cols: dims.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::Pty(e.to_string()))
    }

    fn kill(&self) -> AppResult<()> {
        let mut child = self.child.lock();
        let _ = child.kill();
        self.alive.store(false, Ordering::SeqCst);
        if let Some(dir) = self.cleanup.lock().take() {
            let _ = std::fs::remove_dir_all(dir);
        }
        Ok(())
    }

    fn take_reader(&self) -> AppResult<Box<dyn Read + Send>> {
        self.reader
            .lock()
            .take()
            .ok_or_else(|| AppError::Internal("reader already taken".into()))
    }

    fn is_alive(&self) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        let mut child = self.child.lock();
        match child.try_wait() {
            Ok(Some(_)) => {
                self.alive.store(false, Ordering::SeqCst);
                false
            }
            Ok(None) => true,
            Err(_) => false,
        }
    }
}
