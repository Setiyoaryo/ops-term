//! Serial console provider (FR-15).
//!
//! Opens a Linux serial device directly and configures it with termios. No
//! libudev dependency: devices are enumerated by scanning `/dev`. The port is
//! held exclusively for the life of the session.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use super::ConnectionProvider;
use crate::domain::{SerialSpec, TerminalDimensions};
use crate::error::{AppError, AppResult};

pub struct SerialSession {
    file: Mutex<File>,
    reader: Mutex<Option<Box<dyn Read + Send>>>,
    alive: Arc<AtomicBool>,
    cancel: UnixStream,
    cancel_reader: UnixStream,
    newline: String,
}

/// Both reads and writes wait on the serial port AND a shutdown socket. The
/// device stays nonblocking so a readiness race cannot leave shutdown stuck.
fn wait_ready(fd: i32, cancel: i32, events: i16, alive: &AtomicBool) -> io::Result<bool> {
    while alive.load(Ordering::SeqCst) {
        let mut fds = [
            libc::pollfd {
                fd,
                events,
                revents: 0,
            },
            libc::pollfd {
                fd: cancel,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if fds[1].revents != 0 || !alive.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if fds[0].revents != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

struct SerialReader {
    file: File,
    cancel: UnixStream,
    alive: Arc<AtomicBool>,
}

impl Read for SerialReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if !wait_ready(
                self.file.as_raw_fd(),
                self.cancel.as_raw_fd(),
                libc::POLLIN,
                &self.alive,
            )? {
                return Ok(0);
            }
            match self.file.read(buf) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue
                }
                result => {
                    if !matches!(&result, Ok(n) if *n > 0) {
                        self.alive.store(false, Ordering::SeqCst);
                    }
                    return result;
                }
            }
        }
    }
}

/// Enumerate likely serial devices present on the system.
pub fn list_devices() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("ttyUSB")
                || name.starts_with("ttyACM")
                || name.starts_with("ttyS")
                || name.starts_with("ttyAMA")
            {
                out.push(format!("/dev/{name}"));
            }
        }
    }
    out.sort();
    out
}

fn baud_speed(baud: u32) -> AppResult<libc::speed_t> {
    Ok(match baud {
        1200 => libc::B1200,
        2400 => libc::B2400,
        4800 => libc::B4800,
        9600 => libc::B9600,
        19200 => libc::B19200,
        38400 => libc::B38400,
        57600 => libc::B57600,
        115200 => libc::B115200,
        230400 => libc::B230400,
        other => {
            return Err(AppError::Validation(format!(
                "unsupported baud rate: {other}"
            )))
        }
    })
}

fn configure(fd: libc::c_int, spec: &SerialSpec) -> AppResult<()> {
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut tio) != 0 {
            return Err(AppError::Io(std::io::Error::last_os_error()));
        }
        libc::cfmakeraw(&mut tio);

        let speed = baud_speed(spec.baud)?;
        libc::cfsetispeed(&mut tio, speed);
        libc::cfsetospeed(&mut tio, speed);

        tio.c_cflag &= !libc::CSIZE;
        tio.c_cflag |= match spec.data_bits {
            5 => libc::CS5,
            6 => libc::CS6,
            7 => libc::CS7,
            _ => libc::CS8,
        };

        tio.c_cflag &= !(libc::PARENB | libc::PARODD);
        match spec.parity.as_str() {
            "odd" => tio.c_cflag |= libc::PARENB | libc::PARODD,
            "even" => tio.c_cflag |= libc::PARENB,
            _ => {}
        }

        if spec.stop_bits == 2 {
            tio.c_cflag |= libc::CSTOPB;
        } else {
            tio.c_cflag &= !libc::CSTOPB;
        }

        match spec.flow_control.as_str() {
            "hardware" => {
                tio.c_cflag |= libc::CRTSCTS;
                tio.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
            }
            "software" => {
                tio.c_iflag |= libc::IXON | libc::IXOFF;
                tio.c_cflag &= !libc::CRTSCTS;
            }
            _ => {
                tio.c_cflag &= !libc::CRTSCTS;
                tio.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
            }
        }

        tio.c_cflag |= libc::CLOCAL | libc::CREAD;
        tio.c_cc[libc::VMIN] = 1;
        tio.c_cc[libc::VTIME] = 0;

        if libc::tcsetattr(fd, libc::TCSANOW, &tio) != 0 {
            return Err(AppError::Io(std::io::Error::last_os_error()));
        }
        libc::tcflush(fd, libc::TCIOFLUSH);
    }
    Ok(())
}

pub fn spawn_serial(
    spec: &SerialSpec,
    _dims: TerminalDimensions,
) -> AppResult<Box<dyn ConnectionProvider>> {
    spec.validate()?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
        .open(&spec.device)
        .map_err(|e| {
            AppError::Io(std::io::Error::new(
                e.kind(),
                format!("{}: {e} (is the user in the 'dialout' group?)", spec.device),
            ))
        })?;

    configure(file.as_raw_fd(), spec)?;

    let (cancel_reader, cancel) = UnixStream::pair()?;
    let alive = Arc::new(AtomicBool::new(true));
    let reader = SerialReader {
        file: file.try_clone()?,
        cancel: cancel_reader.try_clone()?,
        alive: alive.clone(),
    };
    Ok(Box::new(SerialSession {
        file: Mutex::new(file),
        reader: Mutex::new(Some(Box::new(reader))),
        alive,
        cancel,
        cancel_reader,
        newline: spec.newline.clone(),
    }))
}

