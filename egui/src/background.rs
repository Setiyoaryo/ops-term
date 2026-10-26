//! Single-result background work; polling never waits for I/O.

use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

pub struct Task<T> {
    rx: Receiver<T>,
    wake: crate::repaint::Wake,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(job: impl FnOnce() -> T + Send + 'static) -> Result<Self, String> {
        let (tx, rx) = sync_channel(1);
        let wake = crate::repaint::Wake::default();
        let notify = wake.clone();
        std::thread::Builder::new()
            .name("opsterm-worker".into())
            .spawn(move || {
                let _ = tx.send(job());
                notify.request();
            })
            .map_err(|e| format!("could not start worker: {e}"))?;
        Ok(Self { rx, wake })
    }

    pub fn bind(&self, ctx: &eframe::egui::Context) {
        self.wake.bind(ctx);
    }

    pub fn poll(&self) -> Result<Option<T>, String> {
        match self.rx.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("background worker stopped unexpectedly".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn blocked_job_does_not_block_submission_or_polling() {
        let (release, wait) = mpsc::channel();
        let started = Instant::now();
        let task = Task::spawn(move || {
            wait.recv().unwrap();
            42
        })
        .unwrap();
        assert_eq!(task.poll().unwrap(), None);
        release.send(()).unwrap();
        loop {
            if let Some(value) = task.poll().unwrap() {
                assert_eq!(value, 42);
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::yield_now();
        }
    }

    #[test]
    fn panicked_worker_is_reported_instead_of_remaining_pending() {
        let task = Task::<()>::spawn(|| panic!("worker failed")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Err(error) = task.poll() {
                assert!(error.contains("worker"));
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
}
