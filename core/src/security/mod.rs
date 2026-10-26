//! Input validation and safe process argument construction.
//!
//! This module is the single choke point between untrusted user/import input
//! and process spawning. Nothing here ever builds a shell command string: the
//! SSH client is launched with an individually validated argv array
//! (see PRD section 7.2 and the security acceptance cases in section 10).

use std::path::{Path, PathBuf};

use crate::domain::SshSpec;
use crate::error::{AppError, AppResult};

/// Absolute locations we are willing to execute for the SSH client, in
/// preference order. We never search `PATH` implicitly for ssh, because a
/// hostile `PATH` entry could redirect execution (PRD FR-01).
const SSH_CANDIDATES: &[&str] = &["/usr/bin/ssh", "/usr/local/bin/ssh", "/bin/ssh"];
const SFTP_CANDIDATES: &[&str] = &["/usr/bin/sftp", "/usr/local/bin/sftp", "/bin/sftp"];
const SCP_CANDIDATES: &[&str] = &["/usr/bin/scp", "/usr/local/bin/scp", "/bin/scp"];

/// Validate a hostname, IPv4, IPv6 literal, bracketed IPv6 literal, or
/// `~/.ssh/config` alias.
///
/// The critical invariant: a host may never begin with `-`, which is how it
/// could otherwise be reinterpreted as an OpenSSH option such as
/// `-oProxyCommand=...` (PRD security acceptance cases).
pub fn validate_host(host: &str) -> AppResult<String> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::Validation("host must not be empty".into()));
    }
    if host.len() > 255 {
        return Err(AppError::Validation("host is too long".into()));
    }
    if host.starts_with('-') {
        return Err(AppError::Validation("host must not begin with '-'".into()));
    }
    if host.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AppError::Validation(
            "host contains whitespace or control characters".into(),
        ));
    }
    // hostname / IPv4 / IPv6 characters, plus zone-id '%' and IPv6 brackets.
    let valid = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '%' | '[' | ']'));
    if !valid {
        return Err(AppError::Validation(
            "host contains unsupported characters".into(),
        ));
    }
    Ok(host.to_string())
}

/// Validate an SSH username. Kept conservative; usernames never begin with
/// `-` so they can be passed via `-l <user>` without being mistaken for flags.
pub fn validate_username(username: &str) -> AppResult<String> {
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::Validation("username must not be empty".into()));
    }
    if username.len() > 64 {
        return Err(AppError::Validation("username is too long".into()));
    }
    if username.starts_with('-') {
        return Err(AppError::Validation(
            "username must not begin with '-'".into(),
        ));
    }
    let valid = username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !valid {
        return Err(AppError::Validation(
            "username contains unsupported characters".into(),
        ));
    }
    Ok(username.to_string())
}

/// Validate a TCP port. `u16` already bounds the upper end; 0 is rejected.
pub fn validate_port(port: u16) -> AppResult<u16> {
    if port == 0 {
        return Err(AppError::Validation(
            "port must be between 1 and 65535".into(),
        ));
    }
    Ok(port)
}

/// Validate a filesystem path that will be handed to OpenSSH (identity file).
/// Leading `~` is expanded; the result must be absolute and free of control
/// characters. We never modify the target file or its permissions.
pub fn validate_identity_file(path: &str) -> AppResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::Validation("identity file path is empty".into()));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(AppError::Validation(
            "identity file path contains control characters".into(),
        ));
    }
    let expanded = expand_tilde(path);
    if !expanded.is_absolute() {
        return Err(AppError::Validation(
            "identity file path must be absolute".into(),
        ));
    }
    Ok(expanded.to_string_lossy().into_owned())
}

