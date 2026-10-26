//! Typed Tauri IPC commands.
//!
//! Every command validates its inputs in Rust before touching a process or the
//! filesystem; the renderer never gets to spawn anything directly
//! (PRD section 9 and 10).

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::domain::{
    default_telnet_port, AppSettings, CommandGroup, CommandGroupInput, CommandInput, Folder,
    FolderInput, LogEntry, RecentConnection, SavedSession, SavedSessionInput, SerialSpec, SshSpec,
    TelnetSpec, TerminalDimensions, Workspace, WorkspaceInput,
};
use crate::error::{AppError, AppResult};
use crate::logging::LogOptions;
use crate::security;
use crate::session_supervisor::SessionSupervisor;
use crate::storage::Database;

/// Managed application state.
pub struct AppState {
    pub supervisor: Arc<SessionSupervisor>,
    pub db: Arc<Database>,
    pub logs: Arc<crate::logging::LogManager>,
    pub tunnels: Arc<crate::tunnel::TunnelManager>,
    pub transfers: Arc<crate::transfer::TransferManager>,
}

/// Open an interactive SFTP session from a saved SSH profile.
#[tauri::command]
pub fn connection_open_sftp(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<String> {
    let saved = state.db.get_session(&session_id)?;
    let settings = state.db.load_settings()?;
    let spec = spec_from_saved(&saved, &settings);
    let id = state
        .supervisor
        .open_sftp(spec, TerminalDimensions { cols, rows })?;
    let _ = state
        .db
        .touch_recent(Some(&session_id), &format!("(sftp) {}", saved.name));
    Ok(id)
}

// ---- File transfer -------------------------------------------------------

#[tauri::command]
pub fn sftp_transfer_start(
    state: State<'_, AppState>,
    request: crate::transfer::TransferRequest,
) -> AppResult<String> {
    state.transfers.start(request)
}

#[tauri::command]
pub fn transfers_list(state: State<'_, AppState>) -> AppResult<Vec<crate::transfer::TransferView>> {
    Ok(state.transfers.list())
}

#[tauri::command]
pub fn transfer_cancel(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.transfers.cancel(&id)
}

// ---- Credentials (OS keyring) -------------------------------------------

#[tauri::command]
pub fn credential_set(
    state: State<'_, AppState>,
    name: String,
    secret: String,
    username: Option<String>,
    kind: Option<String>,
    tags: Option<Vec<String>>,
) -> AppResult<()> {
    crate::credential::store(&name, &secret)?;
    let kind = kind.unwrap_or_else(|| "password".into());
    if !matches!(kind.as_str(), "password" | "passphrase" | "enable") {
        return Err(AppError::Validation("invalid credential kind".into()));
    }
    let tags = tags.unwrap_or_default();
    state
        .db
        .upsert_credential(&name, username.as_deref(), &kind, &tags)?;
    Ok(())
}

/// Explicit, user-initiated reveal of a stored secret. Never called by the SSH
/// layer to auto-answer prompts (PRD FR-19).
#[tauri::command]
pub fn credential_get(name: String) -> AppResult<String> {
    crate::credential::resolve(&name)
}

#[tauri::command]
pub fn credential_delete(state: State<'_, AppState>, name: String) -> AppResult<()> {
    crate::credential::remove(&name)?;
    state.db.remove_credential_name(&name)?;
    Ok(())
}

#[tauri::command]
pub fn credentials_list(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::CredentialRecord>> {
    state.db.list_credentials()
}

/// Result of starting a terminal log.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStartResult {
    pub id: String,
    pub path: String,
}

/// Outcome of a command sent to one target session. Broadcast is deliberately
/// best-effort: partial success is reported rather than rolled back (FR-12).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchResult {
    pub session_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// Result of one automation step.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStepResult {
    pub index: usize,
    pub kind: String,
    pub ok: bool,
    pub detail: String,
}

/// A single automation step. A flat struct with optional fields keeps the JSON
/// shape simple across the IPC boundary.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStep {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub append_newline: Option<bool>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub ms: Option<u64>,
}

