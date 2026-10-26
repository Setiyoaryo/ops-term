//! Connection setup runs off the UI thread; ownership cleans up abandoned results.

use crate::background::Task;
use opsterm_core::domain::{SavedSession, SerialSpec, SshSpec, TelnetSpec, TerminalDimensions};
use opsterm_core::providers::{self, ConnectionProvider};
use std::sync::Arc;

/// A tab exists before DNS, keyring, authentication or a worker slot is ready.
pub struct OpeningProvider;

impl ConnectionProvider for OpeningProvider {
    fn write(&self, _data: &[u8]) -> opsterm_core::error::AppResult<()> {
        Err(opsterm_core::error::AppError::Validation(
            "connection is not ready".into(),
        ))
    }
    fn resize(&self, _dims: TerminalDimensions) -> opsterm_core::error::AppResult<()> {
        Ok(())
    }
    fn kill(&self) -> opsterm_core::error::AppResult<()> {
        Ok(())
    }
    fn take_reader(&self) -> opsterm_core::error::AppResult<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(std::io::empty()))
    }
    fn is_alive(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub enum Origin {
    Local,
    Ssh(SshSpec),
    Telnet(TelnetSpec),
    Serial(SerialSpec),
}

pub struct Request {
    pub title: String,
    pub origin: Origin,
    pub saved: Option<SavedSession>,
    pub secret: Option<String>,
    pub target: Option<u64>,
    pub dims: TerminalDimensions,
}

#[derive(Debug)]
pub enum SetupError {
    Credential(String),
    Connection(String),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Credential(message) | Self::Connection(message) => f.write_str(message),
        }
    }
}

impl From<String> for SetupError {
    fn from(message: String) -> Self {
        Self::Connection(message)
    }
}

impl From<&str> for SetupError {
    fn from(message: &str) -> Self {
        Self::Connection(message.into())
    }
}

pub const MAX_WORKERS: usize = 4;

pub struct Queued {
    pub request: Request,
    pub previous: Option<Arc<dyn ConnectionProvider>>,
}

/// A result can be abandoned when the window or reconnect target is closed.
/// Providers do not all kill their subprocess/socket on ordinary Drop.
pub struct OpenedProvider(pub Option<Box<dyn ConnectionProvider>>);

impl Drop for OpenedProvider {
    fn drop(&mut self) {
        if let Some(provider) = self.0.take() {
            let _ = provider.kill();
        }
    }
}

pub struct Pending {
    pub request: Request,
    pub task: Task<Result<OpenedProvider, SetupError>>,
}

impl Pending {
    pub fn start(
        request: Request,
        previous: Option<Arc<dyn ConnectionProvider>>,
    ) -> Result<Self, String> {
        let mut origin = request.origin.clone();
        let dims = request.dims;
        let provided_secret = request.secret.clone();
        let credential_ref = match &origin {
            Origin::Ssh(_) | Origin::Telnet(_) => request
                .saved
                .as_ref()
                .and_then(|s| s.credential_ref.clone()),
            Origin::Local | Origin::Serial(_) => None,
        };
        let task = Task::spawn(move || {
            if let Some(provider) = previous {
                let _ = provider.kill();
            }
            // A locked vault/keyring may wait for the OS, so lookup belongs here too.
            let secret = if let Some(secret) = provided_secret {
                Some(secret)
            } else if let Some(reference) = credential_ref.as_deref().filter(|r| !r.is_empty()) {
                Some(opsterm_core::credential::resolve(reference).map_err(|_| {
                    SetupError::Credential(
                        "The saved credential could not be read from secure storage. Unlock the credential vault or restore the OS keyring, then retry.".into(),
                    )
                })?)
            } else {
                None
            };
            if let Some(secret) = secret {
                match &mut origin {
                    Origin::Ssh(spec) => spec.password = Some(secret),
                    Origin::Telnet(spec) => spec.password = Some(secret),
                    _ => {}
                }
            }
            let provider = match &origin {
                Origin::Local => providers::local_pty::spawn_local_shell(dims),
                Origin::Ssh(spec) if spec.engine.as_deref() == Some("native") => {
                    providers::russh_ssh::spawn_russh(spec, dims)
                }
                Origin::Ssh(spec) => providers::openssh_pty::spawn_openssh(spec, dims),
                Origin::Telnet(spec) => providers::telnet::spawn_telnet(spec, dims),
                Origin::Serial(spec) => providers::serial::spawn_serial(spec, dims),
            };
            provider
                .map(|p| OpenedProvider(Some(p)))
                .map_err(|e| SetupError::Connection(e.to_string()))
        })?;
        Ok(Self { request, task })
    }
}
