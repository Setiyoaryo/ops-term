//! Telnet provider (FR-14).
//!
//! A dedicated Telnet client with explicit IAC negotiation. Negotiation bytes
//! are consumed here and never rendered as terminal text. Telnet is
//! unencrypted; the UI must label it as insecure before use.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// Bounded view of the current and last nonempty terminal lines. This is only
/// a prompt observer; it never changes the bytes rendered by the terminal.
#[derive(Default)]
struct PromptText {
    current: Vec<char>,
    previous: Vec<char>,
    column: usize,
    overlong: bool,
    previous_overlong: bool,
}

impl PromptText {
    const MAX_COLUMNS: usize = 1024;

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn last_line(&self) -> String {
        let (chars, overlong) = if self.current.iter().any(|c| !c.is_whitespace()) {
            (&self.current, self.overlong)
        } else {
            (&self.previous, self.previous_overlong)
        };
        // Never turn the truncated tail of an oversized banner into a prompt.
        if overlong {
            return String::new();
        }
        chars
            .iter()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
}

impl vte::Perform for PromptText {
    fn print(&mut self, c: char) {
        if self.column >= Self::MAX_COLUMNS {
            self.overlong = true;
            return;
        }
        self.current
            .resize(self.current.len().max(self.column + 1), ' ');
        self.current[self.column] = c;
        self.column += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.column = 0,
            b'\n' => {
                if self.current.iter().any(|c| !c.is_whitespace()) {
                    self.previous = std::mem::take(&mut self.current);
                    self.previous_overlong = self.overlong;
                }
                self.current.clear();
                self.column = 0;
                self.overlong = false;
            }
            8 => self.column = self.column.saturating_sub(1),
            b'\t' => self.column = ((self.column / 8 + 1) * 8).min(Self::MAX_COLUMNS),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore || !intermediates.is_empty() {
            return;
        }
        let value = params
            .iter()
            .next()
            .and_then(|p| p.first())
            .copied()
            .unwrap_or(0) as usize;
        match action {
            'K' => match value {
                0 => {
                    self.current.truncate(self.column);
                    self.overlong = false;
                }
                1 => {
                    let end = (self.column + 1).min(self.current.len());
                    self.current[..end].fill(' ');
                }
                2 => {
                    self.current.clear();
                    self.overlong = false;
                }
                _ => {}
            },
            'J' if value == 2 || value == 3 => self.clear(),
            'G' => self.column = value.max(1).saturating_sub(1).min(Self::MAX_COLUMNS),
            'C' => {
                self.column = self
                    .column
                    .saturating_add(value.max(1))
                    .min(Self::MAX_COLUMNS)
            }
            'D' => self.column = self.column.saturating_sub(value.max(1)),
            _ => {}
        }
    }
}

#[derive(Default)]
enum OscFilter {
    #[default]
    Ground,
    Escape,
    Osc,
    OscEscape,
}

/// Opt-in, prompt-driven Telnet auto-login. Only exact, normalized prompt
/// aliases are answered; password changes and banner suffixes are never filled.
/// Each configured credential is sent at most once, with no authentication retry.
pub(crate) struct LoginAutomation {
    username: Option<String>,
    password: Option<String>,
    sent_user: bool,
    sent_pass: bool,
    parser: vte::Parser,
    text: PromptText,
    osc: OscFilter,
    authenticating: Arc<AtomicBool>,
    authentication_barrier: Arc<AtomicU64>,
}

impl LoginAutomation {
    pub(crate) fn new(username: Option<String>, password: Option<String>) -> Self {
        let username = username.filter(|u| !u.trim().is_empty());
        let password = password.filter(|p| !p.is_empty());
        let authenticating = Arc::new(AtomicBool::new(username.is_some() || password.is_some()));
        Self {
            username,
            password,
            sent_user: false,
            sent_pass: false,
            parser: vte::Parser::new(),
            text: PromptText::default(),
            osc: OscFilter::Ground,
            authenticating,
            authentication_barrier: Arc::new(AtomicU64::new(0)),
        }
    }

    fn has_attempted_login(&self) -> bool {
        self.sent_user || self.sent_pass
    }

    /// Pending configured answers, not confirmation of server authentication.
    fn has_pending_answers(&self) -> bool {
        if self.password.is_some() {
            !self.sent_pass
        } else {
            self.username.is_some() && !self.sent_user
        }
    }

