//! SQLite persistence for saved sessions, folders, commands, workspaces,
//! settings, log index and recent connections (PRD section 8).
//!
//! The store keeps a single connection behind a mutex. Queries are short and
//! local; correctness and a simple migration story matter more than raw
//! concurrency here. WAL mode is enabled so a concurrent reader never blocks a
//! writer and a crash leaves a recoverable database.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::{
    now_iso, AppSettings, Command, CommandGroup, CommandGroupInput, CommandInput, CredentialRecord,
    Folder, FolderInput, LogEntry, RecentConnection, Runbook, RunbookInput, SavedSession,
    SavedSessionInput, Workspace, WorkspaceInput,
};
use crate::error::{AppError, AppResult};

/// Current schema version. Bump and add a migration step when the schema
/// changes so existing user databases upgrade in place.
const SCHEMA_VERSION: i64 = 4;

pub struct Database {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    /// Open (creating if necessary) the database at `path` and run migrations.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let db = Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        };
        db.migrate()?;
        Ok(db)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS folders (
                    id          TEXT PRIMARY KEY,
                    parent_id   TEXT REFERENCES folders(id) ON DELETE CASCADE,
                    name        TEXT NOT NULL,
                    sort_order  INTEGER NOT NULL DEFAULT 0,
                    created_at  TEXT NOT NULL,
                    updated_at  TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS sessions (
                    id                    TEXT PRIMARY KEY,
                    folder_id             TEXT REFERENCES folders(id) ON DELETE SET NULL,
                    name                  TEXT NOT NULL,
                    host                  TEXT NOT NULL,
                    port                  INTEGER NOT NULL DEFAULT 22,
                    protocol              TEXT NOT NULL DEFAULT 'ssh',
                    username              TEXT,
                    ssh_alias             TEXT,
                    identity_file         TEXT,
                    proxy_jump            TEXT,
                    tags_json             TEXT NOT NULL DEFAULT '[]',
                    description           TEXT,
                    notes                 TEXT,
                    settings_override_json TEXT,
                    credential_ref        TEXT,
                    is_favorite           INTEGER NOT NULL DEFAULT 0,
                    sort_order            INTEGER NOT NULL DEFAULT 0,
                    created_at            TEXT NOT NULL,
                    updated_at            TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_sessions_folder ON sessions(folder_id);

                CREATE TABLE IF NOT EXISTS command_groups (
                    id          TEXT PRIMARY KEY,
                    parent_id   TEXT REFERENCES command_groups(id) ON DELETE CASCADE,
                    name        TEXT NOT NULL,
                    sort_order  INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS commands (
                    id                   TEXT PRIMARY KEY,
                    group_id             TEXT REFERENCES command_groups(id) ON DELETE SET NULL,
                    name                 TEXT NOT NULL,
                    content              TEXT NOT NULL,
                    vendor_profile       TEXT,
                    execute_mode         TEXT NOT NULL DEFAULT 'insert',
                    risk_level           TEXT NOT NULL DEFAULT 'low',
                    variables_schema_json TEXT NOT NULL DEFAULT '{}',
                    tags_json            TEXT NOT NULL DEFAULT '[]',
                    created_at           TEXT NOT NULL,
                    updated_at           TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS workspaces (
                    id          TEXT PRIMARY KEY,
                    name        TEXT NOT NULL,
                    description TEXT,
                    layout_json TEXT NOT NULL DEFAULT '{}',
                    created_at  TEXT NOT NULL,
                    updated_at  TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS application_settings (
                    key            TEXT PRIMARY KEY,
                    value_json     TEXT NOT NULL,
                    schema_version INTEGER NOT NULL DEFAULT 1
                );

                CREATE TABLE IF NOT EXISTS log_index (
                    id          TEXT PRIMARY KEY,
                    session_id  TEXT,
                    label       TEXT,
                    file_path   TEXT NOT NULL,
                    started_at  TEXT NOT NULL,
                    ended_at    TEXT,
                    size_bytes  INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS recent_connections (
                    id              TEXT PRIMARY KEY,
                    session_id      TEXT,
                    safe_host_label TEXT NOT NULL,
                    last_used_at    TEXT NOT NULL
                );
                "#,
            )?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            // Named credential references. Secret values live in the OS
            // keyring; this table only remembers which names exist.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS credentials (
                    name       TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL
                 );",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        if version < 3 {
            // Credential metadata: username, kind and tags for the inventory.
            for stmt in [
                "ALTER TABLE credentials ADD COLUMN username TEXT",
                "ALTER TABLE credentials ADD COLUMN kind TEXT NOT NULL DEFAULT 'password'",
                "ALTER TABLE credentials ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]'",
                "ALTER TABLE credentials ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''",
            ] {
                let _ = conn.execute(stmt, []);
            }
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        if version < 4 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS runbooks (
                    id          TEXT PRIMARY KEY,
                    name        TEXT NOT NULL,
                    description TEXT,
                    script      TEXT NOT NULL DEFAULT '',
                    created_at  TEXT NOT NULL,
                    updated_at  TEXT NOT NULL
                 );",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    // ---- Runbooks --------------------------------------------------------

    pub fn list_runbooks(&self) -> AppResult<Vec<Runbook>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, script, created_at, updated_at
             FROM runbooks ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Runbook {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                script: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_runbook(&self, input: &RunbookInput) -> AppResult<Runbook> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation(
                "runbook name must not be empty".into(),
            ));
        }
        let conn = self.conn.lock();
        let now = now_iso();
        let existing: Option<String> = match &input.id {
            Some(id) => conn
                .query_row(
                    "SELECT created_at FROM runbooks WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let created_at = existing.clone().unwrap_or_else(|| now.clone());
        conn.execute(
            "INSERT INTO runbooks (id, name, description, script, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description,
                 script=excluded.script, updated_at=excluded.updated_at",
            params![
                id,
                input.name.trim(),
                input.description,
                input.script,
                created_at,
                now
            ],
        )?;
        Ok(Runbook {
            id,
            name: input.name.trim().to_string(),
            description: input.description.clone(),
            script: input.script.clone(),
            created_at,
            updated_at: now,
        })
    }

    pub fn delete_runbook(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM runbooks WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- Credential references ------------------------------------------

    fn map_credential(r: &Row<'_>) -> rusqlite::Result<CredentialRecord> {
        let tags_json: String = r.get(3)?;
        Ok(CredentialRecord {
            name: r.get(0)?,
            username: r.get(1)?,
            kind: r.get(2)?,
            tags: parse_tags(&tags_json),
            created_at: r.get(4)?,
            updated_at: r.get(5)?,
        })
    }

    pub fn list_credentials(&self) -> AppResult<Vec<CredentialRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT name, username, kind, tags_json, created_at, updated_at
             FROM credentials ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], Self::map_credential)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_credential(&self, name: &str) -> AppResult<Option<CredentialRecord>> {
        let conn = self.conn.lock();
        let record = conn
            .query_row(
                "SELECT name, username, kind, tags_json, created_at, updated_at
                 FROM credentials WHERE name = ?1",
                params![name],
                Self::map_credential,
            )
            .optional()?;
        Ok(record)
    }

    pub fn upsert_credential(
        &self,
        name: &str,
        username: Option<&str>,
        kind: &str,
        tags: &[String],
    ) -> AppResult<()> {
        if name.trim().is_empty() {
            return Err(AppError::Validation(
                "credential name must not be empty".into(),
            ));
        }
        let conn = self.conn.lock();
        let now = now_iso();
        let tags_json = serde_json::to_string(tags)?;
        conn.execute(
            "INSERT INTO credentials (name, username, kind, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(name) DO UPDATE SET username=excluded.username, kind=excluded.kind,
                 tags_json=excluded.tags_json, updated_at=excluded.updated_at",
            params![name, username, kind, tags_json, now],
        )?;
        Ok(())
    }

    pub fn remove_credential_name(&self, name: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions SET credential_ref = NULL WHERE credential_ref = ?1",
            params![name],
        )?;
        conn.execute("DELETE FROM credentials WHERE name = ?1", params![name])?;
        Ok(())
    }

    // ---- Folders ---------------------------------------------------------

    pub fn list_folders(&self) -> AppResult<Vec<Folder>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, name, sort_order, created_at, updated_at
             FROM folders ORDER BY sort_order, name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], map_folder)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_folder(&self, input: &FolderInput) -> AppResult<Folder> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation("folder name must not be empty".into()));
        }
        let conn = self.conn.lock();
        let now = now_iso();
        if let Some(id) = &input.id {
            let existing: Option<Folder> = conn
                .query_row(
                    "SELECT id, parent_id, name, sort_order, created_at, updated_at
                     FROM folders WHERE id = ?1",
                    params![id],
                    map_folder,
                )
                .optional()?;
            let existing = existing.ok_or_else(|| AppError::NotFound(id.clone()))?;
            if input.parent_id.as_deref() == Some(id.as_str()) {
                return Err(AppError::Validation(
                    "folder cannot be its own parent".into(),
                ));
            }
            conn.execute(
                "UPDATE folders SET parent_id = ?1, name = ?2, sort_order = ?3, updated_at = ?4
                 WHERE id = ?5",
                params![
                    input.parent_id,
                    input.name.trim(),
                    input.sort_order.unwrap_or(existing.sort_order),
                    now,
                    id
                ],
            )?;
            Ok(Folder {
                id: id.clone(),
                parent_id: input.parent_id.clone(),
                name: input.name.trim().to_string(),
                sort_order: input.sort_order.unwrap_or(existing.sort_order),
                created_at: existing.created_at,
                updated_at: now,
            })
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let sort_order = input.sort_order.unwrap_or(0);
            conn.execute(
                "INSERT INTO folders (id, parent_id, name, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, input.parent_id, input.name.trim(), sort_order, now],
            )?;
            Ok(Folder {
                id,
                parent_id: input.parent_id.clone(),
                name: input.name.trim().to_string(),
                sort_order,
                created_at: now.clone(),
                updated_at: now,
            })
        }
    }

    pub fn delete_folder(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- Saved sessions --------------------------------------------------

    pub fn list_sessions(&self) -> AppResult<Vec<SavedSession>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, name, host, port, protocol, username, ssh_alias,
                    identity_file, proxy_jump, tags_json, description, notes,
                    settings_override_json, credential_ref, is_favorite, sort_order,
                    created_at, updated_at
             FROM sessions ORDER BY sort_order, name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], map_session)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_session(&self, id: &str) -> AppResult<SavedSession> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, folder_id, name, host, port, protocol, username, ssh_alias,
                    identity_file, proxy_jump, tags_json, description, notes,
                    settings_override_json, credential_ref, is_favorite, sort_order,
                    created_at, updated_at
             FROM sessions WHERE id = ?1",
            params![id],
            map_session,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(id.to_string()))
    }

    pub fn upsert_session(&self, input: &SavedSessionInput) -> AppResult<SavedSession> {
        input.validate()?;
        let conn = self.conn.lock();
        let now = now_iso();
        let tags_json = serde_json::to_string(&input.tags)?;
        let override_json = match &input.settings_override {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        let existing: Option<SavedSession> = match &input.id {
            Some(id) => conn
                .query_row(
                    "SELECT id, folder_id, name, host, port, protocol, username, ssh_alias,
                            identity_file, proxy_jump, tags_json, description, notes,
                            settings_override_json, credential_ref, is_favorite, sort_order,
                            created_at, updated_at
                     FROM sessions WHERE id = ?1",
                    params![id],
                    map_session,
                )
                .optional()?,
            None => None,
        };

        match existing {
            Some(existing) => {
                let sort_order = input.sort_order.unwrap_or(existing.sort_order);
                conn.execute(
                    "UPDATE sessions SET folder_id=?1, name=?2, host=?3, port=?4, protocol=?5,
                        username=?6, ssh_alias=?7, identity_file=?8, proxy_jump=?9, tags_json=?10,
                        description=?11, notes=?12, settings_override_json=?13, credential_ref=?14,
                        is_favorite=?15, sort_order=?16, updated_at=?17
                     WHERE id=?18",
                    params![
                        input.folder_id,
                        input.name.trim(),
                        input.host.trim(),
                        input.port,
                        input.protocol,
                        input.username,
                        input.ssh_alias,
                        input.identity_file,
                        input.proxy_jump,
                        tags_json,
                        input.description,
                        input.notes,
                        override_json,
                        input.credential_ref,
                        input.is_favorite,
                        sort_order,
                        now,
                        existing.id,
                    ],
                )?;
                Ok(SavedSession {
                    id: existing.id,
                    folder_id: input.folder_id.clone(),
                    name: input.name.trim().to_string(),
                    host: input.host.trim().to_string(),
                    port: input.port,
                    protocol: input.protocol.clone(),
                    username: input.username.clone(),
                    ssh_alias: input.ssh_alias.clone(),
                    identity_file: input.identity_file.clone(),
                    proxy_jump: input.proxy_jump.clone(),
                    tags: input.tags.clone(),
                    description: input.description.clone(),
                    notes: input.notes.clone(),
                    settings_override: input.settings_override.clone(),
                    credential_ref: input.credential_ref.clone(),
                    is_favorite: input.is_favorite,
                    sort_order,
                    created_at: existing.created_at,
                    updated_at: now,
                })
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let sort_order = input.sort_order.unwrap_or(0);
                conn.execute(
                    "INSERT INTO sessions (id, folder_id, name, host, port, protocol, username,
                        ssh_alias, identity_file, proxy_jump, tags_json, description, notes,
                        settings_override_json, credential_ref, is_favorite, sort_order,
                        created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18)",
                    params![
                        id,
                        input.folder_id,
                        input.name.trim(),
                        input.host.trim(),
                        input.port,
                        input.protocol,
                        input.username,
                        input.ssh_alias,
                        input.identity_file,
                        input.proxy_jump,
                        tags_json,
                        input.description,
                        input.notes,
                        override_json,
                        input.credential_ref,
                        input.is_favorite,
                        sort_order,
                        now,
                    ],
                )?;
                Ok(SavedSession {
                    id,
                    folder_id: input.folder_id.clone(),
                    name: input.name.trim().to_string(),
                    host: input.host.trim().to_string(),
                    port: input.port,
                    protocol: input.protocol.clone(),
                    username: input.username.clone(),
                    ssh_alias: input.ssh_alias.clone(),
                    identity_file: input.identity_file.clone(),
                    proxy_jump: input.proxy_jump.clone(),
                    tags: input.tags.clone(),
                    description: input.description.clone(),
                    notes: input.notes.clone(),
                    settings_override: input.settings_override.clone(),
                    credential_ref: input.credential_ref.clone(),
                    is_favorite: input.is_favorite,
                    sort_order,
                    created_at: now.clone(),
                    updated_at: now,
                })
            }
        }
    }

    pub fn delete_session(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- Commands --------------------------------------------------------

    pub fn list_command_groups(&self) -> AppResult<Vec<CommandGroup>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, name, sort_order FROM command_groups
             ORDER BY sort_order, name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CommandGroup {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                name: r.get(2)?,
                sort_order: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_command_group(&self, input: &CommandGroupInput) -> AppResult<CommandGroup> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation("group name must not be empty".into()));
        }
        let conn = self.conn.lock();
        let sort_order = input.sort_order.unwrap_or(0);
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        conn.execute(
            "INSERT INTO command_groups (id, parent_id, name, sort_order) VALUES (?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id, name=excluded.name,
                 sort_order=excluded.sort_order",
            params![id, input.parent_id, input.name.trim(), sort_order],
        )?;
        Ok(CommandGroup {
            id,
            parent_id: input.parent_id.clone(),
            name: input.name.trim().to_string(),
            sort_order,
        })
    }

    pub fn delete_command_group(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM command_groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_commands(&self) -> AppResult<Vec<Command>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, group_id, name, content, vendor_profile, execute_mode, risk_level,
                    variables_schema_json, tags_json, created_at, updated_at
             FROM commands ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], map_command)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_command(&self, input: &CommandInput) -> AppResult<Command> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation(
                "command name must not be empty".into(),
            ));
        }
        if input.content.is_empty() {
            return Err(AppError::Validation(
                "command content must not be empty".into(),
            ));
        }
        match input.execute_mode.as_str() {
            "insert" | "send" => {}
            _ => return Err(AppError::Validation("invalid execute mode".into())),
        }
        let conn = self.conn.lock();
        let now = now_iso();
        let tags_json = serde_json::to_string(&input.tags)?;
        let vars_json = serde_json::to_string(&input.variables)?;
        let existing: Option<(String, String)> = match &input.id {
            Some(id) => conn
                .query_row(
                    "SELECT created_at, id FROM commands WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
            None => None,
        };
        match existing {
            Some((created_at, id)) => {
                conn.execute(
                    "UPDATE commands SET group_id=?1, name=?2, content=?3, vendor_profile=?4,
                        execute_mode=?5, risk_level=?6, variables_schema_json=?7, tags_json=?8,
                        updated_at=?9 WHERE id=?10",
                    params![
                        input.group_id,
                        input.name.trim(),
                        input.content,
                        input.vendor_profile,
                        input.execute_mode,
                        input.risk_level,
                        vars_json,
                        tags_json,
                        now,
                        id,
                    ],
                )?;
                Ok(Command {
                    id,
                    group_id: input.group_id.clone(),
                    name: input.name.trim().to_string(),
                    content: input.content.clone(),
                    vendor_profile: input.vendor_profile.clone(),
                    execute_mode: input.execute_mode.clone(),
                    risk_level: input.risk_level.clone(),
                    variables: input.variables.clone(),
                    tags: input.tags.clone(),
                    created_at,
                    updated_at: now,
                })
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO commands (id, group_id, name, content, vendor_profile,
                        execute_mode, risk_level, variables_schema_json, tags_json,
                        created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
                    params![
                        id,
                        input.group_id,
                        input.name.trim(),
                        input.content,
                        input.vendor_profile,
                        input.execute_mode,
                        input.risk_level,
                        vars_json,
                        tags_json,
                        now,
                    ],
                )?;
                Ok(Command {
                    id,
                    group_id: input.group_id.clone(),
                    name: input.name.trim().to_string(),
                    content: input.content.clone(),
                    vendor_profile: input.vendor_profile.clone(),
                    execute_mode: input.execute_mode.clone(),
                    risk_level: input.risk_level.clone(),
                    variables: input.variables.clone(),
                    tags: input.tags.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                })
            }
        }
    }

    pub fn delete_command(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM commands WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- Workspaces ------------------------------------------------------

    pub fn list_workspaces(&self) -> AppResult<Vec<Workspace>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, layout_json, created_at, updated_at
             FROM workspaces ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], map_workspace)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert_workspace(&self, input: &WorkspaceInput) -> AppResult<Workspace> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation(
                "workspace name must not be empty".into(),
            ));
        }
        let conn = self.conn.lock();
        let now = now_iso();
        let layout = serde_json::to_string(&input.layout)?;
        let existing_created: Option<String> = match &input.id {
            Some(id) => conn
                .query_row(
                    "SELECT created_at FROM workspaces WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let created_at = existing_created.clone().unwrap_or_else(|| now.clone());
        conn.execute(
            "INSERT INTO workspaces (id, name, description, layout_json, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description,
                 layout_json=excluded.layout_json, updated_at=excluded.updated_at",
            params![
                id,
                input.name.trim(),
                input.description,
                layout,
                created_at,
                now
            ],
        )?;
        Ok(Workspace {
            id,
            name: input.name.trim().to_string(),
            description: input.description.clone(),
            layout: input.layout.clone(),
            created_at,
            updated_at: now,
        })
    }

    pub fn delete_workspace(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- Settings --------------------------------------------------------

    pub fn load_settings(&self) -> AppResult<AppSettings> {
        let conn = self.conn.lock();
        let json: Option<String> = conn
            .query_row(
                "SELECT value_json FROM application_settings WHERE key = 'app.settings'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match json {
            Some(json) => {
                let settings: AppSettings = serde_json::from_str(&json)?;
                Ok(settings)
            }
            None => Ok(AppSettings::default()),
        }
    }

    pub fn load_named_setting<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> AppResult<Option<T>> {
        let conn = self.conn.lock();
        let json: Option<String> = conn
            .query_row(
                "SELECT value_json FROM application_settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        match json {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    pub fn save_named_setting<T: serde::Serialize>(&self, key: &str, value: &T) -> AppResult<()> {
        let conn = self.conn.lock();
        let json = serde_json::to_string(value)?;
        conn.execute(
            "INSERT INTO application_settings (key, value_json, schema_version)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,
                 schema_version=excluded.schema_version",
            params![key, json, SCHEMA_VERSION],
        )?;
        Ok(())
    }

    pub fn save_settings(&self, settings: &AppSettings) -> AppResult<()> {
        settings.validate()?;
        let conn = self.conn.lock();
        let json = serde_json::to_string(settings)?;
        conn.execute(
            "INSERT INTO application_settings (key, value_json, schema_version)
             VALUES ('app.settings', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,
                 schema_version=excluded.schema_version",
            params![json, SCHEMA_VERSION],
        )?;
        Ok(())
    }

    // ---- Log index -------------------------------------------------------

    pub fn record_log_start(
        &self,
        id: &str,
        session_id: Option<&str>,
        label: Option<&str>,
        file_path: &str,
    ) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO log_index (id, session_id, label, file_path, started_at, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![id, session_id, label, file_path, now_iso()],
        )?;
        Ok(())
    }

    pub fn record_log_end(&self, id: &str, size_bytes: i64) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE log_index SET ended_at = ?1, size_bytes = ?2 WHERE id = ?3",
            params![now_iso(), size_bytes, id],
        )?;
        Ok(())
    }

    pub fn list_logs(&self) -> AppResult<Vec<LogEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, label, file_path, started_at, ended_at, size_bytes
             FROM log_index ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(LogEntry {
                id: r.get(0)?,
                session_id: r.get(1)?,
                label: r.get(2)?,
                file_path: r.get(3)?,
                started_at: r.get(4)?,
                ended_at: r.get(5)?,
                size_bytes: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // ---- Recent connections ---------------------------------------------

    pub fn touch_recent(&self, session_id: Option<&str>, label: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO recent_connections (id, session_id, safe_host_label, last_used_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, session_id, label, now_iso()],
        )?;
        // Keep the table bounded; recent history is a convenience, not a log.
        conn.execute(
            "DELETE FROM recent_connections WHERE id NOT IN (
                 SELECT id FROM recent_connections ORDER BY last_used_at DESC LIMIT 50
             )",
            [],
        )?;
        Ok(())
    }

    pub fn list_recent(&self) -> AppResult<Vec<RecentConnection>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, safe_host_label, last_used_at
             FROM recent_connections ORDER BY last_used_at DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(RecentConnection {
                id: r.get(0)?,
                session_id: r.get(1)?,
                safe_host_label: r.get(2)?,
                last_used_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn map_folder(r: &Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get(2)?,
        sort_order: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

fn parse_tags(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn map_session(r: &Row<'_>) -> rusqlite::Result<SavedSession> {
    let tags_json: String = r.get(10)?;
    let override_json: Option<String> = r.get(13)?;
    Ok(SavedSession {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        name: r.get(2)?,
        host: r.get(3)?,
        port: r.get(4)?,
        protocol: r.get(5)?,
        username: r.get(6)?,
        ssh_alias: r.get(7)?,
        identity_file: r.get(8)?,
        proxy_jump: r.get(9)?,
        tags: parse_tags(&tags_json),
        description: r.get(11)?,
        notes: r.get(12)?,
        settings_override: override_json.and_then(|j| serde_json::from_str(&j).ok()),
        credential_ref: r.get(14)?,
        is_favorite: r.get(15)?,
        sort_order: r.get(16)?,
        created_at: r.get(17)?,
        updated_at: r.get(18)?,
    })
}

fn map_command(r: &Row<'_>) -> rusqlite::Result<Command> {
    let vars_json: String = r.get(7)?;
    let tags_json: String = r.get(8)?;
    Ok(Command {
        id: r.get(0)?,
        group_id: r.get(1)?,
        name: r.get(2)?,
        content: r.get(3)?,
        vendor_profile: r.get(4)?,
        execute_mode: r.get(5)?,
        risk_level: r.get(6)?,
        variables: serde_json::from_str(&vars_json).unwrap_or_else(|_| serde_json::json!({})),
        tags: parse_tags(&tags_json),
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

fn map_workspace(r: &Row<'_>) -> rusqlite::Result<Workspace> {
    let layout_json: String = r.get(3)?;
    Ok(Workspace {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        layout: serde_json::from_str(&layout_json).unwrap_or_else(|_| serde_json::json!({})),
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}
