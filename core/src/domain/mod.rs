pub mod models;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub use models::*;

/// Terminal grid size, in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDimensions {
    pub cols: u16,
    pub rows: u16,
}

impl TerminalDimensions {
    /// Guards against zero and absurdly large dimensions that some terminal
    /// emulators emit during transient layout passes.
    pub fn validate(&self) -> AppResult<()> {
        if self.cols == 0 || self.rows == 0 {
            return Err(AppError::Validation(
                "terminal dimensions must be non-zero".into(),
            ));
        }
        if self.cols > 1000 || self.rows > 1000 {
            return Err(AppError::Validation(
                "terminal dimensions out of supported range".into(),
            ));
        }
        Ok(())
    }
}

/// Transport selected for a session.
///
/// `Ssh` and `Local` have live providers today; `Telnet` and `Serial` are
/// representable in saved profiles now and gain providers in Phase 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Ssh,
    Local,
    Telnet,
    Serial,
    Sftp,
}

/// Lifecycle state reported to the UI.
///
/// The mapping is intentionally conservative: OpenSSH's internal handshake
/// phases are not reliably observable from a subprocess, so we only report
/// states we can actually prove (see PRD FR-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
    Failed,
    Closed,
}

/// A typed, validated SSH connection request.
///
/// Fields are kept structurally separate (host, port, user, identity, jump) so
/// the argv builder never has to parse a command string and user input never
/// becomes an arbitrary process flag (PRD FR-01 / section 7.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshSpec {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub identity_file: Option<String>,
    #[serde(default)]
    pub proxy_jump: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// Terminal `TERM` value requested for the session.
    #[serde(default)]
    pub term: Option<String>,
    /// OpenSSH `ConnectTimeout`, in seconds.
    #[serde(default)]
    pub connect_timeout: Option<u16>,
    /// OpenSSH `ServerAliveInterval`, in seconds.
    #[serde(default)]
    pub server_alive_interval: Option<u16>,
    /// `-X` X11 forwarding (advanced; off by default).
    #[serde(default)]
    pub x11_forwarding: bool,
    /// `-A` agent forwarding (advanced; off by default).
    #[serde(default)]
    pub forward_agent: bool,
    /// `-C` compression.
    #[serde(default)]
    pub compression: bool,
    /// `-o CertificateFile=...` user certificate path.
    #[serde(default)]
    pub certificate_file: Option<String>,
    /// `-o PKCS11Provider=...` path to a PKCS#11 provider library.
    #[serde(default)]
    pub pkcs11_provider: Option<String>,
    /// SSH engine: `openssh` (default) or `native` (experimental russh).
    #[serde(default)]
    pub engine: Option<String>,
    /// Login password resolved from the credential inventory. Memory-only:
    /// never serialized across IPC and never persisted.
    #[serde(skip)]
    pub password: Option<String>,
}

pub fn default_ssh_port() -> u16 {
    22
}

/// A typed Telnet connection request (FR-14). Telnet is unencrypted and is
/// always labelled as such in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelnetSpec {
    pub host: String,
    #[serde(default = "default_telnet_port")]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub term: Option<String>,
    /// Login password from the credential inventory. Memory-only, never
    /// serialized. Used for opt-in prompt-driven auto-login.
    #[serde(skip)]
    pub password: Option<String>,
}

pub fn default_telnet_port() -> u16 {
    23
}

/// Serial line configuration (FR-15). Values mirror the common presets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialSpec {
    pub device: String,
    #[serde(default = "default_baud")]
    pub baud: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    /// `none`, `odd` or `even`.
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// `none`, `software` or `hardware`.
    #[serde(default = "default_flow")]
    pub flow_control: String,
    /// Newline mode: `cr`, `lf` or `crlf`.
    #[serde(default = "default_newline")]
    pub newline: String,
}

fn default_baud() -> u32 {
    9600
}
fn default_data_bits() -> u8 {
    8
}
fn default_parity() -> String {
    "none".into()
}
fn default_stop_bits() -> u8 {
    1
}
fn default_flow() -> String {
    "none".into()
}
fn default_newline() -> String {
    "crlf".into()
}

impl SerialSpec {
    pub fn validate(&self) -> AppResult<()> {
        if !self.device.starts_with('/') {
            return Err(AppError::Validation(
                "serial device must be an absolute path like /dev/ttyUSB0".into(),
            ));
        }
        if self.device.contains("..") || self.device.chars().any(|c| c.is_control()) {
            return Err(AppError::Validation("invalid serial device path".into()));
        }
        if !(5..=8).contains(&self.data_bits) {
            return Err(AppError::Validation("invalid data bits".into()));
        }
        if !matches!(self.stop_bits, 1 | 2) {
            return Err(AppError::Validation("invalid stop bits".into()));
        }
        if !matches!(self.parity.as_str(), "none" | "odd" | "even") {
            return Err(AppError::Validation("invalid parity".into()));
        }
        if !matches!(self.flow_control.as_str(), "none" | "software" | "hardware") {
            return Err(AppError::Validation("invalid flow control".into()));
        }
        if !matches!(self.newline.as_str(), "cr" | "lf" | "crlf") {
            return Err(AppError::Validation("invalid newline mode".into()));
        }
        if self.baud == 0 {
            return Err(AppError::Validation("baud rate must be non-zero".into()));
        }
        Ok(())
    }
}

/// Persisted-for-lifetime description of what a provider should run, so a
/// session can be reconnected without the UI re-sending the details.
#[derive(Debug, Clone)]
pub enum SessionSpec {
    Local,
    Ssh(SshSpec),
    Sftp(SshSpec),
    Telnet(TelnetSpec),
    Serial(SerialSpec),
}