    /// Feed raw output, retaining ANSI and UTF-8 parser state across reads.
    pub(crate) fn on_output(&mut self, bytes: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        if !self.has_pending_answers() {
            return None;
        }
        // Discard OSC payloads before vte: its std-enabled OSC buffer is
        // unbounded, and title/clipboard text must never become a login prompt.
        for &byte in bytes.as_ref() {
            match self.osc {
                OscFilter::Ground if byte == 0x1b => self.osc = OscFilter::Escape,
                OscFilter::Ground => self.parser.advance(&mut self.text, &[byte]),
                OscFilter::Escape if byte == b']' => self.osc = OscFilter::Osc,
                OscFilter::Escape => {
                    self.parser.advance(&mut self.text, &[0x1b]);
                    if byte != 0x1b {
                        self.parser.advance(&mut self.text, &[byte]);
                        self.osc = OscFilter::Ground;
                    }
                }
                OscFilter::Osc | OscFilter::OscEscape if byte == 7 => {
                    self.osc = OscFilter::Ground;
                }
                OscFilter::OscEscape if byte == b'\\' => self.osc = OscFilter::Ground,
                OscFilter::Osc | OscFilter::OscEscape if byte == 0x1b => {
                    self.osc = OscFilter::OscEscape
                }
                OscFilter::Osc | OscFilter::OscEscape => self.osc = OscFilter::Osc,
            }
        }
        let last = self.text.last_line();
        let label = last.strip_suffix(':').map(str::trim_end);
        if !self.sent_user
            && !self.sent_pass
            && matches!(label, Some("username" | "login" | "user name" | "name"))
        {
            if let Some(user) = self.username.as_ref() {
                self.sent_user = true;
                self.text.clear();
                return Some(line(user));
            }
        }
        if !self.sent_pass && matches!(label.unwrap_or(&last), "password" | "passwd") {
            if let Some(pass) = self.password.as_ref() {
                self.sent_pass = true;
                self.text.clear();
                return Some(line(pass));
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
    authenticating: Arc<AtomicBool>,
    authentication_barrier: Arc<AtomicU64>,
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

    let authenticating = login
        .as_ref()
        .map(|login| login.authenticating.clone())
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let authentication_barrier = login
        .as_ref()
        .map(|login| login.authentication_barrier.clone())
        .unwrap_or_else(|| Arc::new(AtomicU64::new(0)));
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
        authenticating,
        authentication_barrier,
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
    let mut decoded_bytes = 0u64;
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
                // Count only bytes delivered to the terminal reader, including
                // the complete current chunk, before considering a prompt.
                decoded_bytes = decoded_bytes.saturating_add(out.len() as u64);
                if let Some(login) = login.as_mut() {
                    if let Some(answer) = login.on_output(&out) {
                        let mut guard = writer.lock();
                        if guard
                            .write_all(&answer)
                            .and_then(|()| guard.flush())
                            .is_ok()
                        {
                            let pending = login.has_pending_answers();
                            if !pending {
                                // Publish the output position before publishing
                                // the completed phase. The UI must render through
                                // this barrier before trusting a CLI prompt.
                                login
                                    .authentication_barrier
                                    .store(decoded_bytes, Ordering::Release);
                            }
                            login.authenticating.store(pending, Ordering::Release);
                        }
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
                        if !login.has_attempted_login()
                            && start.elapsed() >= Duration::from_millis(1200)
                        {
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

    fn is_authenticating(&self) -> bool {
        self.authenticating.load(Ordering::Acquire)
    }

    fn authentication_output_barrier(&self) -> Option<u64> {
        match self.authentication_barrier.load(Ordering::Acquire) {
            0 => None,
            barrier => Some(barrier),
        }
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
        assert!(login.text.current.len() <= PromptText::MAX_COLUMNS);
        assert!(login.text.previous.len() <= PromptText::MAX_COLUMNS);
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
    fn auto_login_answers_colored_prompts_with_space_before_colon() {
        let mut login = LoginAutomation::new(Some("admin".into()), Some("pw".into()));
        assert_eq!(
            login.on_output("banner\r\n\x1b[32mUsername : \x1b[0m"),
            Some(b"admin\r".to_vec())
        );
        assert_eq!(
            login.on_output("\x1b[33mPassword : \x1b[0m\r\n"),
            Some(b"pw\r".to_vec())
        );
    }

    #[test]
    fn auto_login_preserves_split_ansi_parser_state() {
        let mut login = LoginAutomation::new(Some("admin".into()), Some("pw".into()));
        assert!(login.on_output("\x1b[3").is_none());
        assert!(login.on_output("2mUser").is_none());
        assert!(login.on_output("na\x1b[").is_none());
        assert_eq!(login.on_output("0mme :"), Some(b"admin\r".to_vec()));
        assert!(login.on_output("\x1b]0;Password:").is_none());
        assert!(login.on_output("\x07Pass").is_none());
        assert_eq!(login.on_output("word : \x1b[0m"), Some(b"pw\r".to_vec()));
    }

    #[test]
    fn auto_login_answers_password_only_once_with_optional_username() {
        for username in [None, Some("admin".into())] {
            let mut login = LoginAutomation::new(username, Some("pw".into()));
            assert_eq!(login.on_output("Password: "), Some(b"pw\r".to_vec()));
            assert!(login.on_output("\r\nPassword: ").is_none());
            assert!(login.on_output("\r\nUsername: ").is_none());
        }
    }

    #[test]
    fn auto_login_does_not_fill_password_changes_or_banner_suffixes() {
        for prompt in [
            "New password:",
            "Old password:",
            "Current password:",
            "Confirm password:",
            "Retype password:",
            "Please change your password:",
            "Enter new password:",
            "Change user name:",
            "Device name:",
            "Last login:",
        ] {
            let mut login = LoginAutomation::new(Some("admin".into()), Some("pw".into()));
            assert!(login.on_output(prompt).is_none(), "answered {prompt}");
            let mut login = LoginAutomation::new(Some("admin".into()), Some("pw".into()));
            assert_eq!(login.on_output("Username:"), Some(b"admin\r".to_vec()));
            assert!(
                login.on_output(prompt).is_none(),
                "answered {prompt} after username"
            );
        }
    }

    #[test]
    fn auto_login_honors_erased_text_and_backspaces() {
        let mut login = LoginAutomation::new(Some("admin".into()), Some("pw".into()));
        assert_eq!(
            login.on_output("not a prompt\r\x1b[2KUsername :"),
            Some(b"admin\r".to_vec())
        );
        assert_eq!(login.on_output("Passwore\x08d :"), Some(b"pw\r".to_vec()));
    }

    #[test]
    fn authentication_barrier_covers_all_decoded_output_through_final_answer() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let provider = spawn_telnet(
            &TelnetSpec {
                host: "127.0.0.1".into(),
                port: listener.local_addr().unwrap().port(),
                username: None,
                password: Some("pw".into()),
                term: None,
            },
            dims(),
        )
        .unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = provider.take_reader().unwrap();
        let initial_barrier = provider.authentication_output_barrier();
        let banner = b"<OLD>\r\n";
        // Refused negotiation has no reply and must not count as reader output.
        peer.write_all(&[IAC, WONT, SGA]).unwrap();
        peer.write_all(banner).unwrap();
        let mut rendered = vec![0u8; banner.len()];
        reader.read_exact(&mut rendered).unwrap();
        let prompt = b"\x1b[33mPassword : \x1b[0m\r\n";
        peer.write_all(prompt).unwrap();
        let mut answer = [0u8; 3];
        let received = peer.read_exact(&mut answer);
        let deadline = Instant::now() + Duration::from_secs(1);
        while provider.is_authenticating() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let barrier = provider.authentication_output_barrier();
        let pending = provider.is_authenticating();
        // A socket write can arrive in multiple provider reads. ChannelReader
        // preserves those output chunks when its destination is at least the
        // provider's 4096-byte read buffer, so record their actual endpoints.
        let decoded_budget = banner.len() + prompt.len();
        let mut chunk_ends = Vec::new();
        let mut chunk = [0u8; 4096];
        while rendered.len() < decoded_budget {
            let n = reader.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            rendered.extend_from_slice(&chunk[..n]);
            chunk_ends.push(rendered.len() as u64);
        }
        provider.kill().unwrap();
        received.unwrap();
        assert_eq!(answer, *b"pw\r");
        assert_eq!(initial_barrier, None);
        assert!(!pending);
        assert_eq!(rendered, [banner.as_slice(), prompt.as_slice()].concat());
        // "Password" alone is a valid prompt. Later spaces, colon, reset and
        // CRLF count only if they arrived in the chunk that triggered it.
        let earliest_answer = (banner.len() + b"\x1b[33mPassword".len()) as u64;
        assert!(
            barrier.is_some_and(|end| end >= earliest_answer && chunk_ends.contains(&end)),
            "barrier {barrier:?} was not a causal output chunk end: {chunk_ends:?}"
        );
    }

    #[test]
    fn configured_login_phase_waits_for_last_credential_answer() {
        for (username, password, expected) in [
            (
                Some("admin".to_owned()),
                Some("pw".to_owned()),
                b"pw\r".as_slice(),
            ),
            (Some("admin".to_owned()), None, b"admin\r".as_slice()),
            (None, Some("pw".to_owned()), b"pw\r".as_slice()),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let provider = spawn_telnet(
                &TelnetSpec {
                    host: "127.0.0.1".into(),
                    port: listener.local_addr().unwrap().port(),
                    username: username.clone(),
                    password: password.clone(),
                    term: None,
                },
                dims(),
            )
            .unwrap();
            let (mut peer, _) = listener.accept().unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            // Capture before any prompt so the default-false trait stub fails
            // without leaving the session thread alive on assertion failure.
            let initially_pending = provider.is_authenticating();
            let mut intermediate_pending = true;
            let result = (|| -> io::Result<()> {
                let mut answer = vec![0u8; expected.len()];
                if username.is_some() && password.is_some() {
                    peer.write_all(b"Username:")?;
                    let mut user = [0u8; 6];
                    peer.read_exact(&mut user)?;
                    assert_eq!(&user, b"admin\r");
                    intermediate_pending = provider.is_authenticating();
                }
                peer.write_all(if password.is_some() {
                    b"Password:"
                } else {
                    b"Username:"
                })?;
                peer.read_exact(&mut answer)?;
                assert_eq!(answer, expected);
                let deadline = Instant::now() + Duration::from_secs(1);
                while provider.is_authenticating() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            })();
            let still_pending = provider.is_authenticating();
            provider.kill().unwrap();
            assert!(
                initially_pending,
                "configured answers were not marked pending"
            );
            result.unwrap();
            assert!(
                intermediate_pending,
                "phase ended before the password answer"
            );
            assert!(
                !still_pending,
                "phase did not finish after the final answer"
            );
        }
    }

    #[test]
    fn auto_login_preserves_utf8_split_across_socket_reads() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let provider = spawn_telnet(
            &TelnetSpec {
                host: "127.0.0.1".into(),
                port: listener.local_addr().unwrap().port(),
                username: Some("admin".into()),
                password: None,
                term: None,
            },
            dims(),
        )
        .unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = provider.take_reader().unwrap();
        peer.write_all(b"User\xe2").unwrap();
        let mut displayed = [0u8; 5];
        // Observe the first chunk before sending the rest, guaranteeing a split
        // in the provider's parser, without timing assumptions.
        reader.read_exact(&mut displayed).unwrap();
        peer.write_all(b"\x80\x83Name :").unwrap();
        let mut answer = [0u8; 6];
        let received = peer.read_exact(&mut answer);
        provider.kill().unwrap();
        received.unwrap();
        assert_eq!(&answer, b"admin\r");
    }

    #[test]
    fn password_only_login_does_not_send_a_later_enter_nudge() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let provider = spawn_telnet(
            &TelnetSpec {
                host: "127.0.0.1".into(),
                port: listener.local_addr().unwrap().port(),
                username: Some("admin".into()),
                password: Some("pw".into()),
                term: None,
            },
            dims(),
        )
        .unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        peer.write_all(b"Password: ").unwrap();
        let mut answer = [0u8; 16];
        let received = peer.read(&mut answer);
        // Always stop the provider, including on the RED path.
        let extra = if received.as_ref().is_ok_and(|n| &answer[..*n] == b"pw\r") {
            peer.read(&mut answer).ok()
        } else {
            Some(0)
        };
        provider.kill().unwrap();
        assert_eq!(
            received.unwrap(),
            3,
            "password-only prompt was not answered"
        );
        assert!(
            extra.is_none(),
            "unexpected bytes after password-only login"
        );
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
