//! SSH tunnel management (FR-18).
//!
//! Port forwards are implemented by spawning the system OpenSSH client with
//! `-N` and an audited `-L`/`-R`/`-D` argument, never a shell string. This
//! reuses the user's existing trust, agent and `known_hosts` policy. Bind
//! addresses are validated and local forwards default to `127.0.0.1`.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::security;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelSpec {
    /// `local` (`-L`), `remote` (`-R`) or `dynamic` (`-D`).
    pub kind: String,
    #[serde(default = "default_bind")]
    pub bind_address: String,
    pub bind_port: u16,
    #[serde(default)]
    pub dest_host: Option<String>,
    #[serde(default)]
    pub dest_port: Option<u16>,
    pub ssh_host: String,
    #[serde(default = "crate::domain::default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub identity_file: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

fn default_bind() -> String {
    "127.0.0.1".into()
}

/// A tunnel as seen by the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub id: String,
    pub kind: String,
    pub bind: String,
    pub dest: Option<String>,
    pub ssh_host: String,
    pub label: Option<String>,
    pub running: bool,
    pub error: Option<String>,
}

struct Instance {
    spec: TunnelSpec,
    child: Child,
    stderr: Arc<Mutex<String>>,
}

pub struct TunnelManager {
    tunnels: Mutex<HashMap<String, Instance>>,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            tunnels: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, spec: TunnelSpec) -> AppResult<String> {
        let argv = build_tunnel_argv(&spec)?;
        let mut child = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(AppError::Io)?;

        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(mut err) = child.stderr.take() {
            let sink = stderr.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 1024];
                loop {
                    match err.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut guard = sink.lock();
                            guard.push_str(&String::from_utf8_lossy(&buf[..n]));
                            // Keep only the tail so the message stays bounded.
                            crate::text::keep_tail(&mut guard, 4096);
                        }
                    }
                }
            });
        }

        let id = uuid::Uuid::new_v4().to_string();
        self.tunnels.lock().insert(
            id.clone(),
            Instance {
                spec,
                child,
                stderr,
            },
        );
        Ok(id)
    }

    pub fn list(&self) -> Vec<TunnelView> {
        let mut guard = self.tunnels.lock();
        let mut views = Vec::new();
        for (id, inst) in guard.iter_mut() {
            let running = matches!(inst.child.try_wait(), Ok(None));
            let err = inst.stderr.lock().clone();
            views.push(TunnelView {
                id: id.clone(),
                kind: inst.spec.kind.clone(),
                bind: format!("{}:{}", inst.spec.bind_address, inst.spec.bind_port),
                dest: inst
                    .spec
                    .dest_host
                    .as_ref()
                    .map(|h| format!("{h}:{}", inst.spec.dest_port.unwrap_or(0))),
                ssh_host: inst.spec.ssh_host.clone(),
                label: inst.spec.label.clone(),
                running,
                error: if err.trim().is_empty() {
                    None
                } else {
                    Some(err.trim().to_string())
                },
            });
        }
        views.sort_by(|a, b| a.bind.cmp(&b.bind));
        views
    }

    pub fn stop(&self, id: &str) -> AppResult<()> {
        let mut guard = self.tunnels.lock();
        if let Some(inst) = guard.remove(id) {
            let mut child = inst.child;
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the audited argv for a tunnel. `bind_address` and every host are
/// validated so a crafted value cannot become an OpenSSH option.
pub fn build_tunnel_argv(spec: &TunnelSpec) -> AppResult<Vec<String>> {
    let ssh = security::resolve_ssh_binary()?;
    let ssh_host = security::validate_host(&spec.ssh_host)?;
    security::validate_port(spec.ssh_port)?;
    security::validate_port(spec.bind_port)?;

    let bind_address = if spec.bind_address.trim().is_empty() {
        "127.0.0.1".to_string()
    } else {
        security::validate_host(&spec.bind_address)?
    };

    let forward = match spec.kind.as_str() {
        "local" | "remote" => {
            let dest_host = spec
                .dest_host
                .as_ref()
                .ok_or_else(|| AppError::Validation("destination host is required".into()))?;
            let dest_host = security::validate_host(dest_host)?;
            let dest_port = spec
                .dest_port
                .ok_or_else(|| AppError::Validation("destination port is required".into()))?;
            security::validate_port(dest_port)?;
            format!(
                "{bind_address}:{bind_port}:{dest_host}:{dest_port}",
                bind_port = spec.bind_port
            )
        }
        "dynamic" => format!("{bind_address}:{}", spec.bind_port),
        other => {
            return Err(AppError::Validation(format!(
                "unknown tunnel kind: {other}"
            )))
        }
    };

    let mut args = vec![ssh, "-N".to_string()];
    match spec.kind.as_str() {
        "local" => {
            args.push("-L".to_string());
            args.push(forward);
        }
        "remote" => {
            args.push("-R".to_string());
            args.push(forward);
        }
        "dynamic" => {
            args.push("-D".to_string());
            args.push(forward);
        }
        _ => unreachable!(),
    }
    if let Some(user) = &spec.ssh_user {
        let user = security::validate_username(user)?;
        args.push("-l".to_string());
        args.push(user);
    }
    if spec.ssh_port != 22 {
        args.push("-p".to_string());
        args.push(spec.ssh_port.to_string());
    }
    if let Some(identity) = &spec.identity_file {
        args.push("-i".to_string());
        args.push(security::validate_identity_file(identity)?);
    }
    args.push(ssh_host);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: &str) -> TunnelSpec {
        TunnelSpec {
            kind: kind.into(),
            bind_address: "127.0.0.1".into(),
            bind_port: 8080,
            dest_host: Some("internal.example.net".into()),
            dest_port: Some(80),
            ssh_host: "bastion.example.net".into(),
            ssh_port: 22,
            ssh_user: Some("ops".into()),
            identity_file: None,
            label: None,
        }
    }

    #[test]
    fn builds_local_forward() {
        let argv = build_tunnel_argv(&spec("local")).unwrap();
        assert!(argv.contains(&"-N".to_string()));
        assert!(argv.contains(&"-L".to_string()));
        assert!(argv.contains(&"127.0.0.1:8080:internal.example.net:80".to_string()));
        assert_eq!(argv.last().unwrap(), "bastion.example.net");
    }

    #[test]
    fn dynamic_has_no_destination() {
        let mut s = spec("dynamic");
        s.dest_host = None;
        s.dest_port = None;
        let argv = build_tunnel_argv(&s).unwrap();
        assert!(argv.contains(&"-D".to_string()));
        assert!(argv.contains(&"127.0.0.1:8080".to_string()));
    }

    #[test]
    fn rejects_option_like_host() {
        let mut s = spec("local");
        s.ssh_host = "-oProxyCommand=evil".into();
        assert!(build_tunnel_argv(&s).is_err());
    }

    #[test]
    fn local_defaults_to_loopback_bind() {
        let mut s = spec("local");
        s.bind_address = "".into();
        let argv = build_tunnel_argv(&s).unwrap();
        assert!(argv
            .iter()
            .any(|a| a == "127.0.0.1:8080:internal.example.net:80"));
    }
}
