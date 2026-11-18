//! Bounded batch-process I/O and lifetime management for SFTP.

use crate::error::{AppError, AppResult};
use std::io::{self, Read, Write};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

struct BatchChild {
    child: Child,
    finished: bool,
}

impl Drop for BatchChild {
    fn drop(&mut self) {
        if !self.finished {
            // Unix children are started in their own group so a shell wrapper
            // cannot leave descendants behind. `kill` covers every platform.
            #[cfg(unix)]
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn timeout_error() -> AppError {
    AppError::Io(io::Error::new(
        io::ErrorKind::TimedOut,
        "SFTP operation timed out",
    ))
}

fn limit_error() -> AppError {
    AppError::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "SFTP output exceeds 8 MiB",
    ))
}

fn read_bounded(
    mut reader: impl Read + Send + 'static,
    stderr: bool,
    tx: Sender<(bool, io::Result<Vec<u8>>)>,
) {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if output.len() + n > OUTPUT_LIMIT {
                        let _ = tx.send((
                            stderr,
                            Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "SFTP output exceeds 8 MiB",
                            )),
                        ));
                        return;
                    }
                    output.extend_from_slice(&buffer[..n]);
                }
                Err(error) => {
                    let _ = tx.send((stderr, Err(error)));
                    return;
                }
            }
        }
        let _ = tx.send((stderr, Ok(output)));
    });
}

pub(super) fn run(mut command: Command, input: &[u8], timeout: Duration) -> AppResult<Output> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut process = BatchChild {
        child,
        finished: false,
    };
    let mut stdin = process.child.stdin.take().expect("piped stdin");
    let stdout = process.child.stdout.take().expect("piped stdout");
    let stderr = process.child.stderr.take().expect("piped stderr");
    let (tx, rx) = channel();
    read_bounded(stdout, false, tx.clone());
    read_bounded(stderr, true, tx);
    let input = input.to_vec();
    std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });

    let started = Instant::now();
    let (mut out, mut err) = (None, None);
    while out.is_none() || err.is_none() {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .ok_or_else(timeout_error)?;
        match rx.recv_timeout(remaining) {
            Ok((true, Ok(bytes))) => err = Some(bytes),
            Ok((false, Ok(bytes))) => out = Some(bytes),
            Ok((_, Err(error))) if error.kind() == io::ErrorKind::InvalidData => {
                return Err(limit_error())
            }
            Ok((_, Err(error))) => return Err(error.into()),
            Err(_) => return Err(timeout_error()),
        }
    }
    let status = process.child.wait()?;
    process.finished = true;
    Ok(Output {
        status,
        stdout: out.unwrap_or_default(),
        stderr: err.unwrap_or_default(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn stalled_batch_is_killed_and_reaped_at_its_deadline() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 0.2"]);
        let start = Instant::now();
        let result = run(command, b"bye\n", Duration::from_millis(20));
        assert!(
            result.is_err(),
            "a stalled batch must fail rather than complete late"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn successful_batch_preserves_stdin_and_both_output_streams() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "cat; printf error >&2"]);
        let output = run(command, b"commands\n", Duration::from_secs(2)).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"commands\n");
        assert_eq!(output.stderr, b"error");
    }
}