/// Wait for a regex to appear in a session's recent output.
#[tauri::command]
pub fn session_wait_for(
    state: State<'_, AppState>,
    session_id: String,
    pattern: String,
    timeout_ms: u64,
) -> AppResult<crate::session_supervisor::WaitResult> {
    state.supervisor.wait_for(&session_id, &pattern, timeout_ms)
}

/// Run an explicit, user-triggered automation sequence against sessions.
/// Steps are bounded in count and total time; there is no background schedule.
#[tauri::command]
pub fn automation_run(
    state: State<'_, AppState>,
    steps: Vec<AutomationStep>,
) -> AppResult<Vec<AutomationStepResult>> {
    if steps.len() > 200 {
        return Err(AppError::Validation("too many automation steps".into()));
    }
    let started = std::time::Instant::now();
    let mut results = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        if started.elapsed() > std::time::Duration::from_secs(600) {
            results.push(AutomationStepResult {
                index,
                kind: step.kind.clone(),
                ok: false,
                detail: "automation budget exceeded".into(),
            });
            break;
        }
        let outcome: AppResult<String> = match step.kind.as_str() {
            "send" => {
                let sid = step
                    .session_id
                    .as_ref()
                    .ok_or_else(|| AppError::Validation("send requires sessionId".into()));
                match (sid, step.data.as_ref()) {
                    (Ok(sid), Some(data)) => {
                        let mut payload = data.clone().into_bytes();
                        if step.append_newline.unwrap_or(false) {
                            payload.push(b'\n');
                        }
                        state.supervisor.write(sid, &payload).map(|_| "sent".into())
                    }
                    (Err(e), _) => Err(e),
                    (_, None) => Err(AppError::Validation("send requires data".into())),
                }
            }
            "waitFor" => {
                let sid = step
                    .session_id
                    .as_ref()
                    .ok_or_else(|| AppError::Validation("waitFor requires sessionId".into()))?;
                let pattern = step
                    .pattern
                    .as_ref()
                    .ok_or_else(|| AppError::Validation("waitFor requires pattern".into()))?;
                let timeout = step.timeout_ms.unwrap_or(10_000);
                state.supervisor.wait_for(sid, pattern, timeout).map(|r| {
                    if r.matched {
                        format!("matched: {}", r.text)
                    } else {
                        "timeout".to_string()
                    }
                })
            }
            "sleep" => {
                let ms = step.ms.unwrap_or(0).min(60_000);
                std::thread::sleep(std::time::Duration::from_millis(ms));
                Ok(format!("slept {ms}ms"))
            }
            other => Err(AppError::Validation(format!("unknown step: {other}"))),
        };
        match outcome {
            Ok(detail) => results.push(AutomationStepResult {
                index,
                kind: step.kind.clone(),
                ok: true,
                detail,
            }),
            Err(e) => {
                results.push(AutomationStepResult {
                    index,
                    kind: step.kind.clone(),
                    ok: false,
                    detail: e.to_string(),
                });
                break;
            }
        }
    }
    Ok(results)
}

/// Send `content` to each target session. Used by the Command Composer and
/// Button Bar. Scope validation happens in the UI, but every target is checked
/// here too: unknown or closed sessions are reported as failures, never
/// silently skipped.
#[tauri::command]
pub fn command_dispatch(
    state: State<'_, AppState>,
    session_ids: Vec<String>,
    content: String,
    append_enter: bool,
) -> AppResult<Vec<DispatchResult>> {
    if content.is_empty() {
        return Err(AppError::Validation("command content is empty".into()));
    }
    if content.len() > 64 * 1024 {
        return Err(AppError::Validation("command content is too large".into()));
    }
    let mut payload = content.into_bytes();
    if append_enter {
        payload.push(b'\n');
    }
    let mut results = Vec::with_capacity(session_ids.len());
    for id in session_ids {
        let outcome = state.supervisor.write(&id, &payload);
        results.push(DispatchResult {
            session_id: id,
            ok: outcome.is_ok(),
            error: outcome.err().map(|e| e.to_string()),
        });
    }
    Ok(results)
}

// ---- Connections ---------------------------------------------------------

