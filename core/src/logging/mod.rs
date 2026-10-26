//! Terminal logging / evidence capture (PRD FR-09).
//!
//! Logging is opt-in, per session, and writes to a file created with mode
//! `0600` inside a directory we own. Remote output is treated as sensitive: it
//! is written raw, never uploaded, and never placed in the app database. The
//! database only keeps a small index (path, times, size).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::error::{AppError, AppResult};

/// Roll to a new file once a log exceeds this size, so a long-running session
/// cannot fill the disk with one unbounded file.
const MAX_LOG_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LogOptions {
    pub label: Option<String>,
    pub session_id: Option<String>,
    pub mode: String,
    pub timestamps: bool,
}

struct ActiveLog {
    id: String,
    path: PathBuf,
    file: File,
    bytes: u64,
    rot_index: u32,
    timestamps: bool,
    line_start: bool,
    session_id: Option<String>,
    label: Option<String>,
}

/// Manager of live log files, keyed by runtime session id.
pub struct LogManager {
    dir: PathBuf,
    active: Mutex<HashMap<String, ActiveLog>>,
}

/// A filename-safe rendering of an arbitrary label.
fn sanitize(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for c in segment.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches(|c| c == '_' || c == '.');
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

impl LogManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            active: Mutex::new(HashMap::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Create the log file for `key` and register it. Returns the `(id, path)`.
    pub fn start(&self, key: &str, opts: LogOptions) -> AppResult<(String, PathBuf)> {
        std::fs::create_dir_all(&self.dir)?;
        let id = uuid::Uuid::new_v4().to_string();
        let ts = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let label = sanitize(opts.label.as_deref().unwrap_or("session"));
        let path = self.dir.join(format!("{label}_{ts}.log"));
        let file = open_private(&path)?;
        let mut active = self.active.lock();
        if active.contains_key(key) {
            return Err(AppError::Validation(
                "logging already active for session".into(),
            ));
        }
        active.insert(
            key.to_string(),
            ActiveLog {
                id: id.clone(),
                path: path.clone(),
                file,
                bytes: 0,
                rot_index: 0,
                timestamps: opts.timestamps,
                line_start: true,
                session_id: opts.session_id,
                label: opts.label,
            },
        );
        Ok((id, path))
    }

    /// Whether logging is active for a session key.
    pub fn is_active(&self, key: &str) -> bool {
        self.active.lock().contains_key(key)
    }

    /// Append received bytes to the session's log, if any.
    pub fn write(&self, key: &str, data: &[u8]) {
        let mut active = self.active.lock();
        let Some(log) = active.get_mut(key) else {
            return;
        };
        let payload = if log.timestamps {
            add_timestamps(log, data)
        } else {
            data.to_vec()
        };
        if log.file.write_all(&payload).is_err() {
            return;
        }
        log.bytes += payload.len() as u64;
        if log.bytes >= MAX_LOG_BYTES {
            let _ = roll_over(&self.dir, log);
        }
    }

    /// Close the session log and return the final size in bytes.
    pub fn stop(&self, key: &str) -> Option<(String, i64, Option<String>, Option<String>)> {
        let mut active = self.active.lock();
        if let Some(mut log) = active.remove(key) {
            let _ = log.file.flush();
            Some((log.id, log.bytes as i64, log.session_id, log.label))
        } else {
            None
        }
    }
}

fn add_timestamps(log: &mut ActiveLog, data: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(data);
    let prefix = chrono::Local::now()
        .format("[%Y-%m-%d %H:%M:%S] ")
        .to_string();
    let mut out = String::with_capacity(text.len() + 32);
    for ch in text.chars() {
        if log.line_start {
            out.push_str(&prefix);
            log.line_start = false;
        }
        out.push(ch);
        if ch == '\n' {
            log.line_start = true;
        }
    }
    out.into_bytes()
}

fn roll_over(dir: &Path, log: &mut ActiveLog) -> AppResult<()> {
    let _ = log.file.flush();
    log.rot_index += 1;
    let stem = log
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".into());
    let path = dir.join(format!("{stem}.{}.log", log.rot_index));
    log.file = open_private(&path)?;
    log.path = path;
    log.bytes = 0;
    log.line_start = true;
    Ok(())
}

/// Open a log file with least privilege (`0600` on Unix).
fn open_private(path: &Path) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(AppError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_labels() {
        assert_eq!(sanitize("CORE-JKT-01"), "CORE-JKT-01");
        assert_eq!(sanitize("../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize("///"), "session");
    }

    #[test]
    fn writes_and_stops() {
        let dir = std::env::temp_dir().join(format!("opsterm-log-{}", uuid::Uuid::new_v4()));
        let mgr = LogManager::new(dir.clone());
        let (id, path) = mgr
            .start(
                "s1",
                LogOptions {
                    label: Some("test".into()),
                    session_id: Some("s1".into()),
                    mode: "raw".into(),
                    timestamps: false,
                },
            )
            .expect("start");
        assert!(mgr.is_active("s1"));
        mgr.write("s1", b"hello\nworld\n");
        let (stop_id, size, _, _) = mgr.stop("s1").expect("stop");
        assert_eq!(stop_id, id);
        assert_eq!(size, 12);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello\nworld\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