/// Validate an OpenSSH `ProxyJump` (`-J`) value.
///
/// Accepts one or more comma-separated hops of the form `[user@]host[:port]`.
/// Every component is validated with the same host/username/port rules so a
/// crafted jump string cannot smuggle in an option or shell fragment.
pub fn validate_proxy_jump(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Validation("proxy jump is empty".into()));
    }
    if value.len() > 512 {
        return Err(AppError::Validation("proxy jump is too long".into()));
    }
    for hop in value.split(',') {
        let hop = hop.trim();
        if hop.is_empty() {
            return Err(AppError::Validation("proxy jump has an empty hop".into()));
        }
        let (user_part, host_port) = match hop.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, hop),
        };
        if let Some(user) = user_part {
            validate_username(user)?;
        }
        // IPv6 hops must be bracketed `[::1]:22` to disambiguate the port.
        let (host, port) = if let Some(rest) = host_port.strip_prefix('[') {
            match rest.split_once(']') {
                Some((h, suffix)) => {
                    let port = suffix.strip_prefix(':').map(|s| s.to_string());
                    (format!("[{h}]"), port)
                }
                None => {
                    return Err(AppError::Validation(
                        "proxy jump has an unterminated IPv6 literal".into(),
                    ))
                }
            }
        } else {
            match host_port.matches(':').count() {
                // Exactly one colon means `host:port`; multiple colons mean an
                // unbracketed IPv6 literal with no port.
                1 => {
                    let (h, p) = host_port.split_once(':').expect("one colon");
                    (h.to_string(), Some(p.to_string()))
                }
                _ => (host_port.to_string(), None),
            }
        };
        validate_host(&host)?;
        if let Some(p) = port {
            let p: u16 = p
                .parse()
                .map_err(|_| AppError::Validation("proxy jump port is invalid".into()))?;
            validate_port(p)?;
        }
    }
    Ok(value.to_string())
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    } else if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path)
}

/// Resolve the absolute path to a trusted OpenSSH client binary.
///
/// Only known system locations are considered; there is no implicit `PATH`
/// search, and the chosen path must be a file (not a directory).
pub fn resolve_ssh_binary() -> AppResult<String> {
    for candidate in SSH_CANDIDATES {
        let p = Path::new(candidate);
        if p.is_file() {
            return Ok((*candidate).to_string());
        }
    }
    Err(AppError::Validation(
        "OpenSSH client not found. Install it with: sudo apt install openssh-client".into(),
    ))
}

/// Whether a usable SSH binary is present, for a friendly startup check.
pub fn ssh_binary_available() -> bool {
    resolve_ssh_binary().is_ok()
}

fn resolve_binary(candidates: &[&str], what: &str) -> AppResult<String> {
    for candidate in candidates {
        if Path::new(candidate).is_file() {
            return Ok((*candidate).to_string());
        }
    }
    Err(AppError::Validation(format!(
        "{what} client not found. Install it with: sudo apt install openssh-client"
    )))
}

/// Resolve the trusted `sftp` binary.
pub fn resolve_sftp_binary() -> AppResult<String> {
    resolve_binary(SFTP_CANDIDATES, "SFTP")
}

/// Resolve the trusted `scp` binary.
pub fn resolve_scp_binary() -> AppResult<String> {
    resolve_binary(SCP_CANDIDATES, "SCP")
}

/// Build `[user@]host` for a file-transfer destination. Both parts are
/// individually validated; the `@` join is safe because neither can contain it.
pub fn ssh_destination(spec: &SshSpec) -> AppResult<String> {
    let host = validate_host(&spec.host)?;
    match &spec.username {
        Some(user) => Ok(format!("{}@{host}", validate_username(user)?)),
        None => Ok(host),
    }
}

fn push_common_transfer_args(args: &mut Vec<String>, spec: &SshSpec) -> AppResult<()> {
    // sftp/scp use an uppercase `-P` for the port (unlike ssh's `-p`).
    if spec.port != 22 {
        args.push("-P".to_string());
        args.push(spec.port.to_string());
    }
    if let Some(identity) = &spec.identity_file {
        args.push("-i".to_string());
        args.push(validate_identity_file(identity)?);
    }
    if let Some(jump) = &spec.proxy_jump {
        args.push("-J".to_string());
        args.push(validate_proxy_jump(jump)?);
    }
    Ok(())
}

/// Build the argv for an interactive `sftp` session run under a PTY.
pub fn build_sftp_argv(spec: &SshSpec) -> AppResult<Vec<String>> {
    let sftp = resolve_sftp_binary()?;
    validate_port(spec.port)?;
    let mut args = vec![sftp];
    push_common_transfer_args(&mut args, spec)?;
    args.push(ssh_destination(spec)?);
    Ok(args)
}

/// Build the argv for a batch-mode `sftp -b -` session (commands on stdin).
pub fn build_sftp_batch_argv(spec: &SshSpec) -> AppResult<Vec<String>> {
    let sftp = resolve_sftp_binary()?;
    validate_port(spec.port)?;
    let mut args = vec![sftp, "-b".to_string(), "-".to_string()];
    push_common_transfer_args(&mut args, spec)?;
    args.push(ssh_destination(spec)?);
    Ok(args)
}

