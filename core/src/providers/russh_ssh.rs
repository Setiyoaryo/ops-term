//! Native SSH engine (`russh`) — M4 advanced option (FR-01 / PRD section 7.2).
//!
//! This is an *alternative* provider to the default OpenSSH subprocess. It does
//! not replace OpenSSH: the user must opt in per session. Host keys are verified
//! against `~/.ssh/known_hosts` and unknown or changed keys are always rejected
//! (no auto-accept). Authentication supports public keys (identity file or the
//! default `~/.ssh` identities).
//!
//! Deliberate limitations, surfaced as clear errors rather than silent
//! fallbacks: no `ProxyJump`, no password/keyboard-interactive auth, and a host
//! must already be present in `known_hosts`.

use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::{check_known_hosts_path, load_secret_key, PrivateKeyWithHashAlg};
use russh::{Channel, ChannelMsg, Disconnect};

use super::ConnectionProvider;
use crate::domain::{SshSpec, TerminalDimensions};
use crate::error::{AppError, AppResult};

async fn bounded_setup<T>(
    timeout: Duration,
    ready: &mpsc::Sender<Result<(), String>>,
    setup: impl std::future::Future<Output = AppResult<T>>,
) -> AppResult<T> {
    match tokio::time::timeout(timeout, setup).await {
        Ok(result) => result,
        Err(_) => {
            let message = "native SSH setup timed out".to_string();
            let _ = ready.send(Err(message.clone()));
            Err(AppError::Validation(message))
        }
    }
}

/// A `Read` over data pushed from the async SSH task.
pub(crate) struct ChannelReader {
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

enum Control {
    Resize(u32, u32),
    Close,
}

/// Client handler that performs strict known_hosts verification.
struct HostKeyVerifier {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    reason: Arc<Mutex<Option<String>>>,
}

impl client::Handler for HostKeyVerifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match check_known_hosts_path(&self.host, self.port, server_public_key, &self.known_hosts) {
            Ok(true) => Ok(true),
            Ok(false) => {
                *self.reason.lock() = Some(format!(
                    "host key for {}:{} is not in {} — connect once with the OpenSSH engine to trust it",
                    self.host,
                    self.port,
                    self.known_hosts.display()
                ));
                Ok(false)
            }
            Err(e) => {
                *self.reason.lock() = Some(format!("host key verification failed: {e}"));
                Ok(false)
            }
        }
    }
}

pub struct RusshSession {
    write_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    control_tx: tokio::sync::mpsc::UnboundedSender<Control>,
    reader: Mutex<Option<Box<dyn Read + Send>>>,
    alive: Arc<AtomicBool>,
}

fn known_hosts_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ssh").join("known_hosts")
}

