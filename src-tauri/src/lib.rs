//! OpsTerm — Linux-first secure terminal client (Tauri desktop app).
//!
//! The UI-agnostic behaviour now lives in the `opsterm-core` crate and is
//! re-exported here so existing `crate::domain`, `crate::providers`, … paths
//! keep working. Only the Tauri IPC surface and the session supervisor (which
//! emits Tauri events) live in this crate.

pub use opsterm_core::*;

pub mod commands;
pub mod session_supervisor;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

use commands::AppState;
use logging::LogManager;
use session_supervisor::SessionSupervisor;
use storage::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let db = Arc::new(
                Database::open(&data_dir.join("opsterm.db"))
                    .expect("failed to open application database"),
            );
            let settings = db.load_settings().unwrap_or_default();
            let log_dir = settings
                .logging
                .log_dir
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir.join("logs"));
            let logs = Arc::new(LogManager::new(log_dir));
            let supervisor = Arc::new(SessionSupervisor::new(app.handle().clone(), logs.clone()));
            let tunnels = Arc::new(tunnel::TunnelManager::new());
            let transfers = Arc::new(transfer::TransferManager::new());
            app.manage(AppState {
                supervisor,
                db,
                logs,
                tunnels,
                transfers,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connection_open_local,
            commands::connection_open_ssh,
            commands::connection_open_saved,
            commands::connection_open_sftp,
            commands::connection_open_telnet,
            commands::connection_open_serial,
            commands::serial_devices_list,
            commands::connection_serial_break,
            commands::connection_write,
            commands::connection_resize,
            commands::connection_close,
            commands::connection_reconnect,
            commands::ssh_available,
            commands::sessions_list,
            commands::session_get,
            commands::session_save,
            commands::session_delete,
            commands::folders_list,
            commands::folder_save,
            commands::folder_delete,
            commands::command_groups_list,
            commands::command_group_save,
            commands::command_group_delete,
            commands::commands_list,
            commands::command_save,
            commands::command_delete,
            commands::command_dispatch,
            commands::workspaces_list,
            commands::workspace_save,
            commands::workspace_delete,
            commands::settings_get,
            commands::settings_set,
            commands::log_start,
            commands::log_stop,
            commands::log_status,
            commands::logs_list,
            commands::log_read,
            commands::recent_list,
            commands::session_wait_for,
            commands::automation_run,
            commands::tunnel_start,
            commands::tunnel_list,
            commands::tunnel_stop,
            commands::runbooks_list,
            commands::runbook_save,
            commands::runbook_delete,
            commands::sftp_list,
            commands::sftp_mkdir,
            commands::sftp_remove,
            commands::sftp_rename,
            commands::sftp_transfer_start,
            commands::transfers_list,
            commands::transfer_cancel,
            commands::credential_set,
            commands::credential_get,
            commands::credential_delete,
            commands::credentials_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpsTerm");
}