/// Open a local shell session in a new PTY.
#[tauri::command]
pub fn connection_open_local(
    state: State<'_, AppState>,
    cols: u16,
    rows: u16,
) -> AppResult<String> {
    let id = state
        .supervisor
        .open_local(TerminalDimensions { cols, rows })?;
    let _ = state.db.touch_recent(None, "Local shell");
    Ok(id)
}

/// Open an SSH session using the validated structured spec.
#[tauri::command]
pub fn connection_open_ssh(
    state: State<'_, AppState>,
    spec: SshSpec,
    cols: u16,
    rows: u16,
) -> AppResult<String> {
    let label = spec
        .username
        .as_ref()
        .map(|u| format!("{u}@{}", spec.host))
        .unwrap_or_else(|| spec.host.clone());
    let id = state
        .supervisor
        .open_ssh(spec, TerminalDimensions { cols, rows })?;
    let _ = state.db.touch_recent(None, &label);
    Ok(id)
}

/// Open a connection from a saved session profile, applying global settings
/// defaults where the profile does not override them.
#[tauri::command]
pub fn connection_open_saved(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<String> {
    let saved = state.db.get_session(&session_id)?;
    let dims = TerminalDimensions { cols, rows };
    let settings = state.db.load_settings()?;
    match saved.protocol.as_str() {
        "local" => {
            let id = state.supervisor.open_local(dims)?;
            let _ = state.db.touch_recent(Some(&session_id), &saved.name);
            Ok(id)
        }
        "ssh" => {
            let mut spec = spec_from_saved(&saved, &settings);
            // Resolve an inventory credential (by name) into a memory-only
            // password used for this connection. Never persisted.
            if let Some(reference) = saved.credential_ref.as_deref().filter(|r| !r.is_empty()) {
                let (secret, cred_user) = resolve_credential(&state, reference)?;
                spec.password = secret;
                if spec
                    .username
                    .as_deref()
                    .map(|u| u.trim().is_empty())
                    .unwrap_or(true)
                {
                    spec.username = cred_user;
                }
            }
            let id = state.supervisor.open_ssh(spec, dims)?;
            let _ = state.db.touch_recent(Some(&session_id), &saved.name);
            Ok(id)
        }
        "telnet" => {
            let mut spec = telnet_spec_from_saved(&saved, &settings);
            if let Some(reference) = saved.credential_ref.as_deref().filter(|r| !r.is_empty()) {
                let (secret, cred_user) = resolve_credential(&state, reference)?;
                spec.password = secret;
                if spec
                    .username
                    .as_deref()
                    .map(|u| u.trim().is_empty())
                    .unwrap_or(true)
                {
                    spec.username = cred_user;
                }
            }
            let id = state.supervisor.open_telnet(spec, dims)?;
            let _ = state.db.touch_recent(Some(&session_id), &saved.name);
            Ok(id)
        }
        "serial" => {
            let spec = serial_spec_from_saved(&saved)?;
            let id = state.supervisor.open_serial(spec, dims)?;
            let _ = state.db.touch_recent(Some(&session_id), &saved.name);
            Ok(id)
        }
        other => Err(AppError::Validation(format!(
            "unsupported protocol: {other}"
        ))),
    }
}

/// Open a Telnet session directly (unencrypted; the UI labels it as insecure).
#[tauri::command]
pub fn connection_open_telnet(
    state: State<'_, AppState>,
    spec: TelnetSpec,
    cols: u16,
    rows: u16,
) -> AppResult<String> {
    let label = match &spec.username {
        Some(u) => format!("(telnet) {u}@{}", spec.host),
        None => format!("(telnet) {}", spec.host),
    };
    let id = state
        .supervisor
        .open_telnet(spec, TerminalDimensions { cols, rows })?;
    let _ = state.db.touch_recent(None, &label);
    Ok(id)
}

/// Open a serial console session directly.
#[tauri::command]
pub fn connection_open_serial(
    state: State<'_, AppState>,
    spec: SerialSpec,
    cols: u16,
    rows: u16,
) -> AppResult<String> {
    let label = format!("(serial) {}", spec.device);
    let id = state
        .supervisor
        .open_serial(spec, TerminalDimensions { cols, rows })?;
    let _ = state.db.touch_recent(None, &label);
    Ok(id)
}

/// Enumerate serial devices for the connection dialog.
#[tauri::command]
pub fn serial_devices_list() -> Vec<String> {
    crate::providers::serial::list_devices()
}

/// Send a serial break on a live session.
#[tauri::command]
pub fn connection_serial_break(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.supervisor.send_break(&session_id)
}

fn telnet_spec_from_saved(saved: &SavedSession, settings: &AppSettings) -> TelnetSpec {
    TelnetSpec {
        host: saved.host.clone(),
        port: if saved.port == 0 {
            default_telnet_port()
        } else {
            saved.port
        },
        username: saved.username.clone(),
        term: Some(settings.ssh.term.clone()),
        password: None,
    }
}

/// Load a credential's secret and optional username from the keyring + metadata.
fn resolve_credential(
    state: &State<'_, AppState>,
    reference: &str,
) -> AppResult<(Option<String>, Option<String>)> {
    let secret = match crate::credential::resolve(reference) {
        Ok(s) => Some(s),
        Err(e) => {
            return Err(AppError::Validation(format!(
                "credential \"{reference}\" could not be loaded: {e}"
            )))
        }
    };
    let username = state.db.get_credential(reference)?.and_then(|r| r.username);
    Ok((secret, username))
}

fn serial_spec_from_saved(saved: &SavedSession) -> AppResult<SerialSpec> {
    // Serial parameters live in the session's settings override so a saved
    // profile round-trips baud/parity/flow.
    let mut spec = SerialSpec {
        device: saved.host.clone(),
        baud: 9600,
        data_bits: 8,
        parity: "none".into(),
        stop_bits: 1,
        flow_control: "none".into(),
        newline: "crlf".into(),
    };
    if let Some(over) = &saved.settings_override {
        if let Some(serial) = over.get("serial") {
            if let Some(baud) = serial.get("baud").and_then(|v| v.as_u64()) {
                spec.baud = baud as u32;
            }
            if let Some(d) = serial.get("dataBits").and_then(|v| v.as_u64()) {
                spec.data_bits = d as u8;
            }
            if let Some(p) = serial.get("parity").and_then(|v| v.as_str()) {
                spec.parity = p.to_string();
            }
            if let Some(s) = serial.get("stopBits").and_then(|v| v.as_u64()) {
                spec.stop_bits = s as u8;
            }
            if let Some(f) = serial.get("flowControl").and_then(|v| v.as_str()) {
                spec.flow_control = f.to_string();
            }
            if let Some(nl) = serial.get("newline").and_then(|v| v.as_str()) {
                spec.newline = nl.to_string();
            }
        }
    }
    spec.validate()?;
    Ok(spec)
}

fn spec_from_saved(saved: &SavedSession, settings: &AppSettings) -> SshSpec {
    SshSpec {
        host: saved.host.clone(),
        port: saved.port,
        username: saved.username.clone(),
        identity_file: saved
            .identity_file
            .clone()
            .or_else(|| settings.ssh.identity_file.clone()),
        proxy_jump: saved.proxy_jump.clone(),
        name: Some(saved.name.clone()),
        term: Some(settings.ssh.term.clone()),
        connect_timeout: Some(settings.ssh.connect_timeout_secs),
        server_alive_interval: Some(settings.ssh.server_alive_interval_secs),
        x11_forwarding: settings.ssh.x11_forwarding,
        forward_agent: settings.ssh.forward_agent,
        compression: settings.ssh.compression,
        certificate_file: settings.ssh.certificate_file.clone(),
        pkcs11_provider: settings.ssh.pkcs11_provider.clone(),
        engine: Some(
            saved
                .settings_override
                .as_ref()
                .and_then(|o| o.get("engine"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| settings.ssh.engine.clone()),
        ),
        password: None,
    }
}

/// Write raw terminal input bytes to a live session.
#[tauri::command]
pub fn connection_write(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> AppResult<()> {
    state.supervisor.write(&session_id, data.as_bytes())
}

/// Resize the PTY for a live session.
#[tauri::command]
pub fn connection_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    state
        .supervisor
        .resize(&session_id, TerminalDimensions { cols, rows })
}

/// Close a session and reclaim its process resources.
#[tauri::command]
pub fn connection_close(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.supervisor.close(&session_id)
}

/// Reconnect a session, replacing its process while keeping its id.
#[tauri::command]
pub fn connection_reconnect(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    state
        .supervisor
        .reconnect(&session_id, TerminalDimensions { cols, rows })
}

/// Startup capability check: is a trusted OpenSSH client installed?
#[tauri::command]
pub fn ssh_available() -> bool {
    security::ssh_binary_available()
}

// ---- Saved sessions ------------------------------------------------------

#[tauri::command]
pub fn sessions_list(state: State<'_, AppState>) -> AppResult<Vec<SavedSession>> {
    state.db.list_sessions()
}

#[tauri::command]
pub fn session_get(state: State<'_, AppState>, id: String) -> AppResult<SavedSession> {
    state.db.get_session(&id)
}

#[tauri::command]
pub fn session_save(
    state: State<'_, AppState>,
    input: SavedSessionInput,
) -> AppResult<SavedSession> {
    state.db.upsert_session(&input)
}

#[tauri::command]
pub fn session_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_session(&id)
}

// ---- Folders -------------------------------------------------------------

#[tauri::command]
pub fn folders_list(state: State<'_, AppState>) -> AppResult<Vec<Folder>> {
    state.db.list_folders()
}

#[tauri::command]
pub fn folder_save(state: State<'_, AppState>, input: FolderInput) -> AppResult<Folder> {
    state.db.upsert_folder(&input)
}

#[tauri::command]
pub fn folder_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_folder(&id)
}

