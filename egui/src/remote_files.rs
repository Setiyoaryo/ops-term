//! Capture the device and directory for each asynchronous SFTP operation.

use crate::background::Task;
use opsterm_core::{
    domain::SshSpec,
    sftp::{self, SftpEntry},
};

pub enum Action {
    List,
    Mkdir(String),
    Remove(String, bool),
}

pub struct Request {
    pub device_id: String,
    pub path: String,
    pub action: Action,
}

pub struct Pending {
    pub device_id: String,
    pub path: String,
    pub task: Task<Result<Vec<SftpEntry>, String>>,
}

impl Pending {
    pub fn start(
        mut spec: SshSpec,
        request: Request,
        credential_ref: Option<String>,
        secret: Option<String>,
    ) -> Result<Self, String> {
        let path = request.path.clone();
        let task = Task::spawn(move || {
            let secret = secret.or_else(|| {
                credential_ref
                    .as_deref()
                    .and_then(|r| opsterm_core::credential::resolve(r).ok())
            });
            if let Some(secret) = secret {
                spec.password = Some(secret);
            }
            let result = match request.action {
                Action::List => Ok(()),
                Action::Mkdir(target) => sftp::mkdir(&spec, &target),
                Action::Remove(target, is_dir) => sftp::remove(&spec, &target, is_dir),
            };
            result
                .and_then(|()| sftp::list(&spec, &path))
                .map_err(|e| e.to_string())
        })?;
        Ok(Self {
            device_id: request.device_id,
            path: request.path,
            task,
        })
    }
}