/// Resolve the identity key to use: explicit path, else the first existing
/// default identity.
fn resolve_identity(spec: &SshSpec) -> AppResult<PathBuf> {
    if let Some(path) = &spec.identity_file {
        let validated = crate::security::validate_identity_file(path)?;
        return Ok(PathBuf::from(validated));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
        let candidate = home.join(".ssh").join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(AppError::Validation(
        "native SSH engine needs an identity file (or ~/.ssh/id_ed25519)".into(),
    ))
}

pub fn spawn_russh(
    spec: &SshSpec,
    dims: TerminalDimensions,
) -> AppResult<Box<dyn ConnectionProvider>> {
    dims.validate()?;
    let host = crate::security::validate_host(&spec.host)?;
    crate::security::validate_port(spec.port)?;
    if spec
        .proxy_jump
        .as_deref()
        .map(|j| !j.is_empty())
        .unwrap_or(false)
    {
        return Err(AppError::Validation(
            "the native SSH engine does not support ProxyJump yet; use the OpenSSH engine".into(),
        ));
    }
    let username = spec
        .username
        .as_ref()
        .map(|u| crate::security::validate_username(u))
        .transpose()?
        .or_else(|| std::env::var("USER").ok())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| AppError::Validation("a username is required for native SSH".into()))?;
    let identity = resolve_identity(spec)?;
    let term = spec.term.clone().unwrap_or_else(|| "xterm-256color".into());
    let timeout = Duration::from_secs(spec.connect_timeout.unwrap_or(20).max(1) as u64);

    let password = spec.password.clone();

    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let (data_tx, data_rx) = mpsc::sync_channel::<Vec<u8>>(32);
    let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel::<Control>();
    let alive = Arc::new(AtomicBool::new(true));

    let thread_alive = alive.clone();
    let host_for_thread = host.clone();
    let port = spec.port;
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = ready_tx.send(Err(format!("runtime: {e}")));
                return;
            }
        };
        runtime.block_on(async move {
            let outcome = run_session(
                host_for_thread,
                port,
                username,
                identity,
                term,
                dims,
                timeout,
                password,
                ready_tx,
                data_tx,
                write_rx,
                control_rx,
            )
            .await;
            if let Err(e) = outcome {
                // Only reached before the ready signal or during serve.
                let _ = e;
            }
            thread_alive.store(false, Ordering::SeqCst);
        });
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Box::new(RusshSession {
            write_tx,
            control_tx,
            reader: Mutex::new(Some(Box::new(ChannelReader::new(data_rx)))),
            alive,
        })),
        Ok(Err(message)) => Err(AppError::Validation(message)),
        Err(_) => Err(AppError::Internal("native SSH task stopped early".into())),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    host: String,
    port: u16,
    username: String,
    identity: PathBuf,
    term: String,
    dims: TerminalDimensions,
    timeout: Duration,
    password: Option<String>,
    ready_tx: mpsc::Sender<Result<(), String>>,
    data_tx: mpsc::SyncSender<Vec<u8>>,
    mut write_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    mut control_rx: tokio::sync::mpsc::UnboundedReceiver<Control>,
) -> AppResult<()> {
    // Bound connection, agent/key/password authentication and shell setup as
    // one operation. Once ready, the live SSH session has no lifetime limit.
    let (session, mut channel) = bounded_setup(timeout, &ready_tx, async {
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            ..<_>::default()
        });
        let reason = Arc::new(Mutex::new(None));
        let handler = HostKeyVerifier {
            host: host.clone(),
            port,
            known_hosts: known_hosts_path(),
            reason: reason.clone(),
        };

        let connect = tokio::time::timeout(
            timeout,
            client::connect(config, (host.as_str(), port), handler),
        );
        let mut session: Handle<HostKeyVerifier> = match connect.await {
            Err(_) => {
                let msg = "native SSH connect timed out".to_string();
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(AppError::Internal(msg));
            }
            Ok(Err(e)) => {
                let msg = reason
                    .lock()
                    .clone()
                    .unwrap_or_else(|| format!("native SSH connect failed: {e}"));
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(AppError::Validation(msg));
            }
            Ok(Ok(handle)) => handle,
        };

        // Authenticate: try the agent first, then the identity key.
        let mut authenticated = false;
        if let Ok(mut agent) = AgentClient::connect_env().await {
            let identities = agent.request_identities().await.unwrap_or_default();
            for key in identities {
                if let Ok(russh::client::AuthResult::Success) = session
                    .authenticate_publickey_with(username.clone(), key, None, &mut agent)
                    .await
                {
                    authenticated = true;
                    break;
                }
            }
        }
        if !authenticated {
            match load_secret_key(&identity, None) {
                Ok(key) => {
                    let hash = session
                        .best_supported_rsa_hash()
                        .await
                        .ok()
                        .flatten()
                        .flatten();
                    let res = session
                        .authenticate_publickey(
                            username.clone(),
                            PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                        )
                        .await;
                    authenticated = matches!(res, Ok(russh::client::AuthResult::Success));
                }
                Err(e) => {
                    let msg = format!("could not load identity {}: {e}", identity.display());
                    let _ = ready_tx.send(Err(msg.clone()));
                    return Err(AppError::Validation(msg));
                }
            }
        }
        if !authenticated {
            if let Some(secret) = password.as_deref().filter(|p| !p.is_empty()) {
                let res = session
                    .authenticate_password(username.clone(), secret.to_string())
                    .await;
                authenticated = matches!(res, Ok(russh::client::AuthResult::Success));
            }
        }
        if !authenticated {
            let msg = "native SSH authentication failed (agent/key/password)".to_string();
            let _ = ready_tx.send(Err(msg.clone()));
            return Err(AppError::Validation(msg));
        }

        let channel: Channel<client::Msg> = match session.channel_open_session().await {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("channel open failed: {e}");
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(AppError::Internal(msg));
            }
        };
        let _ = channel
            .request_pty(true, &term, dims.cols as u32, dims.rows as u32, 0, 0, &[])
            .await;
        let _ = channel.request_shell(true).await;

        Ok((session, channel))
    })
    .await?;

    let _ = ready_tx.send(Ok(()));

    loop {
        tokio::select! {
            Some(ctrl) = control_rx.recv() => match ctrl {
                Control::Resize(cols, rows) => {
                    let _ = channel.window_change(cols, rows, 0, 0).await;
                }
                Control::Close => {
                    let _ = channel.eof().await;
                    let _ = channel.close().await;
                    break;
                }
            },
            Some(data) = write_rx.recv() => {
                if channel.data(&data[..]).await.is_err() {
                    break;
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        if data_tx.send(data.to_vec()).is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                        let _ = data_tx.send(data.to_vec());
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => break,
                    None => break,
                    _ => {}
                }
            }
        }
    }

    let _ = session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
    Ok(())
}

