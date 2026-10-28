//! Content-free stall diagnostics; never inspect terminal data or block the UI.
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    NativeLoop,
    Input,
    Frame,
    SessionOutput,
    TerminalPaint,
}

fn stalled_for(now_ms: u64, progress_ms: u64, minimized: bool) -> Option<u64> {
    let elapsed = now_ms.saturating_sub(progress_ms);
    (!minimized && elapsed >= 6_000).then_some(elapsed)
}

struct Progress {
    start: Instant,
    // Keep the stage and its timestamp in one coherent snapshot.
    snapshot: AtomicU64,
    minimized: AtomicBool,
}

pub struct Monitor {
    progress: Arc<Progress>,
    stop: Sender<()>,
}

impl Monitor {
    pub fn start(directory: PathBuf) -> std::io::Result<Self> {
        let progress = Arc::new(Progress {
            start: Instant::now(),
            snapshot: AtomicU64::new(Stage::NativeLoop as u64),
            minimized: AtomicBool::new(false),
        });
        let observed = progress.clone();
        let (stop, stopped) = channel();
        // No terminal text, input, session names, destinations or credentials enter
        // this worker. A missing frame can also mean compositor occlusion; logs
        // deliberately describe observations rather than asserting a deadlock.
        std::thread::Builder::new().name("opsterm-ui-watch".into()).spawn(move || {
            let run_id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let mut reported = None;
            let mut file = None;
            let mut written = 0;
            while stopped.recv_timeout(Duration::from_secs(1)).is_err_and(|e| {
                e == std::sync::mpsc::RecvTimeoutError::Timeout
            }) {
                let snapshot = observed.snapshot.load(Ordering::Acquire);
                let now = observed.start.elapsed().as_millis() as u64;
                if let Some(elapsed) = stalled_for(now, snapshot >> 8,
                    observed.minimized.load(Ordering::Acquire)) {
                    if reported == Some(snapshot) { continue; }
                    reported = Some(snapshot);
                    let stage = match snapshot & 0xff {
                        0 => "native-event-loop-or-render",
                        1 => "input",
                        2 => "application-frame",
                        3 => "session-output",
                        4 => "terminal-paint",
                        _ => "unknown",
                    };
                    let line = format!("version={} pid={} unix_ms={} elapsed_ms={} stage={} main_wait={} main_syscall={}\n",
                        env!("CARGO_PKG_VERSION"), std::process::id(),
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), elapsed, stage,
                        main_probe("wchan"), main_probe("syscall"));
                    if written + line.len() > 64 * 1024 { continue; }
                    if file.is_none() { file = open_log(&directory, run_id).ok(); }
                    if let Some(log) = file.as_mut() {
                        if log.write_all(line.as_bytes()).is_ok() { written += line.len(); }
                    }
                }
            }
        })?;
        Ok(Self { progress, stop })
    }

    pub fn mark(&self, stage: Stage) {
        let milliseconds = self.progress.start.elapsed().as_millis() as u64;
        self.progress
            .snapshot
            .store((milliseconds << 8) | stage as u64, Ordering::Release);
    }

    pub fn minimized(&self, minimized: bool) {
        self.progress.minimized.store(minimized, Ordering::Release);
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

fn main_probe(name: &str) -> String {
    // Only the kernel wait-function name or syscall number, never syscall args.
    fs::read_to_string(format!("/proc/self/task/{}/{name}", std::process::id()))
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_owned))
        .filter(|text| {
            text.len() <= 80
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        })
        .unwrap_or_else(|| "unavailable".into())
}

fn open_log(directory: &std::path::Path, run_id: u128) -> std::io::Result<File> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(directory)?.file_type().is_dir() {
                return Err(std::io::Error::other("diagnostic path is not a directory"));
            }
        }
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(directory.join(format!("ui-stalls-{run_id:039}-{}.log", std::process::id())))?;
    prune_logs(directory);
    Ok(file)
}

fn prune_logs(directory: &std::path::Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut expired: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let name = entry.file_name().into_string().ok()?;
            let (stamp, pid) = name
                .strip_prefix("ui-stalls-")?
                .strip_suffix(".log")?
                .split_once('-')?;
            stamp.parse::<u128>().ok()?;
            let pid = pid.parse::<u32>().ok()?;
            // Keep live processes' evidence, even if several application windows exist.
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                return None;
            }
            Some((name, entry.path()))
        })
        .collect();
    expired.sort_unstable();
    let remove = expired.len().saturating_sub(8);
    for (_, path) in expired.into_iter().take(remove) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_stalled_progress_but_not_regular_frames() {
        assert_eq!(stalled_for(10_000, 9_500, false), None);
        assert_eq!(stalled_for(10_000, 3_000, false), Some(7_000));
    }

    #[test]
    fn stalled_worker_writes_a_private_content_free_report() {
        let root = std::env::temp_dir().join(format!(
            "opsterm-ui-watch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let monitor = Monitor::start(root.clone()).unwrap();
        monitor.mark(Stage::SessionOutput);
        let deadline = Instant::now() + Duration::from_secs(10);
        let text = loop {
            let file = fs::read_dir(&root)
                .ok()
                .and_then(|mut files| files.next())
                .and_then(Result::ok)
                .map(|entry| entry.path());
            if let Some(text) = file.as_ref().and_then(|file| fs::read_to_string(file).ok()) {
                if !text.is_empty() {
                    break text;
                }
            }
            assert!(Instant::now() < deadline, "missing automatic stall report");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(text.contains("stage=session-output"));
        assert!(text.contains("main_syscall="));
        assert_eq!(text.lines().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
            assert_eq!(
                fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        monitor.mark(Stage::NativeLoop);
        drop(monitor);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retention_preserves_live_logs_and_unrelated_files() {
        let root =
            std::env::temp_dir().join(format!("opsterm-ui-retention-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        for stamp in 0..12 {
            fs::write(
                root.join(format!("ui-stalls-{stamp:039}-4294967295.log")),
                "old",
            )
            .unwrap();
        }
        let live = root.join(format!("ui-stalls-{:039}-{}.log", 0, std::process::id()));
        fs::write(&live, "live").unwrap();
        fs::write(root.join("unrelated.log"), "preserve").unwrap();
        prune_logs(&root);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 10); // 8 expired + live + unrelated.
        assert!(live.exists());
        assert_eq!(
            fs::read_to_string(root.join("unrelated.log")).unwrap(),
            "preserve"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_file_never_follows_a_symlink_or_overwrites_existing_data() {
        let root = std::env::temp_dir().join(format!("opsterm-ui-log-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let log = root.join(format!("ui-stalls-{:039}-{}.log", 42, std::process::id()));
        let sentinel = root.join("sentinel");
        fs::write(&sentinel, "preserve").unwrap();
        std::os::unix::fs::symlink(&sentinel, &log).unwrap();
        assert!(open_log(&root, 42).is_err());
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn minimized_windows_are_not_reported_as_stalls() {
        assert_eq!(stalled_for(60_000, 1, true), None);
    }
}
