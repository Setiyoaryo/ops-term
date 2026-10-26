//! Persisted domain models (PRD section 8).
//!
//! These are the records that survive restarts: saved sessions, folders,
//! commands, workspaces and settings. They never carry live process handles or
//! secrets — credential *references* only (PRD section 7.5).

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Current UTC time as an ISO-8601 / RFC-3339 string.
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// A folder in the saved-session tree. `parent_id` NULL means top level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating/updating a folder.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

/// A saved connection profile. Secrets are never stored here; only a
/// `credential_ref` that points at the OS keyring (Phase 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSession {
    pub id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub username: Option<String>,
    pub ssh_alias: Option<String>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub settings_override: Option<serde_json::Value>,
    pub credential_ref: Option<String>,
    pub is_favorite: bool,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating/updating a saved session.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSessionInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub name: String,
    pub host: String,
    #[serde(default = "crate::domain::default_ssh_port")]
    pub port: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub ssh_alias: Option<String>,
    #[serde(default)]
    pub identity_file: Option<String>,
    #[serde(default)]
    pub proxy_jump: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub settings_override: Option<serde_json::Value>,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

fn default_protocol() -> String {
    "ssh".to_string()
}

impl SavedSessionInput {
    /// Validate fields that could otherwise become arbitrary process arguments
    /// or corrupt the tree (PRD security acceptance cases).
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation(
                "session name must not be empty".into(),
            ));
        }
        match self.protocol.as_str() {
            "ssh" | "local" | "telnet" | "serial" => {}
            other => {
                return Err(AppError::Validation(format!(
                    "unsupported protocol: {other}"
                )))
            }
        }
        if self.protocol == "ssh" || self.protocol == "telnet" {
            crate::security::validate_host(&self.host)?;
            crate::security::validate_port(self.port)?;
        }
        if let Some(user) = &self.username {
            if !user.is_empty() {
                crate::security::validate_username(user)?;
            }
        }
        if let Some(identity) = &self.identity_file {
            if !identity.is_empty() {
                crate::security::validate_identity_file(identity)?;
            }
        }
        if let Some(jump) = &self.proxy_jump {
            if !jump.is_empty() {
                crate::security::validate_proxy_jump(jump)?;
            }
        }
        Ok(())
    }
}

/// A reusable command snippet (FR-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub id: String,
    pub group_id: Option<String>,
    pub name: String,
    pub content: String,
    pub vendor_profile: Option<String>,
    pub execute_mode: String,
    pub risk_level: String,
    pub variables: serde_json::Value,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub vendor_profile: Option<String>,
    #[serde(default = "default_execute_mode")]
    pub execute_mode: String,
    #[serde(default = "default_risk")]
    pub risk_level: String,
    #[serde(default = "default_json_object")]
    pub variables: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_execute_mode() -> String {
    "insert".to_string()
}
fn default_risk() -> String {
    "low".to_string()
}
fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandGroup {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandGroupInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

/// A named workspace layout snapshot (FR-10). Never stores secrets or buffers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub layout: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_json_object")]
    pub layout: serde_json::Value,
}

/// A logged terminal session's index entry (FR-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub session_id: Option<String>,
    pub label: Option<String>,
    pub file_path: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub size_bytes: i64,
}

/// A reusable automation script / runbook (expect DSL). Explicitly user-run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runbook {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub script: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunbookInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub script: String,
}

/// A named credential in the inventory (FR-19). The secret itself lives only in
/// the OS keyring; this record is metadata for selection and tagging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRecord {
    pub name: String,
    pub username: Option<String>,
    /// `password`, `passphrase` or `enable`.
    pub kind: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Recently used connection label (FR-05 ordering helpers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentConnection {
    pub id: String,
    pub session_id: Option<String>,
    pub safe_host_label: String,
    pub last_used_at: String,
}

/// The single persisted settings document. Stored as one JSON blob keyed
/// `app.settings` so a partially written update can never corrupt unrelated
/// fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub general: GeneralSettings,
    pub terminal: TerminalSettings,
    pub ssh: SshSettings,
    pub logging: LoggingSettings,
    pub appearance: AppearanceSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    /// Confirm before closing a connected tab.
    pub confirm_close: bool,
    /// Default protocol offered by the session editor.
    pub default_protocol: String,
    /// Reopen the tabs that were open when the app last exited.
    pub reopen_last_tabs: bool,
    /// Automatically retry dropped sessions (bounded attempts).
    #[serde(default)]
    pub auto_reconnect: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: u16,
    pub scrollback: u32,
    pub color_scheme: String,
    pub cursor_style: String,
    pub cursor_blink: bool,
    /// What the Backspace key sends: `del` (0x7f, modern Linux) or `bs`
    /// (0x08, required by many Huawei/Cisco CLIs).
    #[serde(default = "default_backspace")]
    pub backspace: String,
    /// Copy the terminal selection to the clipboard automatically.
    #[serde(default)]
    pub copy_on_select: bool,
}

