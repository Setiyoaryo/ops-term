//! Local shell provider: the user's `$SHELL` (or a `/bin` fallback) running
//! under a real PTY, so `vim`, `top`, `htop` and other full-screen TUIs behave
//! correctly (PRD FR-02).

use portable_pty::CommandBuilder;

use super::{ConnectionProvider, PtySession};
use crate::domain::TerminalDimensions;
use crate::error::AppResult;
use crate::security::resolve_local_shell;

/// Spawn an interactive local shell in a PTY sized to `dims`.
pub fn spawn_local_shell(dims: TerminalDimensions) -> AppResult<Box<dyn ConnectionProvider>> {
    let shell = resolve_local_shell()?;
    let mut cmd = CommandBuilder::new(&shell);
    // Login shell so the user's profile/rc files apply, matching a normal
    // terminal emulator.
    cmd.arg("-l");
    cmd.env("TERM", "xterm-256color");
    // Force a sane prompt/encoding for interactive use.
    cmd.env(
        "LANG",
        std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into()),
    );
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }

    Ok(Box::new(PtySession::spawn(cmd, dims)?))
}
