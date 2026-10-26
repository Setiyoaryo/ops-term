//! Bounded batch-process I/O and lifetime management for SFTP.

use crate::error::{AppError, AppResult};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

struct BatchChild {
    child: Child,
    finished: bool,
}

impl Drop for BatchChild {
    fn drop(&mut self) {
        if !self.finished {
            // The child is deliberately not reaped until its pipes close, so
            // its PID cannot be reused while we address its process group.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn nonblocking(fd: i32) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn drain(reader: &mut impl Read, output: &mut Vec<u8>) -> io::Result<bool> {
    let mut buf = [0; 8192];
    // Continuous server output must not prevent checking the deadline.
    for _ in 0..8 {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                if output.len() + n > OUTPUT_LIMIT {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "SFTP output exceeds 8 MiB",
                    ));
                }
                output.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

pub(super) fn run(mut command: Command, input: &[u8], timeout: Duration) -> AppResult<Output> {
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let mut process = BatchChild {
        child,
        finished: false,
    };
    let mut stdin = process.child.stdin.take();
    let mut stdout = process.child.stdout.take().expect("piped stdout");
    let mut stderr = process.child.stderr.take().expect("piped stderr");
    nonblocking(stdin.as_ref().expect("piped stdin").as_raw_fd())?;
    nonblocking(stdout.as_raw_fd())?;
    nonblocking(stderr.as_raw_fd())?;
    let mut sent = 0;
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut out_done, mut err_done) = (false, false);
    let started = Instant::now();
    loop {
        if started.elapsed() >= timeout {
            return Err(AppError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "SFTP operation timed out",
            )));
        }
        if let Some(pipe) = stdin.as_mut() {
            match pipe.write(&input[sent..]) {
                Ok(n) => sent += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            }
            if sent == input.len() {
                stdin.take();
            }
        }
        if !out_done {
            out_done = drain(&mut stdout, &mut out)?;
        }
        if !err_done {
            err_done = drain(&mut stderr, &mut err)?;
        }
        if out_done && err_done {
            if let Some(status) = process.child.try_wait()? {
                process.finished = true;
                return Ok(Output {
                    status,
                    stdout: out,
                    stderr: err,
                });
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
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
