//! Keep blocking transport calls off the egui thread, with bounded pending input.
use opsterm_core::domain::TerminalDimensions;
use opsterm_core::error::{AppError, AppResult};
use opsterm_core::providers::ConnectionProvider;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_OPERATIONS: usize = 64;
const MAX_PENDING_BYTES: usize = 1024 * 1024;

struct Shared {
    alive: AtomicBool,
    busy: AtomicBool,
    closed: AtomicBool,
    bytes: AtomicUsize,
    error: Mutex<Option<String>>,
    wake: crate::repaint::Wake,
}

#[derive(Clone)]
pub struct Status(Arc<Shared>);

impl Status {
    pub fn bind(&self, ctx: &eframe::egui::Context) {
        self.0.wake.bind(ctx);
    }
    pub fn take_error(&self) -> Option<String> {
        self.0
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

enum Operation {
    Write(Vec<u8>),
    Resize(TerminalDimensions),
    Break,
}

struct Responsive {
    transport: Arc<dyn ConnectionProvider>,
    tx: SyncSender<Operation>,
    reader: Mutex<Option<Box<dyn Read + Send>>>,
    status: Status,
}

fn input_error(message: &str) -> AppError {
    AppError::Validation(message.into())
}

impl Responsive {
    fn submit(&self, operation: Operation) -> AppResult<()> {
        if self.status.0.closed.load(Ordering::Acquire) {
            return Err(input_error("session is closed; input was not sent"));
        }
        self.tx.try_send(operation).map_err(|_| {
            input_error(
                "input queue is full or closed; input was not sent, retry when the device is ready",
            )
        })
    }
}

impl ConnectionProvider for Responsive {
    fn write(&self, data: &[u8]) -> AppResult<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.status
            .0
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                pending
                    .checked_add(data.len())
                    .filter(|&total| total <= MAX_PENDING_BYTES)
            })
            .map_err(|_| {
                input_error(
                    "pending input exceeds 1 MiB; input was not sent, paste a smaller section",
                )
            })?;
        let result = self.submit(Operation::Write(data.to_vec()));
        if result.is_err() {
            self.status.0.bytes.fetch_sub(data.len(), Ordering::AcqRel);
        }
        result
    }
    fn resize(&self, dims: TerminalDimensions) -> AppResult<()> {
        dims.validate()?;
        self.submit(Operation::Resize(dims))
    }
    fn send_break(&self) -> AppResult<()> {
        self.submit(Operation::Break)
    }
    fn kill(&self) -> AppResult<()> {
        self.status.0.alive.store(false, Ordering::Release);
        if self.status.0.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        self.status.0.wake.request();
        // Closing must bypass a blocked write rather than queue behind it.
        let transport = self.transport.clone();
        std::thread::Builder::new()
            .name("opsterm-close".into())
            .spawn(move || {
                let _ = transport.kill();
            })
            .map_err(|_| input_error("could not start session shutdown worker"))?;
        Ok(())
    }
    fn is_alive(&self) -> bool {
        self.status.0.alive.load(Ordering::Acquire)
    }
    fn has_running_command(&self) -> bool {
        self.status.0.bytes.load(Ordering::Acquire) > 0
            || self.status.0.busy.load(Ordering::Acquire)
    }
    fn take_reader(&self) -> AppResult<Box<dyn Read + Send>> {
        self.reader
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or_else(|| input_error("reader already taken"))
    }
}
impl Drop for Responsive {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

pub fn responsive(
    provider: Box<dyn ConnectionProvider>,
) -> AppResult<(Box<dyn ConnectionProvider>, Status)> {
    let reader = match provider.take_reader() {
        Ok(reader) => reader,
        Err(error) => {
            let _ = provider.kill();
            return Err(error);
        }
    };
    let transport: Arc<dyn ConnectionProvider> = Arc::from(provider);
    let status = Status(Arc::new(Shared {
        alive: AtomicBool::new(true),
        busy: AtomicBool::new(true),
        closed: AtomicBool::new(false),
        bytes: AtomicUsize::new(0),
        error: Mutex::new(None),
        wake: crate::repaint::Wake::default(),
    }));
    let (tx, rx) = sync_channel(MAX_OPERATIONS);
    let worker = transport.clone();
    let state = status.clone();
    if std::thread::Builder::new().name("opsterm-session-io".into()).spawn(move || {
        let mut settle_until = std::time::Instant::now();
        while !state.0.closed.load(Ordering::Acquire) {
            let result = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(_) if state.0.closed.load(Ordering::Acquire) => break,
                Ok(Operation::Write(bytes)) => {
                    // PTY writes can return before the shell creates its child.
                    // Keep a short grace interval while the shell consumes input.
                    state.0.busy.store(true, Ordering::Release);
                    let result = worker.write(&bytes);
                    settle_until = std::time::Instant::now() + Duration::from_millis(300);
                    state.0.bytes.fetch_sub(bytes.len(), Ordering::AcqRel);
                    result
                }
                Ok(Operation::Resize(dims)) => worker.resize(dims),
                Ok(Operation::Break) => worker.send_break(),
                Err(RecvTimeoutError::Timeout) => Ok(()),
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if result.is_err() {
                if !state.0.closed.load(Ordering::Acquire) {
                    // Do not expose arbitrary transport error content or input bytes.
                    *state.0.error.lock().unwrap_or_else(|e| e.into_inner()) = Some("Session I/O failed; reconnect and check the device before resending commands.".into());
                }
                break;
            }
            if !worker.is_alive() { break; }
            let busy = worker.has_running_command() || std::time::Instant::now() < settle_until;
            if state.0.busy.swap(busy, Ordering::AcqRel) != busy {
                state.0.wake.request();
            }
        }
        state.0.closed.store(true, Ordering::Release);
        state.0.alive.store(false, Ordering::Release);
        state.0.wake.request();
        let _ = worker.kill();
    }).is_err() {
        let _ = transport.kill();
        return Err(input_error("could not start session I/O worker"));
    }
    Ok((
        Box::new(Responsive {
            transport,
            tx,
            reader: Mutex::new(Some(reader)),
            status: status.clone(),
        }),
        status,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsterm_core::{domain::TerminalDimensions, error::AppResult};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    struct Stalled {
        release: Mutex<mpsc::Receiver<()>>,
        entered: mpsc::Sender<()>,
    }
    impl ConnectionProvider for Stalled {
        fn write(&self, _: &[u8]) -> AppResult<()> {
            self.entered.send(()).unwrap();
            let _ = self.release.lock().unwrap().recv();
            Ok(())
        }
        fn resize(&self, _: TerminalDimensions) -> AppResult<()> {
            Ok(())
        }
        fn kill(&self) -> AppResult<()> {
            Ok(())
        }
        fn take_reader(&self) -> AppResult<Box<dyn std::io::Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }
        fn is_alive(&self) -> bool {
            true
        }
    }

    #[test]
    fn blocked_transport_write_does_not_block_ui_submission() {
        let (release, wait) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let provider: Arc<dyn ConnectionProvider> = Arc::from(
            responsive(Box::new(Stalled {
                release: Mutex::new(wait),
                entered,
            }))
            .unwrap()
            .0,
        );
        let (done, completion) = mpsc::channel();
        let writer = provider.clone();
        let handle = std::thread::spawn(move || done.send(writer.write(b"paste").is_ok()).unwrap());
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let responsive = completion.recv_timeout(Duration::from_millis(100));
        release.send(()).unwrap();
        handle.join().unwrap();
        provider.kill().unwrap();
        assert!(
            responsive.unwrap_or(false),
            "UI waited for a stalled transport write"
        );
    }

    #[test]
    fn bounded_queue_and_close_remain_responsive_behind_a_blocked_write() {
        let (release, wait) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let (provider, status) = responsive(Box::new(Stalled {
            release: Mutex::new(wait),
            entered,
        }))
        .unwrap();
        provider.write(b"first").unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..MAX_OPERATIONS {
            provider.write(b"queued").unwrap();
        }
        assert!(provider.write(b"overflow").is_err());
        assert!(provider.write(&vec![b'x'; MAX_PENDING_BYTES + 1]).is_err());
        assert!(status.0.bytes.load(Ordering::Acquire) <= MAX_PENDING_BYTES);
        let before = std::time::Instant::now();
        provider.kill().unwrap();
        assert!(!provider.is_alive());
        assert!(provider.write(b"after close").is_err());
        assert!(before.elapsed() < Duration::from_millis(100));
        release.send(()).unwrap();
        assert!(
            started.recv_timeout(Duration::from_millis(200)).is_err(),
            "queued input was sent after close"
        );
    }

    struct Recording {
        events: mpsc::Sender<String>,
        blocked: Option<(&'static str, Mutex<mpsc::Receiver<()>>)>,
        fail: bool,
    }
    impl Recording {
        fn operation(&self, event: &str) -> AppResult<()> {
            let _ = self.events.send(event.to_owned());
            if let Some((kind, release)) = &self.blocked {
                if event == *kind {
                    let _ = release.lock().unwrap().recv();
                }
            }
            if self.fail && event != "kill" {
                Err(input_error("sensitive transport message"))
            } else {
                Ok(())
            }
        }
    }
    impl ConnectionProvider for Recording {
        fn write(&self, data: &[u8]) -> AppResult<()> {
            self.operation(std::str::from_utf8(data).unwrap())
        }
        fn resize(&self, _: TerminalDimensions) -> AppResult<()> {
            self.operation("resize")
        }
        fn send_break(&self) -> AppResult<()> {
            self.operation("break")
        }
        fn kill(&self) -> AppResult<()> {
            self.operation("kill")
        }
        fn take_reader(&self) -> AppResult<Box<dyn Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }
        fn is_alive(&self) -> bool {
            let _ = self.operation("probe");
            true
        }
    }

    #[test]
    fn input_resize_and_break_keep_their_order() {
        let (events, received) = mpsc::channel();
        let (provider, _) = responsive(Box::new(Recording {
            events,
            blocked: None,
            fail: false,
        }))
        .unwrap();
        provider.write(b"one").unwrap();
        provider
            .resize(TerminalDimensions {
                cols: 120,
                rows: 40,
            })
            .unwrap();
        provider.write(b"two").unwrap();
        provider.send_break().unwrap();
        for expected in ["one", "resize", "two", "break"] {
            loop {
                let actual = received.recv_timeout(Duration::from_secs(2)).unwrap();
                if actual != "probe" {
                    assert_eq!(actual, expected);
                    break;
                }
            }
        }
        provider.kill().unwrap();
    }

    #[test]
    fn stalled_resize_probe_and_shutdown_never_wait_on_the_ui() {
        for operation in ["resize", "probe", "kill"] {
            let (release, wait) = mpsc::channel();
            let (events, received) = mpsc::channel();
            let (provider, _) = responsive(Box::new(Recording {
                events,
                blocked: Some((operation, Mutex::new(wait))),
                fail: false,
            }))
            .unwrap();
            let before = std::time::Instant::now();
            match operation {
                "resize" => provider
                    .resize(TerminalDimensions {
                        cols: 100,
                        rows: 30,
                    })
                    .unwrap(),
                "kill" => provider.kill().unwrap(),
                _ => assert!(provider.is_alive()),
            }
            assert!(before.elapsed() < Duration::from_millis(100));
            loop {
                if received.recv_timeout(Duration::from_secs(2)).unwrap() == operation {
                    break;
                }
            }
            assert_eq!(provider.is_alive(), operation != "kill");
            drop(provider);
            release.send(()).unwrap();
        }
    }

    #[test]
    fn asynchronous_failure_is_visible_without_exposing_transport_content() {
        let (events, _) = mpsc::channel();
        let (provider, status) = responsive(Box::new(Recording {
            events,
            blocked: None,
            fail: true,
        }))
        .unwrap();
        provider.write(b"failing").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while provider.is_alive() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let error = status.take_error().unwrap();
        assert!(error.contains("Session I/O failed"));
        assert!(!error.contains("sensitive"));
        assert!(provider.write(b"retry").is_err());
    }

    struct Idle {
        write_entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl ConnectionProvider for Idle {
        fn write(&self, _: &[u8]) -> AppResult<()> {
            self.write_entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            Ok(())
        }
        fn resize(&self, _: TerminalDimensions) -> AppResult<()> {
            Ok(())
        }
        fn kill(&self) -> AppResult<()> {
            Ok(())
        }
        fn take_reader(&self) -> AppResult<Box<dyn Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }
        fn is_alive(&self) -> bool {
            true
        }
        fn has_running_command(&self) -> bool {
            false
        }
    }

    #[test]
    fn cached_idle_probe_marks_queued_input_busy_until_worker_settles() {
        let (entered, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (provider, _) = responsive(Box::new(Idle {
            write_entered: entered,
            release: Mutex::new(wait),
        }))
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while provider.has_running_command() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        provider.write(b"sleep 30\n").unwrap();
        assert!(
            provider.has_running_command(),
            "queued Enter was treated as idle"
        );
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let before = std::time::Instant::now();
        assert!(provider.has_running_command());
        assert!(before.elapsed() < Duration::from_millis(100));
        release.send(()).unwrap();
        // Even a transport that reports idle immediately cannot erase the
        // shell scheduling grace after accepting a command.
        std::thread::sleep(Duration::from_millis(20));
        assert!(provider.has_running_command());
        while provider.has_running_command() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        provider.kill().unwrap();
    }

    #[test]
    fn real_local_pty_with_undrained_output_accepts_input_and_closes_without_waiting() {
        let transport = opsterm_core::providers::local_pty::spawn_local_shell(TerminalDimensions {
            cols: 80,
            rows: 24,
        })
        .unwrap();
        let (provider, _) = responsive(transport).unwrap();
        let _undrained_reader = provider.take_reader().unwrap();
        let before = std::time::Instant::now();
        provider.write(&vec![b'x'; MAX_PENDING_BYTES]).unwrap();
        provider.kill().unwrap();
        assert!(!provider.is_alive());
        assert!(before.elapsed() < Duration::from_millis(200));
    }
}