// ---- Commands / Button Bar ----------------------------------------------

#[tauri::command]
pub fn command_groups_list(state: State<'_, AppState>) -> AppResult<Vec<CommandGroup>> {
    state.db.list_command_groups()
}

#[tauri::command]
pub fn command_group_save(
    state: State<'_, AppState>,
    input: CommandGroupInput,
) -> AppResult<CommandGroup> {
    state.db.upsert_command_group(&input)
}

#[tauri::command]
pub fn command_group_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_command_group(&id)
}

#[tauri::command]
pub fn commands_list(state: State<'_, AppState>) -> AppResult<Vec<crate::domain::Command>> {
    state.db.list_commands()
}

#[tauri::command]
pub fn command_save(
    state: State<'_, AppState>,
    input: CommandInput,
) -> AppResult<crate::domain::Command> {
    state.db.upsert_command(&input)
}

#[tauri::command]
pub fn command_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_command(&id)
}

// ---- Workspaces ----------------------------------------------------------

#[tauri::command]
pub fn workspaces_list(state: State<'_, AppState>) -> AppResult<Vec<Workspace>> {
    state.db.list_workspaces()
}

#[tauri::command]
pub fn workspace_save(state: State<'_, AppState>, input: WorkspaceInput) -> AppResult<Workspace> {
    state.db.upsert_workspace(&input)
}

