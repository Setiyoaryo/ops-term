//! OpsTerm core — all behaviour that is independent of the UI toolkit.
//!
//! Both the Tauri desktop app and the native GTK4/VTE app build on these
//! modules: domain types, validated process spawning, connection providers,
//! SQLite storage, logging, credentials, tunnels and SFTP.

pub mod askpass;
pub mod credential;
pub mod domain;
pub mod error;
pub mod logging;
pub mod providers;
pub mod security;
pub mod sftp;
pub mod storage;
pub mod text;
pub mod transfer;
pub mod tunnel;
pub mod vault;