/// Build the argv for a single `scp` transfer. `paths` are the source and
/// destination operands as the caller computed them (one may be remote).
pub fn build_scp_argv(spec: &SshSpec, recursive: bool, paths: &[String]) -> AppResult<Vec<String>> {
    if paths.len() != 2 {
        return Err(AppError::Validation(
            "scp requires a source and a destination".into(),
        ));
    }
    let scp = resolve_scp_binary()?;
    validate_port(spec.port)?;
    let mut args = vec![scp];
    if recursive {
        args.push("-r".to_string());
    }
    push_common_transfer_args(&mut args, spec)?;
    for p in paths {
        if p.is_empty() || p.chars().any(|c| c.is_control()) {
            return Err(AppError::Validation("invalid transfer path".into()));
        }
        args.push(p.clone());
    }
    Ok(args)
}

/// Resolve the interactive shell to run for a local PTY session. Only absolute
/// paths that resolve to real executables are accepted.
pub fn resolve_local_shell() -> AppResult<String> {
    if let Some(shell) = std::env::var_os("SHELL") {
        let shell = PathBuf::from(&shell);
        if shell.is_absolute() && shell.is_file() {
            return Ok(shell.to_string_lossy().into_owned());
        }
    }
    for candidate in ["/bin/bash", "/bin/sh"] {
        if Path::new(candidate).is_file() {
            return Ok(candidate.to_string());
        }
    }
    Err(AppError::Validation("no usable local shell found".into()))
}