#[tauri::command]
pub fn workspace_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_workspace(&id)
}

// ---- Settings ------------------------------------------------------------

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> AppResult<AppSettings> {
    state.db.load_settings()
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, settings: AppSettings) -> AppResult<AppSettings> {
    state.db.save_settings(&settings)?;
    Ok(settings)
}

// ---- Logging -------------------------------------------------------------

#[tauri::command]
pub fn log_start(
    state: State<'_, AppState>,
    session_id: String,
    label: Option<String>,
    mode: String,
    timestamps: bool,
) -> AppResult<LogStartResult> {
    if mode != "raw" && mode != "text" {
        return Err(AppError::Validation("invalid logging mode".into()));
    }
    let (id, path) = state.logs.start(
        &session_id,
        LogOptions {
            label: label.clone(),
            session_id: Some(session_id.clone()),
            mode,
            timestamps,
        },
    )?;
    let path_str = path.to_string_lossy().into_owned();
    state
        .db
        .record_log_start(&id, Some(&session_id), label.as_deref(), &path_str)?;
    Ok(LogStartResult { id, path: path_str })
}

#[tauri::command]
pub fn log_stop(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    if let Some((id, size, _, _)) = state.logs.stop(&session_id) {
        state.db.record_log_end(&id, size)?;
    }
    Ok(())
}

