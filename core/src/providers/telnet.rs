//! Telnet provider (FR-14).
//!
//! A dedicated Telnet client with explicit IAC negotiation. Negotiation bytes
//! are consumed here and never rendered as terminal text. Telnet is
//! unencrypted; the UI must label it as insecure before use.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::ConnectionProvider;
use crate::domain::{TelnetSpec, TerminalDimensions};
use crate::error::{AppError, AppResult};

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const ECHO: u8 = 1;
const SGA: u8 = 3;
const TTYPE: u8 = 24;
const NAWS: u8 = 31;
const IS: u8 = 0;
const SEND: u8 = 1;

/// A `Read` over processed (negotiation-stripped) bytes arriving on a channel.
struct ChannelReader {
    rx: Receiver<Vec<u8>>,
    cur: Vec<u8>,
    pos: usize,
}

impl ChannelReader {
    fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            cur: Vec::new(),
            pos: 0,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.pos < self.cur.len() {
                let n = buf.len().min(self.cur.len() - self.pos);
                buf[..n].copy_from_slice(&self.cur[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            match self.rx.recv() {
                Ok(v) => {
                    self.cur = v;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
    }
}

/// Opt-in, prompt-driven Telnet auto-login. Deliberately conservative: it
/// answers the first `Username:`/`login:` and `Password:` prompts only, using
/// prompt *tails* to avoid matching banner text, and never retries (so a wrong
/// credential cannot cause an account lockout loop).
pub(crate) struct LoginAutomation {
    username: Option<String>,
    password: Option<String>,
    sent_user: bool,
    sent_pass: bool,
    buf: String,
}

impl LoginAutomation {
    pub(crate) fn new(username: Option<String>, password: Option<String>) -> Self {
        Self {
            username: username.filter(|u| !u.is_empty()),
            password: password.filter(|p| !p.is_empty()),
            sent_user: false,
            sent_pass: false,
            buf: String::new(),
        }
    }

    pub(crate) fn sent_user(&self) -> bool {
        self.sent_user
    }

    /// The last non-empty line, lower-cased. Matching on the last line is far
    /// more robust than matching the whole buffer (banners, echo, line wraps).
    fn last_line(&self) -> String {
        self.buf
            .rsplit(['\n', '\r'])
            .find(|s| !s.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_lowercase()
    }

    /// Feed decoded output; return bytes to send when a prompt is recognised.
    pub(crate) fn on_output(&mut self, text: &str) -> Option<Vec<u8>> {
        self.buf.push_str(text);
        crate::text::keep_tail(&mut self.buf, 1024);
        let last = self.last_line();
        if !self.sent_user
            && (last.ends_with("username:")
                || last.ends_with("login:")
                || last.ends_with("user name:")
                || last.ends_with("name:"))
        {
            if let Some(user) = self.username.clone() {
                self.sent_user = true;
                self.buf.clear();
                return Some(line(&user));
            }
        }
        if self.sent_user
            && !self.sent_pass
            && (last.ends_with("password:")
                || last.ends_with("passwd:")
                || last.ends_with("password"))
        {
            if let Some(pass) = self.password.clone() {
                self.sent_pass = true;
                self.buf.clear();
                return Some(line(&pass));
            }
        }
        None
    }
}

fn line(value: &str) -> Vec<u8> {
    // A single CR is what a terminal sends on Enter; an extra LF would be read
    // as a blank line by many devices and cause a duplicate prompt.
    let mut v = value.as_bytes().to_vec();
    v.push(b'\r');
    v
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    HaveIac,
    HaveCmd,
    Subneg,
}

/// Incremental Telnet option negotiator. Handles sequences that straddle read
/// boundaries and emits reply bytes through a closure.
struct Negotiator {
    state: State,
    pending_cmd: u8,
    subneg: Vec<u8>,
    last_was_iac: bool,
}

impl Negotiator {
    fn new() -> Self {
        Self {
            state: State::Normal,
            pending_cmd: 0,
            subneg: Vec::new(),
            last_was_iac: false,
        }
    }

    /// Feed raw socket bytes; append user data to `out`, send replies via
    /// `reply`, and record the current window size via `size` when asked.
    fn feed(
        &mut self,
        input: &[u8],
        out: &mut Vec<u8>,
        reply: &mut dyn FnMut(&[u8]),
        term: &str,
        dims: TerminalDimensions,
    ) {
        for &b in input {
            match self.state {
                State::Normal => {
                    if b == IAC {
                        self.state = State::HaveIac;
                    } else {
                        out.push(b);
                    }
                }
                State::HaveIac => match b {
                    IAC => {
                        out.push(IAC);
                        self.state = State::Normal;
                    }
                    DO | DONT | WILL | WONT => {
                        self.pending_cmd = b;
                        self.state = State::HaveCmd;
                    }
                    SB => {
                        self.subneg.clear();
                        self.state = State::Subneg;
                        self.last_was_iac = false;
                    }
                    _ => self.state = State::Normal,
                },
                State::HaveCmd => {
                    self.handle_option(self.pending_cmd, b, reply, term, dims);
                    self.state = State::Normal;
                }
                State::Subneg => {
                    if self.last_was_iac {
                        self.last_was_iac = false;
                        if b == SE {
                            self.handle_subneg(reply, term);
                            self.state = State::Normal;
                        } else if b == IAC {
                            self.subneg.push(IAC);
                        } else {
                            self.subneg.push(b);
                        }
                    } else if b == IAC {
                        self.last_was_iac = true;
                    } else {
                        self.subneg.push(b);
                    }
                }
            }
        }
    }

    fn handle_option(
        &self,
        cmd: u8,
        opt: u8,
        reply: &mut dyn FnMut(&[u8]),
        term: &str,
        dims: TerminalDimensions,
    ) {
        match cmd {
            DO => match opt {
                // We are a client: the server echoes, we do not.
                ECHO => reply(&[IAC, WONT, ECHO]),
                // Suppress Go-Ahead.
                SGA => reply(&[IAC, WILL, SGA]),
                // Terminal type + window size are supported.
                TTYPE => {
                    reply(&[IAC, WILL, TTYPE]);
                    let mut seq = vec![IAC, SB, TTYPE, IS];
                    seq.extend_from_slice(term.as_bytes());
                    seq.extend_from_slice(&[IAC, SE]);
                    reply(&seq);
                }
                NAWS => {
                    reply(&[IAC, WILL, NAWS]);
                    reply(&naws(dims));
                }
                other => reply(&[IAC, WONT, other]),
            },
            WILL => match opt {
                ECHO | SGA => reply(&[IAC, DO, opt]),
                other => reply(&[IAC, DONT, other]),
            },
            // Refusals are acknowledged implicitly; no reply needed.
            DONT | WONT => {}
            _ => {}
        }
    }

    fn handle_subneg(&self, reply: &mut dyn FnMut(&[u8]), term: &str) {
        if self.subneg.len() >= 2 && self.subneg[0] == TTYPE && self.subneg[1] == SEND {
            let mut seq = vec![IAC, SB, TTYPE, IS];
            seq.extend_from_slice(term.as_bytes());
            seq.extend_from_slice(&[IAC, SE]);
            reply(&seq);
        }
    }
}

/// Build an `IAC SB NAWS` window-size subnegotiation.
fn naws(dims: TerminalDimensions) -> Vec<u8> {
    vec![
        IAC,
        SB,
        NAWS,
        (dims.cols >> 8) as u8,
        (dims.cols & 0xff) as u8,
        (dims.rows >> 8) as u8,
        (dims.rows & 0xff) as u8,
        IAC,
        SE,
    ]
}

pub struct TelnetSession {
    writer: Arc<Mutex<TcpStream>>,
    shutdown: TcpStream,
    reader: Mutex<Option<Box<dyn Read + Send>>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    dims: Mutex<TerminalDimensions>,
}

pub fn spawn_telnet(
    spec: &TelnetSpec,
    dims: TerminalDimensions,
) -> AppResult<Box<dyn ConnectionProvider>> {
    dims.validate()?;
    let host = crate::security::validate_host(&spec.host)?;
    crate::security::validate_port(spec.port)?;
    let term = spec.term.clone().unwrap_or_else(|| "xterm-256color".into());

    let addr = format!("{host}:{}", spec.port);
    let stream = TcpStream::connect(&addr)
        .map_err(|e| AppError::Io(io::Error::new(e.kind(), format!("connect {addr}: {e}"))))?;
    stream.set_read_timeout(Some(Duration::from_millis(300)))?;
    let _ = stream.set_nodelay(true);

    let shutdown = stream.try_clone()?;
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let stop = Arc::new(AtomicBool::new(false));
    let alive = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(32);

    // Auto-login when a username and/or an inventory password are available.
    let login = if spec
        .username
        .as_deref()
        .map(|u| !u.is_empty())
        .unwrap_or(false)
        || spec
            .password
            .as_deref()
            .map(|p| !p.is_empty())
            .unwrap_or(false)
    {
        Some(LoginAutomation::new(
            spec.username.clone(),
            spec.password.clone(),
        ))
    } else {
        None
    };

    let writer_thread = writer.clone();
    let stop_thread = stop.clone();
    let alive_thread = alive.clone();
    let term_thread = term.clone();
    std::thread::spawn(move || {
        serve(
            stream,
            writer_thread,
            tx,
            stop_thread,
            term_thread,
            dims,
            login,
        );
        alive_thread.store(false, Ordering::SeqCst);
    });

    Ok(Box::new(TelnetSession {
        writer,
        shutdown,
        reader: Mutex::new(Some(Box::new(ChannelReader::new(rx)))),
        stop,
        alive,
        dims: Mutex::new(dims),
    }))
}

fn serve(
    mut stream: TcpStream,
    writer: Arc<Mutex<TcpStream>>,
    tx: mpsc::SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    term: String,
    dims: TerminalDimensions,
    mut login: Option<LoginAutomation>,
) {
    let mut negotiator = Negotiator::new();
    let mut buf = [0u8; 4096];
    let start = Instant::now();
    let mut nudged = false;
    while !stop.load(Ordering::SeqCst) {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut out = Vec::with_capacity(n);
                let mut replies: Vec<Vec<u8>> = Vec::new();
                let mut reply = |bytes: &[u8]| replies.push(bytes.to_vec());
                negotiator.feed(&buf[..n], &mut out, &mut reply, &term, dims);
                for r in replies {
                    let mut guard = writer.lock();
                    let _ = guard.write_all(&r);
                }
                if let Some(login) = login.as_mut() {
                    let text = String::from_utf8_lossy(&out);
                    if let Some(answer) = login.on_output(&text) {
                        let mut guard = writer.lock();
                        let _ = guard.write_all(&answer);
                        let _ = guard.flush();
                    }
                }
                if !out.is_empty() && tx.send(out).is_err() {
                    break;
                }
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                // Some telnet consoles only print the login prompt after an
                // initial Enter. Nudge once if no prompt appeared in time.
                if !nudged {
                    if let Some(login) = login.as_ref() {
                        if !login.sent_user() && start.elapsed() >= Duration::from_millis(1200) {
                            let mut guard = writer.lock();
                            let _ = guard.write_all(b"\r");
                            let _ = guard.flush();
                            nudged = true;
                        }
                    }
                }
                continue;
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    // The reader ends with EOF when `tx` drops here.
}

impl ConnectionProvider for TelnetSession {
    fn write(&self, data: &[u8]) -> AppResult<()> {
        let mut writer = self.writer.lock();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    fn resize(&self, dims: TerminalDimensions) -> AppResult<()> {
        dims.validate()?;
        *self.dims.lock() = dims;
        let mut writer = self.writer.lock();
        writer.write_all(&naws(dims))?;
        writer.flush()?;
        Ok(())
    }

    fn kill(&self) -> AppResult<()> {
        self.stop.store(true, Ordering::SeqCst);
        self.alive.store(false, Ordering::SeqCst);
        // A blocked write holds the writer mutex; shutdown must bypass it.
        let _ = self.shutdown.shutdown(std::net::Shutdown::Both);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims() -> TerminalDimensions {
        TerminalDimensions { cols: 80, rows: 24 }
    }

    fn feed(input: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut n = Negotiator::new();
        let mut out = Vec::new();
        let mut replies: Vec<Vec<u8>> = Vec::new();
        let mut reply = |b: &[u8]| replies.push(b.to_vec());
        n.feed(input, &mut out, &mut reply, "xterm-256color", dims());
        (out, replies)
    }

    #[test]
    fn passes_plain_data_through() {
        let (out, replies) = feed(b"hello\r\n");
        assert_eq!(out, b"hello\r\n");
        assert!(replies.is_empty());
    }

    #[test]
    fn strips_and_answers_sga() {
        let (out, replies) = feed(&[IAC, DO, SGA, b'x']);
        assert_eq!(out, b"x");
        assert_eq!(replies, vec![vec![IAC, WILL, SGA]]);
    }

    #[test]
    fn refuses_echo_and_unknown_options() {
        let (_, replies) = feed(&[IAC, DO, ECHO, IAC, DO, 42]);
        assert_eq!(replies[0], vec![IAC, WONT, ECHO]);
        assert_eq!(replies[1], vec![IAC, WONT, 42]);
    }

    #[test]
    fn answers_naws_with_window_size() {
        let (_, replies) = feed(&[IAC, DO, NAWS]);
        assert_eq!(replies[0], vec![IAC, WILL, NAWS]);
        assert_eq!(replies[1], vec![IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]);
    }

    #[test]
    fn terminal_type_subnegotiation_replies() {
        let (_, replies) = feed(&[IAC, SB, TTYPE, SEND, IAC, SE]);
        let expected: Vec<u8> = [&[IAC, SB, TTYPE, IS][..], b"xterm-256color", &[IAC, SE]].concat();
        assert_eq!(replies[0], expected);
    }

    #[test]
    fn auto_login_handles_long_unicode_banner() {
        let mut login = LoginAutomation::new(Some("admin".into()), None);
        let banner = format!("{}\r\nUsername: ", "界".repeat(400));
        assert_eq!(login.on_output(&banner), Some(b"admin\r".to_vec()));
        assert!(login.buf.len() <= 1024);
    }

    #[test]
    fn auto_login_answers_username_then_password_once() {
        let mut login = LoginAutomation::new(Some("admin".into()), Some("s3cret".into()));
        let out = login.on_output("Welcome to device\r\nUsername: ").unwrap();
        assert_eq!(out, b"admin\r");
        let out = login.on_output("Password: ").unwrap();
        assert_eq!(out, b"s3cret\r");
        // Never retries, even if prompted again.
        assert!(login.on_output("Password: ").is_none());
        assert!(login.on_output("Username: ").is_none());
    }

    #[test]
    fn auto_login_matches_last_line_and_ignores_banner() {
        let mut login = LoginAutomation::new(Some("admin".into()), Some("pw".into()));
        // A banner mentioning "username" without a colon is ignored.
        assert!(login
            .on_output("### username must be valid ###\r\n")
            .is_none());
        // Banner + prompt in one chunk: last line is the prompt.
        let out = login.on_output("login to device\r\nUser Name: ").unwrap();
        assert_eq!(out, b"admin\r");
        assert!(login.on_output("Password").is_some()); // colon optional
    }

    #[test]
    fn auto_login_without_password_only_answers_username() {
        let mut login = LoginAutomation::new(Some("admin".into()), None);
        let out = login.on_output("Username: ").unwrap();
        assert_eq!(out, b"admin\r");
        // No password configured: the prompt is left to the user.
        assert!(login.on_output("Password: ").is_none());
    }

    #[test]
    fn handles_literal_ff_and_split_sequences() {
        // Literal 0xFF is escaped as IAC IAC.
        let (out, _) = feed(&[IAC, IAC]);
        assert_eq!(out, vec![IAC]);
        // A DO that straddles two feeds is still negotiated once.
        let mut n = Negotiator::new();
        let mut out = Vec::new();
        let mut replies: Vec<Vec<u8>> = Vec::new();
        {
            let mut reply = |b: &[u8]| replies.push(b.to_vec());
            n.feed(&[IAC], &mut out, &mut reply, "xterm", dims());
            n.feed(&[DO, SGA], &mut out, &mut reply, "xterm", dims());
        }
        assert_eq!(replies, vec![vec![IAC, WILL, SGA]]);
    }

    #[test]
    fn shutdown_cancels_a_write_to_a_nonreading_peer() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let provider: Arc<dyn ConnectionProvider> = Arc::from(
            spawn_telnet(
                &TelnetSpec {
                    host: "127.0.0.1".into(),
                    port: listener.local_addr().unwrap().port(),
                    username: None,
                    password: None,
                    term: None,
                },
                dims(),
            )
            .unwrap(),
        );
        let (peer, _) = listener.accept().unwrap();
        let writer = provider.clone();
        let (write_tx, write_rx) = mpsc::channel();
        let writing = std::thread::spawn(move || {
            let result = writer.write(&vec![b'x'; 16 * 1024 * 1024]);
            let _ = write_tx.send(result.is_err());
        });
        assert!(write_rx.recv_timeout(Duration::from_millis(100)).is_err());
        let closing = provider.clone();
        let (close_tx, close_rx) = mpsc::channel();
        let close = std::thread::spawn(move || {
            let _ = close_tx.send(closing.kill().is_ok());
        });
        let promptly = close_rx.recv_timeout(Duration::from_millis(200));
        // Release the old implementation even when the regression fails.
        let _ = peer.shutdown(std::net::Shutdown::Both);
        drop(peer);
        writing.join().unwrap();
        close.join().unwrap();
        assert!(
            promptly.unwrap_or(false),
            "close waited for the blocked writer mutex"
        );
        assert!(!provider.is_alive());
    }
}