/// Build the complete argv array for an SSH session.
///
/// Invariants enforced here:
/// - argv[0] is an absolute trusted OpenSSH path, never a shell;
/// - each option is a distinct, typed argument;
/// - no option is added that weakens host-key verification;
/// - no password or secret is ever placed on the command line.
pub fn build_ssh_argv(spec: &SshSpec) -> AppResult<Vec<String>> {
    let ssh = resolve_ssh_binary()?;
    let host = validate_host(&spec.host)?;
    validate_port(spec.port)?;

    let mut args: Vec<String> = vec![ssh];
    // Request a remote PTY. We already run inside a PTY, so this guarantees
    // the remote side allocates a tty for interactive CLIs.
    args.push("-tt".to_string());

    if spec.compression {
        args.push("-C".to_string());
    }
    if spec.forward_agent {
        args.push("-A".to_string());
    }
    if spec.x11_forwarding {
        args.push("-X".to_string());
    }
    if let Some(cert) = &spec.certificate_file {
        if !cert.is_empty() {
            args.push("-o".to_string());
            args.push(format!("CertificateFile={}", validate_identity_file(cert)?));
        }
    }
    if let Some(provider) = &spec.pkcs11_provider {
        if !provider.is_empty() {
            args.push("-o".to_string());
            args.push(format!(
                "PKCS11Provider={}",
                validate_identity_file(provider)?
            ));
        }
    }

    if let Some(username) = &spec.username {
        let username = validate_username(username)?;
        args.push("-l".to_string());
        args.push(username);
    }
    if spec.port != 22 {
        args.push("-p".to_string());
        args.push(spec.port.to_string());
    }
    if let Some(identity) = &spec.identity_file {
        args.push("-i".to_string());
        args.push(validate_identity_file(identity)?);
    }
    if let Some(jump) = &spec.proxy_jump {
        args.push("-J".to_string());
        args.push(validate_proxy_jump(jump)?);
    }
    // These `-o` options never weaken host-key verification; they only bound
    // connection and keepalive timing (PRD FR-01).
    if let Some(timeout) = spec.connect_timeout {
        if timeout > 0 {
            args.push("-o".to_string());
            args.push(format!("ConnectTimeout={timeout}"));
        }
    }
    if let Some(interval) = spec.server_alive_interval {
        if interval > 0 {
            args.push("-o".to_string());
            args.push(format!("ServerAliveInterval={interval}"));
        }
    }

    // Destination is always the final argument and is guaranteed not to start
    // with '-' by validate_host.
    args.push(host);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(host: &str) -> SshSpec {
        SshSpec {
            host: host.to_string(),
            port: 22,
            username: None,
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
    fn rejects_host_that_looks_like_an_option() {
        // Core security acceptance case: a host beginning with '-' must never
        // spawn a process.
        assert!(validate_host("-oProxyCommand=touch /tmp/pwned").is_err());
        assert!(build_ssh_argv(&spec("-oProxyCommand=touch /tmp/pwned")).is_err());
    }

    #[test]
    fn rejects_hosts_with_whitespace_or_control_chars() {
        assert!(validate_host("host name").is_err());
        assert!(validate_host("host\nname").is_err());
        assert!(validate_host("").is_err());
    }

    #[test]
    fn accepts_common_host_forms() {
        for host in [
            "router.example.net",
            "192.0.2.10",
            "2001:db8::1",
            "[2001:db8::1]",
            "core-jkt-01",
        ] {
            assert!(validate_host(host).is_ok(), "expected {host} to be valid");
        }
    }

    #[test]
    fn rejects_username_that_looks_like_an_option() {
        assert!(validate_username("-oProxyCommand=x").is_err());
        assert!(validate_username("bad user").is_err());
        assert!(validate_username("operator").is_ok());
    }

    #[test]
    fn rejects_zero_port() {
        assert!(validate_port(0).is_err());
        assert!(validate_port(22).is_ok());
        assert!(validate_port(65535).is_ok());
    }

    #[test]
    fn identity_file_must_be_absolute() {
        assert!(validate_identity_file("relative/key").is_err());
        assert!(validate_identity_file("/home/user/.ssh/id_ed25519").is_ok());
    }

    #[test]
    fn proxy_jump_accepts_and_rejects_correctly() {
        assert!(validate_proxy_jump("bastion.example.net").is_ok());
        assert!(validate_proxy_jump("ops@bastion.example.net:2222").is_ok());
        assert!(validate_proxy_jump("[2001:db8::1]:22").is_ok());
        assert!(validate_proxy_jump("a.example.net,b.example.net").is_ok());
        assert!(validate_proxy_jump("-oProxyCommand=x").is_err());
        assert!(validate_proxy_jump("host:notaport").is_err());
        assert!(validate_proxy_jump("").is_err());
    }

    #[test]
    fn argv_contains_no_host_key_bypass() {
        let mut s = spec("router.example.net");
        s.username = Some("operator".into());
        s.identity_file = Some("/home/operator/.ssh/id_ed25519".into());
        s.proxy_jump = Some("bastion.example.net".into());
        s.port = 2222;
        let argv = build_ssh_argv(&s).expect("argv");

        let joined = argv.join(" ");
        assert!(!joined.contains("StrictHostKeyChecking=no"));
        assert!(!joined.contains("UserKnownHostsFile=/dev/null"));
        assert!(!joined.contains("sshpass"));

        // Structured args, not a single shell string.
        assert!(argv[0].ends_with("/ssh"));
        assert!(argv.contains(&"-l".to_string()));
        assert!(argv.contains(&"operator".to_string()));
        assert!(argv.contains(&"-p".to_string()));
        assert!(argv.contains(&"2222".to_string()));
        assert!(argv.contains(&"-i".to_string()));
        assert!(argv.contains(&"-J".to_string()));
        // Destination last, no leading '-'.
        assert_eq!(argv.last().unwrap(), "router.example.net");
    }

    #[test]
    fn argv_omits_default_port_flag() {
        let argv = build_ssh_argv(&spec("router.example.net")).unwrap();
        assert!(!argv.contains(&"-p".to_string()));
    }

    #[test]
    fn advanced_options_are_opt_in() {
        // Off by default.
        let base = build_ssh_argv(&spec("router.example.net")).unwrap();
        assert!(!base.contains(&"-C".to_string()));
        assert!(!base.contains(&"-A".to_string()));
        assert!(!base.contains(&"-X".to_string()));

        let mut s = spec("router.example.net");
        s.compression = true;
        s.forward_agent = true;
        s.x11_forwarding = true;
        s.certificate_file = Some("/home/operator/.ssh/id_ed25519-cert.pub".into());
        let argv = build_ssh_argv(&s).unwrap();
        assert!(argv.contains(&"-C".to_string()));
        assert!(argv.contains(&"-A".to_string()));
        assert!(argv.contains(&"-X".to_string()));
        assert!(argv.iter().any(|a| a.starts_with("CertificateFile=")));
    }

    #[test]
    fn certificate_file_must_be_absolute() {
        let mut s = spec("router.example.net");
        s.certificate_file = Some("relative/cert.pub".into());
        assert!(build_ssh_argv(&s).is_err());
    }
}