#[tauri::command]
pub fn log_status(state: State<'_, AppState>, session_id: String) -> AppResult<bool> {
    Ok(state.logs.is_active(&session_id))
}

#[tauri::command]
pub fn logs_list(state: State<'_, AppState>) -> AppResult<Vec<LogEntry>> {
    state.db.list_logs()
}

/// Read a log file for export. The path must resolve inside the log directory.
#[tauri::command]
pub fn log_read(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let dir = state
        .logs
        .dir()
        .canonicalize()
        .map_err(|e| AppError::Validation(format!("log directory unavailable: {e}")))?;
    let target = std::path::Path::new(&path)
        .canonicalize()
        .map_err(|e| AppError::Validation(format!("log file unavailable: {e}")))?;
    if !target.starts_with(&dir) {
        return Err(AppError::Validation(
            "log path is outside the log directory".into(),
        ));
    }
    Ok(std::fs::read_to_string(&target)?)
}

// ---- Recent --------------------------------------------------------------

#[tauri::command]
pub fn recent_list(state: State<'_, AppState>) -> AppResult<Vec<RecentConnection>> {
    state.db.list_recent()
}

// ---- SFTP browser --------------------------------------------------------

/// Build the SSH spec for a saved session, resolving any inventory credential.
fn ssh_spec_for(state: &State<'_, AppState>, session_id: &str) -> AppResult<SshSpec> {
    let saved = state.db.get_session(session_id)?;
    let settings = state.db.load_settings()?;
    let mut spec = spec_from_saved(&saved, &settings);
    if let Some(reference) = saved.credential_ref.as_deref().filter(|r| !r.is_empty()) {
        let (secret, cred_user) = resolve_credential(state, reference)?;
        spec.password = secret;
        if spec
            .username
            .as_deref()
            .map(|u| u.trim().is_empty())
            .unwrap_or(true)
        {
            spec.username = cred_user;
        }
    }
    Ok(spec)
}

#[tauri::command]
pub fn sftp_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<Vec<crate::sftp::SftpEntry>> {
    let spec = ssh_spec_for(&state, &session_id)?;
    crate::sftp::list(&spec, &path)
}

#[tauri::command]
pub fn sftp_mkdir(state: State<'_, AppState>, session_id: String, path: String) -> AppResult<()> {
    let spec = ssh_spec_for(&state, &session_id)?;
    crate::sftp::mkdir(&spec, &path)
}

#[tauri::command]
pub fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> AppResult<()> {
    let spec = ssh_spec_for(&state, &session_id)?;
    crate::sftp::remove(&spec, &path, is_dir)
}

#[tauri::command]
pub fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> AppResult<()> {
    let spec = ssh_spec_for(&state, &session_id)?;
    crate::sftp::rename(&spec, &from, &to)
}

// ---- Runbooks ------------------------------------------------------------

#[tauri::command]
pub fn runbooks_list(state: State<'_, AppState>) -> AppResult<Vec<crate::domain::Runbook>> {
    state.db.list_runbooks()
}

#[tauri::command]
pub fn runbook_save(
    state: State<'_, AppState>,
    input: crate::domain::RunbookInput,
) -> AppResult<crate::domain::Runbook> {
    state.db.upsert_runbook(&input)
}

#[tauri::command]
pub fn runbook_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_runbook(&id)
}

// ---- SSH tunnels ---------------------------------------------------------

#[tauri::command]
pub fn tunnel_start(
    state: State<'_, AppState>,
    spec: crate::tunnel::TunnelSpec,
) -> AppResult<String> {
    state.tunnels.start(spec)
}

#[tauri::command]
pub fn tunnel_list(state: State<'_, AppState>) -> AppResult<Vec<crate::tunnel::TunnelView>> {
    Ok(state.tunnels.list())
}

#[tauri::command]
pub fn tunnel_stop(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.tunnels.stop(&id)
}