/// Convert outgoing bytes to the configured line-ending convention.
fn transform_newline(data: &[u8], mode: &str) -> Vec<u8> {
    match mode {
        "cr" => data.to_vec(),
        "lf" => data
            .iter()
            .map(|&b| if b == b'\r' { b'\n' } else { b })
            .collect(),
        // crlf
        _ => {
            let mut out = Vec::with_capacity(data.len() + 8);
            for &b in data {
                if b == b'\r' {
                    out.push(b'\r');
                    out.push(b'\n');
                } else {
                    out.push(b);
                }
            }
            out
        }
    }
}

impl ConnectionProvider for SerialSession {
    fn write(&self, data: &[u8]) -> AppResult<()> {
        let transformed = transform_newline(data, &self.newline);
        let mut file = self.file.lock();
        let mut remaining = transformed.as_slice();
        while !remaining.is_empty() {
            if !wait_ready(
                file.as_raw_fd(),
                self.cancel_reader.as_raw_fd(),
                libc::POLLOUT,
                &self.alive,
            )? {
                return Err(AppError::Io(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "serial session closed",
                )));
            }
            match file.write(remaining) {
                Ok(0) => {
                    return Err(AppError::Io(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "serial write returned zero",
                    )))
                }
                Ok(n) => remaining = &remaining[n..],
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            }
        }
        file.flush()?;
        Ok(())
    }

    fn resize(&self, _dims: TerminalDimensions) -> AppResult<()> {
        // Serial lines have no window-size concept; resize is a no-op.
        Ok(())
    }

    fn kill(&self) -> AppResult<()> {
        self.alive.store(false, Ordering::SeqCst);
        // Never acquire the writer lock here: shutdown must also cancel a
        // write waiting for a device whose hardware flow control is stopped.
        let _ = self.cancel.shutdown(Shutdown::Both);
        Ok(())
    }

    fn take_reader(&self) -> AppResult<Box<dyn Read + Send>> {
        self.reader
            .lock()
            .take()
            .ok_or_else(|| AppError::Internal("reader already taken".into()))
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn send_break(&self) -> AppResult<()> {
        let file = self.file.lock();
        unsafe {
            libc::tcsendbreak(file.as_raw_fd(), 0);
        }
        Ok(())
    }
}

impl Drop for SerialSession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::FromRawFd;
    use std::sync::mpsc;
    use std::time::Duration;

    fn pseudo_serial() -> (File, Box<dyn ConnectionProvider>) {
        let (mut master, mut slave) = (-1, -1);
        let mut name = [0i8; 128];
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    name.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let device = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned();
        let provider = spawn_serial(
            &SerialSpec {
                device,
                baud: 9600,
                data_bits: 8,
                parity: "none".into(),
                stop_bits: 1,
                flow_control: "none".into(),
                newline: "cr".into(),
            },
            TerminalDimensions { cols: 80, rows: 24 },
        )
        .unwrap();
        drop(slave);
        (master, provider)
    }

    #[test]
    fn idle_reader_stops_when_serial_is_killed() {
        let (mut master, provider) = pseudo_serial();
        let mut reader = provider.take_reader().unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut buf = [0; 16];
            tx.send(reader.read(&mut buf).unwrap()).unwrap();
        });
        std::thread::sleep(Duration::from_millis(20));
        provider.kill().unwrap();
        provider.kill().unwrap();
        let stopped = rx.recv_timeout(Duration::from_millis(500));
        if stopped.is_err() {
            // Unblock the old broken implementation so this regression cannot hang.
            let _ = master.write_all(b"x");
            let _ = rx.recv_timeout(Duration::from_secs(1));
        }
        handle.join().unwrap();
        assert_eq!(
            stopped.unwrap(),
            0,
            "shutdown must wake an idle reader with EOF"
        );
    }

    #[test]
    fn serial_reads_data_and_empty_buffers_without_disconnect() {
        let (mut master, provider) = pseudo_serial();
        let mut reader = provider.take_reader().unwrap();
        assert_eq!(reader.read(&mut []).unwrap(), 0);
        master.write_all(b"hello").unwrap();
        let mut buf = [0; 5];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
        provider.kill().unwrap();
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn shutdown_cancels_a_write_to_a_full_serial_port() {
        let (_master, provider) = pseudo_serial();
        let provider: Arc<dyn ConnectionProvider> = Arc::from(provider);
        let writer = provider.clone();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            tx.send(writer.write(&vec![b'x'; 1024 * 1024]).is_err())
                .unwrap();
        });
        std::thread::sleep(Duration::from_millis(20));
        provider.kill().unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap());
        handle.join().unwrap();
    }

    #[test]
    fn dropping_serial_provider_wakes_its_taken_reader() {
        let (_master, provider) = pseudo_serial();
        let mut reader = provider.take_reader().unwrap();
        drop(provider);
        assert_eq!(reader.read(&mut [0; 16]).unwrap(), 0);
    }

    #[test]
    fn newline_modes() {
        assert_eq!(transform_newline(b"a\rb", "cr"), b"a\rb");
        assert_eq!(transform_newline(b"a\rb", "lf"), b"a\nb");
        assert_eq!(transform_newline(b"a\r", "crlf"), b"a\r\n");
    }

    #[test]
    fn enumerates_only_serial_like_devices() {
        for d in list_devices() {
            assert!(d.starts_with("/dev/"));
        }
    }
}
