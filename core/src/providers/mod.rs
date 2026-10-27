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
    /// Whether closing could interrupt a running command. Unknown transports
    /// conservatively return true. May inspect OS state; UI clients must cache
    /// this on their I/O worker rather than call it from the event loop.
    fn has_running_command(&self) -> bool {
        true
    }
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
    #[cfg(target_os = "linux")]
    executable: Option<PathBuf>,
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

        #[cfg(target_os = "linux")]
        let executable = cmd
            .get_argv()
            .first()
            .and_then(|path| std::fs::canonicalize(path).ok());
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
            #[cfg(target_os = "linux")]
            executable,
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

    fn has_running_command(&self) -> bool {
        if !self.alive.load(Ordering::Acquire) {
            return false;
        }
        #[cfg(unix)]
        {
            // Never wait behind a resize or shutdown, even for non-egui callers.
            let Some(child) = self.child.try_lock() else {
                return true;
            };
            let Some(pid) = child.process_id() else {
                return true;
            };
            drop(child);
            let Some(master) = self.master.try_lock() else {
                return true;
            };
            let foreground = master.process_group_leader();
            let termios = master.get_termios();
            drop(master);
            if foreground.map(|group| group as u32) != Some(pid) {
                return true;
            }
            // Readline and ZLE disable canonical input and kernel echo while
            // waiting at the prompt. Ordinary builtins (read, loops) restore
            // cooked mode even though PID, executable and children do not change.
            // Only recognize this positive editing signal; canonical/unknown
            // shells conservatively prompt. Builtins such as `read -e` can use
            // the same editor state and need shell integration to distinguish.
            let Some(termios) = termios else {
                return true;
            };
            if termios.local_flags.bits() & (libc::ICANON | libc::ECHO) != 0 {
                return true;
            }
            #[cfg(target_os = "linux")]
            {
                // Background/stopped jobs remain shell children even when the
                // shell owns the foreground. Missing procfs data is unknown.
                let Ok(children) =
                    std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
                else {
                    return true;
                };
                if !children.trim().is_empty() {
                    return true;
                }
                // `exec program` keeps the shell PID and process group.
                let Ok(current) = std::fs::read_link(format!("/proc/{pid}/exe")) else {
                    return true;
                };
                self.executable.as_ref() != Some(&current)
            }
            #[cfg(not(target_os = "linux"))]
            {
                true // No portable background-job / exec detection.
            }
        }
        #[cfg(not(unix))]
        {
            true
        }
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

#[cfg(all(test, target_os = "linux"))]
mod close_activity_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "PTY activity did not reach expected state"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn real_shell_activity_tracks_foreground_background_and_exec() {
        let mut command = CommandBuilder::new("/bin/bash");
        command.args(["--noprofile", "--norc", "-i"]);
        let session =
            PtySession::spawn(command, TerminalDimensions { cols: 80, rows: 24 }).unwrap();
        let mut reader = session.take_reader().unwrap();
        let drain = std::thread::spawn(move || {
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });
        wait_until(|| !session.has_running_command());
        session.write(b"sleep 30\n").unwrap();
        wait_until(|| session.has_running_command());
        session.write(&[3]).unwrap();
        wait_until(|| !session.has_running_command());
        session.write(b"sleep 30 &\n").unwrap();
        wait_until(|| session.has_running_command());
        session.write(b"kill -9 $!; wait\n").unwrap();
        wait_until(|| !session.has_running_command());
        session.write(b"exec sleep 30\n").unwrap();
        wait_until(|| session.has_running_command());
        session.kill().unwrap();
        drain.join().unwrap();
        assert!(!session.has_running_command());
    }

    fn builtin_stays_busy(command: &[u8]) {
        let mut shell = CommandBuilder::new("/bin/bash");
        shell.args(["--noprofile", "--norc", "-i"]);
        let session = PtySession::spawn(shell, TerminalDimensions { cols: 80, rows: 24 }).unwrap();
        let mut reader = session.take_reader().unwrap();
        let drain = std::thread::spawn(move || {
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });
        wait_until(|| !session.has_running_command());
        session.write(command).unwrap();
        // Beyond the UI worker's scheduling grace: shell builtins have no
        // external child and keep the shell executable and foreground group.
        std::thread::sleep(Duration::from_millis(400));
        let busy = session.has_running_command();
        session.write(&[3]).unwrap();
        wait_until(|| !session.has_running_command());
        session.kill().unwrap();
        drain.join().unwrap();
        assert!(
            busy,
            "running shell builtin was mistaken for an idle prompt"
        );
    }

    #[test]
    fn builtin_read_requires_close_confirmation() {
        builtin_stays_busy(b"read -r value\n");
    }

    #[test]
    fn builtin_loop_requires_close_confirmation() {
        builtin_stays_busy(b"while :; do :; done\n");
    }

    #[test]
    fn activity_probe_is_conservative_when_pty_lock_is_busy() {
        let session = PtySession::spawn(
            CommandBuilder::new("/bin/sh"),
            TerminalDimensions { cols: 80, rows: 24 },
        )
        .unwrap();
        {
            let _locked = session.master.lock();
            let before = Instant::now();
            assert!(session.has_running_command());
            assert!(before.elapsed() < Duration::from_millis(100));
        }
        session.kill().unwrap();
    }
}
