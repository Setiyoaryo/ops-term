//! Interactive SFTP provider (FR-17).
//!
//! Runs the system `sftp` client under a PTY using an audited argv, so remote
//! file browsing reuses the user's existing host trust, identity files and
//! `ProxyJump` instead of a second SSH implementation. Upload/download of whole
//! files is available through the separate `scp`-backed transfer manager.

use portable_pty::CommandBuilder;

use super::{ConnectionProvider, PtySession};
use crate::domain::{SshSpec, TerminalDimensions};
use crate::error::AppResult;
use crate::security::build_sftp_argv;

/// Spawn an interactive `sftp` client connected to `spec` under a PTY.
pub fn spawn_sftp(
    spec: &SshSpec,
    dims: TerminalDimensions,
) -> AppResult<Box<dyn ConnectionProvider>> {
    let argv = build_sftp_argv(spec)?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }
    cmd.env(
        "LANG",
        std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into()),
    );
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }
    Ok(Box::new(PtySession::spawn(cmd, dims)?))
}
