//! Shared SSH_ASKPASS helper used to feed inventory passwords to OpenSSH
//! clients (ssh, sftp, scp) without ever putting a secret on the command line.
//!
//! The password is written to a `0600` file inside a `0700` directory and an
//! `askpass.sh` that prints it is pointed at by `SSH_ASKPASS` with
//! `SSH_ASKPASS_REQUIRE=force`. Callers remove the directory when done.

use std::path::PathBuf;

use crate::error::AppResult;

/// Create a private askpass helper directory and return its path.
pub fn create_askpass(password: &str) -> AppResult<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let dir = std::env::temp_dir().join(format!("opsterm-askpass-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;

    let pass_path = dir.join("pass");
    let mut pass = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&pass_path)?;
    pass.write_all(password.as_bytes())?;
    pass.flush()?;

    let script_path = dir.join("askpass.sh");
    let mut script = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o700)
        .open(&script_path)?;
    script.write_all(format!("#!/bin/sh\nexec cat '{}'\n", pass_path.display()).as_bytes())?;
    script.flush()?;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

/// Apply the askpass environment to a command when a password is present.
pub fn apply_askpass(
    command: &mut std::process::Command,
    password: Option<&str>,
) -> Option<PathBuf> {
    let password = password.filter(|p| !p.is_empty())?;
    let dir = create_askpass(password).ok()?;
    command.env("SSH_ASKPASS", dir.join("askpass.sh"));
    command.env("SSH_ASKPASS_REQUIRE", "force");
    command.env(
        "DISPLAY",
        std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
    );
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn askpass_files_are_private_and_correct() {
        let dir = create_askpass("hunter2").expect("askpass");
        let pass = dir.join("pass");
        let script = dir.join("askpass.sh");
        assert_eq!(
            std::fs::metadata(&pass).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&script).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(std::fs::read_to_string(&pass).unwrap(), "hunter2");
        assert!(std::fs::read_to_string(&script).unwrap().contains("cat"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