fn default_backspace() -> String {
    "del".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshSettings {
    pub connect_timeout_secs: u16,
    pub server_alive_interval_secs: u16,
    pub term: String,
    pub identity_file: Option<String>,
    #[serde(default)]
    pub compression: bool,
    #[serde(default)]
    pub forward_agent: bool,
    #[serde(default)]
    pub x11_forwarding: bool,
    #[serde(default)]
    pub certificate_file: Option<String>,
    #[serde(default)]
    pub pkcs11_provider: Option<String>,
    /// `openssh` (default) or `native`.
    #[serde(default = "default_engine")]
    pub engine: String,
}

fn default_engine() -> String {
    "openssh".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingSettings {
    pub enabled_by_default: bool,
    pub log_dir: Option<String>,
    /// `raw` or `text`.
    pub mode: String,
    pub timestamps: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    /// `dark`, `light` or `system`.
    pub ui_theme: String,
    pub density: String,
}

impl AppSettings {
    /// Validate ranges so a malformed settings document cannot produce an
    /// unusable terminal (PRD FR-07 acceptance).
    pub fn validate(&self) -> AppResult<()> {
        if !(10..=24).contains(&self.terminal.font_size) {
            return Err(AppError::Validation(
                "terminal font size must be between 10 and 24".into(),
            ));
        }
        match self.terminal.scrollback {
            2000 | 5000 | 10000 | 50000 => {}
            _ => {
                return Err(AppError::Validation(
                    "scrollback must be one of 2000, 5000, 10000, 50000".into(),
                ))
            }
        }
        match self.appearance.ui_theme.as_str() {
            "dark" | "light" | "system" => {}
            _ => return Err(AppError::Validation("invalid ui theme".into())),
        }
        match self.general.default_protocol.as_str() {
            "ssh" | "local" | "telnet" | "serial" => {}
            _ => return Err(AppError::Validation("invalid default protocol".into())),
        }
        match self.logging.mode.as_str() {
            "raw" | "text" => {}
            _ => return Err(AppError::Validation("invalid logging mode".into())),
        }
        match self.ssh.engine.as_str() {
            "openssh" | "native" => {}
            _ => return Err(AppError::Validation("invalid ssh engine".into())),
        }
        match self.terminal.backspace.as_str() {
            "del" | "bs" => {}
            _ => return Err(AppError::Validation("invalid backspace mode".into())),
        }
        Ok(())
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            general: GeneralSettings {
                confirm_close: true,
                default_protocol: "ssh".into(),
                reopen_last_tabs: false,
                auto_reconnect: false,
            },
            terminal: TerminalSettings {
                font_family: "\"JetBrains Mono\", \"DejaVu Sans Mono\", \"Ubuntu Mono\", monospace"
                    .into(),
                font_size: 13,
                scrollback: 5000,
                color_scheme: "default-dark".into(),
                cursor_style: "block".into(),
                cursor_blink: true,
                backspace: default_backspace(),
                copy_on_select: false,
            },
            ssh: SshSettings {
                connect_timeout_secs: 20,
                server_alive_interval_secs: 60,
                term: "xterm-256color".into(),
                identity_file: None,
                compression: false,
                forward_agent: false,
                x11_forwarding: false,
                certificate_file: None,
                pkcs11_provider: None,
                engine: default_engine(),
            },
            logging: LoggingSettings {
                enabled_by_default: false,
                log_dir: None,
                mode: "raw".into(),
                timestamps: false,
            },
            appearance: AppearanceSettings {
                ui_theme: "dark".into(),
                density: "compact".into(),
            },
        }
    }
}