impl ConnectionProvider for RusshSession {
    fn write(&self, data: &[u8]) -> AppResult<()> {
        self.write_tx
            .send(data.to_vec())
            .map_err(|_| AppError::Validation("session is not connected".into()))
    }

    fn resize(&self, dims: TerminalDimensions) -> AppResult<()> {
        dims.validate()?;
        let _ = self
            .control_tx
            .send(Control::Resize(dims.cols as u32, dims.rows as u32));
        Ok(())
    }

    fn kill(&self) -> AppResult<()> {
        self.alive.store(false, Ordering::SeqCst);
        let _ = self.control_tx.send(Control::Close);
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

    #[test]
    fn setup_deadline_also_covers_authentication_after_connect() {
        let (tx, rx) = mpsc::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(bounded_setup(Duration::from_millis(10), &tx, async {
            // Connecting has completed, but authentication does not respond in time.
            tokio::time::sleep(Duration::from_millis(40)).await;
            Ok(())
        }));
        assert!(
            result.is_err(),
            "authentication must not hold a connection worker indefinitely"
        );
        assert!(rx.try_recv().unwrap().unwrap_err().contains("timed out"));
    }

    #[test]
    fn successful_setup_returns_ownership_without_a_session_lifetime_limit() {
        let (tx, _rx) = mpsc::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let value = runtime
            .block_on(bounded_setup(Duration::from_secs(1), &tx, async { Ok(42) }))
            .unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn unknown_host_is_rejected() {
        // A known_hosts file that does not contain the sample key.
        let dir = std::env::temp_dir().join(format!("opsterm-kh-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("known_hosts");
        std::fs::write(&path, "# empty\n").unwrap();

        let key = russh::keys::ssh_key::PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILl6Q0cuDe6lbK9Fqfowz1o0HdS1n9a9C3T3m0N0oPqX test@host",
        );
        if let Ok(key) = key {
            let result = check_known_hosts_path("example.net", 22, &key, &path);
            assert!(!matches!(result, Ok(true)), "unknown key must not verify");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identity_resolution_prefers_explicit() {
        let spec = SshSpec {
            host: "h".into(),
            port: 22,
            username: Some("u".into()),
            identity_file: Some("/etc/ssh/ssh_host_ed25519_key".into()),
            proxy_jump: None,
            name: None,
            term: None,
            connect_timeout: None,
            server_alive_interval: None,
            x11_forwarding: false,
            forward_agent: false,
            compression: false,
            certificate_file: None,
            pkcs11_provider: None,
            engine: None,
            password: None,
        };
        // Explicit absolute path is returned as-is (validation only checks it is
        // absolute and control-character free).
        assert_eq!(
            resolve_identity(&spec).unwrap(),
            PathBuf::from("/etc/ssh/ssh_host_ed25519_key")
        );
    }
}
