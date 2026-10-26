//! File transfer jobs backed by the system `scp` client (FR-17).
//!
//! Each job is an audited `scp` argv (never a shell string) that reuses the
//! user's SSH trust and identity. Jobs are user-triggered and cancellable; the
//! manager reports completion/failure, not a transactional guarantee.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::domain::SshSpec;
use crate::error::{AppError, AppResult};
use crate::security;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub spec: SshSpec,
    /// `upload` or `download`.
    pub direction: String,
    pub local_path: String,
    pub remote_path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferView {
    pub id: String,
    pub direction: String,
    pub local_path: String,
    pub remote_path: String,
    /// `running`, `done` or `failed`.
    pub status: String,
    pub message: Option<String>,
}

struct Job {
    view: TransferView,
    child: Child,
    stderr: Arc<Mutex<String>>,
}

pub struct TransferManager {
    jobs: Mutex<HashMap<String, Job>>,
}

impl TransferManager {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, req: TransferRequest) -> AppResult<String> {
        let dest = security::ssh_destination(&req.spec)?;
        let (source, target) = match req.direction.as_str() {
            "upload" => (
                req.local_path.clone(),
                format!("{dest}:{}", req.remote_path),
            ),
            "download" => (
                format!("{dest}:{}", req.remote_path),
                req.local_path.clone(),
            ),
            other => {
                return Err(AppError::Validation(format!(
                    "invalid transfer direction: {other}"
                )))
            }
        };
        let argv = security::build_scp_argv(&req.spec, req.recursive, &[source, target])?;

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
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut guard = sink.lock();
                    guard.push_str(&String::from_utf8_lossy(&buf[..n]));
                    crate::text::keep_tail(&mut guard, 4096);
                }
            });
        }

        let id = uuid::Uuid::new_v4().to_string();
        self.jobs.lock().insert(
            id.clone(),
            Job {
                view: TransferView {
                    id: id.clone(),
                    direction: req.direction,
                    local_path: req.local_path,
                    remote_path: req.remote_path,
                    status: "running".into(),
                    message: None,
                },
                child,
                stderr,
            },
        );
        Ok(id)
    }

    pub fn list(&self) -> Vec<TransferView> {
        let mut guard = self.jobs.lock();
        for job in guard.values_mut() {
            if job.view.status == "running" {
                match job.child.try_wait() {
                    Ok(Some(status)) => {
                        if status.success() {
                            job.view.status = "done".into();
                        } else {
                            job.view.status = "failed".into();
                            let msg = job.stderr.lock().clone();
                            job.view.message = Some(msg.trim().to_string());
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        job.view.status = "failed".into();
                        job.view.message = Some(e.to_string());
                    }
                }
            }
        }
        guard.values().map(|j| j.view.clone()).collect()
    }

    pub fn cancel(&self, id: &str) -> AppResult<()> {
        let mut guard = self.jobs.lock();
        if let Some(job) = guard.get_mut(id) {
            let _ = job.child.kill();
            let _ = job.child.wait();
            job.view.status = "failed".into();
            job.view.message = Some("cancelled".into());
        }
        Ok(())
    }
}

impl Default for TransferManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SshSpec {
        SshSpec {
            host: "files.example.net".into(),
            port: 2222,
            username: Some("ops".into()),
            identity_file: None,
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
        }
    }

    #[test]
    fn scp_argv_uses_uppercase_port() {
        let argv = security::build_scp_argv(&spec(), false, &["a".into(), "b".into()]).unwrap();
        assert!(argv.contains(&"-P".to_string()));
        assert!(argv.contains(&"2222".to_string()));
        assert!(argv[0].ends_with("scp"));
    }

    #[test]
    fn sftp_destination_includes_user() {
        assert_eq!(
            security::ssh_destination(&spec()).unwrap(),
            "ops@files.example.net"
        );
    }
}
