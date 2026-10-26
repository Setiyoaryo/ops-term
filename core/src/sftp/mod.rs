//! SFTP browsing/management via the system `sftp -b -` client (FR-17).
//!
//! Commands are fed on stdin in batch mode and reparse the audited argv from
//! `security::build_sftp_batch_argv`, so remote file management reuses the
//! user's existing host trust, identity and `~/.ssh/config`. The directory
//! listing is parsed from `ls -l` output; the parser tolerates the common
//! OpenSSH format (including file names with spaces).

use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use crate::domain::SshSpec;
use crate::error::{AppError, AppResult};
use crate::security;

mod process;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    pub permissions: String,
    pub modified: String,
}

struct AskpassCleanup(Option<std::path::PathBuf>);

impl Drop for AskpassCleanup {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn run_batch(spec: &SshSpec, commands: &[String]) -> AppResult<String> {
    let argv = security::build_sftp_batch_argv(spec)?;
    let input = format!("{}\nbye\n", commands.join("\n"));
    if input.len() > 64 * 1024 {
        return Err(AppError::Validation(
            "SFTP batch commands are too long".into(),
        ));
    }
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    let _askpass = AskpassCleanup(crate::askpass::apply_askpass(
        &mut command,
        spec.password.as_deref(),
    ));
    let output = process::run(command, input.as_bytes(), Duration::from_secs(30))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().last().unwrap_or("sftp command failed");
        return Err(AppError::Validation(detail.trim().to_string()));
    }
    Ok(stdout)
}

/// Parse one `ls -l` line into an entry.
fn parse_ls_line(line: &str, dir: &str) -> Option<SftpEntry> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() || trimmed.starts_with("total ") {
        return None;
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 9 {
        return None;
    }
    let permissions = parts[0].to_string();
    if permissions.len() < 10 {
        return None;
    }
    let is_dir = permissions.starts_with('d');
    let is_link = permissions.starts_with('l');
    let size = parts[4].parse::<u64>().unwrap_or(0);
    let modified = format!("{} {} {}", parts[5], parts[6], parts[7]);
    // Name is everything after the 8th field, so spaces are preserved.
    let mut name = parts[8..].join(" ");
    if is_link {
        if let Some((target, _)) = name.split_once(" -> ") {
            name = target.to_string();
        }
    }
    if name == "." || name == ".." {
        return None;
    }
    // Some SFTP servers return absolute names; never prefix those (avoids
    // "/home" + "/home/x" -> "//home//home/x").
    let path = if name.starts_with('/') {
        name.clone()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    };
    Some(SftpEntry {
        name,
        path,
        is_dir,
        is_link,
        size,
        permissions,
        modified,
    })
}

pub fn list(spec: &SshSpec, path: &str) -> AppResult<Vec<SftpEntry>> {
    if path.chars().any(|c| c.is_control()) {
        return Err(AppError::Validation("invalid path".into()));
    }
    let quoted = quote(path);
    let out = run_batch(spec, &[format!("ls -l {quoted}")])?;
    let mut entries: Vec<SftpEntry> = out.lines().filter_map(|l| parse_ls_line(l, path)).collect();
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

pub fn mkdir(spec: &SshSpec, path: &str) -> AppResult<()> {
    require_path(path)?;
    run_batch(spec, &[format!("mkdir {}", quote(path))])?;
    Ok(())
}

pub fn remove(spec: &SshSpec, path: &str, is_dir: bool) -> AppResult<()> {
    require_path(path)?;
    let cmd = if is_dir { "rmdir" } else { "rm" };
    run_batch(spec, &[format!("{cmd} {}", quote(path))])?;
    Ok(())
}

pub fn rename(spec: &SshSpec, from: &str, to: &str) -> AppResult<()> {
    require_path(from)?;
    require_path(to)?;
    run_batch(spec, &[format!("rename {} {}", quote(from), quote(to))])?;
    Ok(())
}

fn require_path(path: &str) -> AppResult<()> {
    if path.is_empty() || path.chars().any(|c| c.is_control()) {
        return Err(AppError::Validation("invalid path".into()));
    }
    Ok(())
}

/// Quote an sftp command operand (spaces allowed, quotes escaped).
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_ls_lines() {
        let dir = "/tmp";
        let e = parse_ls_line(
            "drwxr-xr-x    2 root     root         4096 Jan  1 12:00 my dir",
            dir,
        )
        .unwrap();
        assert_eq!(e.name, "my dir");
        assert!(e.is_dir);
        assert_eq!(e.path, "/tmp/my dir");

        let f = parse_ls_line(
            "-rw-r--r--    1 root     root         1234 Feb 10 09:30 config.cfg",
            dir,
        )
        .unwrap();
        assert!(!f.is_dir);
        assert_eq!(f.size, 1234);

        let l = parse_ls_line(
            "lrwxrwxrwx    1 root     root           11 Jan  1 00:00 link -> /etc/hosts",
            dir,
        )
        .unwrap();
        assert!(l.is_link);
        assert_eq!(l.name, "link");
    }

    #[test]
    fn ignores_total_and_junk() {
        assert!(parse_ls_line("total 8", "/x").is_none());
        assert!(parse_ls_line("", "/x").is_none());
        assert!(parse_ls_line(".", "/x").is_none());
    }

    #[test]
    fn quotes_operands() {
        assert_eq!(quote("/a b/c"), "\"/a b/c\"");
    }
}
