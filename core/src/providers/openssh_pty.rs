//! OpenSSH-backed SSH provider.
//!
//! M1 runs the system OpenSSH client inside a PTY using a structured argv
//! array built by [`crate::security::build_ssh_argv`]. This deliberately
//! inherits the user's existing `~/.ssh/config`, `known_hosts`, agent keys and
//! `ProxyJump` behaviour, and never disables host-key verification
//! (PRD FR-01 / section 7.2).

use std::path::PathBuf;

use portable_pty::CommandBuilder;

use super::{ConnectionProvider, PtySession};
use crate::domain::{SshSpec, TerminalDimensions};
use crate::error::AppResult;
use crate::security::build_ssh_argv;

/// Spawn an OpenSSH client connected to `spec` under a PTY sized to `dims`.
pub fn spawn_openssh(
    spec: &SshSpec,
    dims: TerminalDimensions,
) -> AppResult<Box<dyn ConnectionProvider>> {
    let argv = build_ssh_argv(spec)?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }

    cmd.env("TERM", sanitize_term(spec.term.as_deref()));
    cmd.env(
        "LANG",
        std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into()),
    );
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }

    // Optional automatic login password from the credential inventory. The
    // password is written to a 0600 file inside a 0700 temp dir and supplied to
    // OpenSSH through the standard `SSH_ASKPASS` mechanism — never on the
    // command line or in an environment value.
    let mut askpass_dir: Option<PathBuf> = None;
    if let Some(password) = spec.password.as_deref().filter(|p| !p.is_empty()) {
        let dir = crate::askpass::create_askpass(password)?;
        cmd.env(
            "SSH_ASKPASS",
            dir.join("askpass.sh").to_string_lossy().to_string(),
        );
        cmd.env("SSH_ASKPASS_REQUIRE", "force");
        cmd.env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
        );
        askpass_dir = Some(dir);
    }

    let session = PtySession::spawn(cmd, dims)?;
    if let Some(dir) = askpass_dir {
        // Safety net: if the session is never explicitly closed, drop the helper
        // after two minutes (authentication is long over by then).
        let cleanup = dir.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(120));
            let _ = std::fs::remove_dir_all(cleanup);
        });
        session.set_cleanup(dir);
    }
    Ok(Box::new(session))
}

/// Constrain the requested `TERM` to a safe character set. Anything unexpected
/// falls back to `xterm-256color`.
fn sanitize_term(term: Option<&str>) -> String {
    match term {
        Some(t)
            if !t.is_empty()
                && t.len() <= 64
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')) =>
        {
            t.to_string()
        }
        _ => "xterm-256color".to_string(),
    }
}
