//! Credential storage via the OS keyring (FR-19).
//!
//! Secrets live in durable Secret Service storage; the app database stores names
//! (references), never secret values. There is deliberately no plaintext
//! fallback. Secrets are resolved for explicit connection requests or reveal.

use crate::error::{AppError, AppResult};
use crate::storage::Database;
use crate::vault::{self, Vault};

/// Keyring service namespace for OpsTerm secrets.
const SERVICE: &str = "org.opsterm.app";

fn entry(name: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, name)
        .map_err(|e| AppError::Internal(format!("keyring unavailable: {e}")))
}

#[cfg(target_os = "linux")]
fn legacy_entry(name: &str) -> keyring::Result<keyring::Entry> {
    keyring::keyutils::KeyutilsCredential::new_with_target(None, SERVICE, name)
        .map(|credential| keyring::Entry::new_with_credential(Box::new(credential)))
}

/// Store (or replace) a named secret in the OS keyring.
pub fn set(name: &str, secret: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation(
            "credential name must not be empty".into(),
        ));
    }
    entry(name)?
        .set_password(secret)
        .map_err(|e| AppError::Internal(format!("keyring set failed: {e}")))?;
    // Compatibility for older clients. Durable storage has already succeeded;
    // a kernel-cache failure must not invalidate that successful save.
    #[cfg(target_os = "linux")]
    if let Ok(legacy) = legacy_entry(name) {
        let _ = legacy.set_password(secret);
    }
    Ok(())
}

/// Read a named secret for a user-requested connection or explicit reveal.
pub fn get(name: &str) -> AppResult<String> {
    let durable = entry(name)?;
    match durable.get_password() {
        Ok(secret) => Ok(secret),
        #[cfg(target_os = "linux")]
        Err(keyring::Error::NoEntry) => {
            let secret = legacy_entry(name)
                .and_then(|legacy| legacy.get_password())
                .map_err(|e| AppError::Internal(format!("keyring get failed: {e}")))?;
            // Upgrade an existing pre-reboot credential automatically. Never
            // overwrite an existing durable entry with a stale kernel value.
            durable
                .set_password(&secret)
                .map_err(|e| AppError::Internal(format!("credential migration failed: {e}")))?;
            Ok(secret)
        }
        Err(e) => Err(AppError::Internal(format!("keyring get failed: {e}"))),
    }
}

/// Delete a named secret.
pub fn delete(name: &str) -> AppResult<()> {
    // Remove the legacy copy first so a durable deletion cannot resurrect it.
    #[cfg(target_os = "linux")]
    match legacy_entry(name).and_then(|legacy| legacy.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => {
            return Err(AppError::Internal(format!(
                "legacy keyring delete failed: {e}"
            )))
        }
    }
    match entry(name)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Internal(format!("keyring delete failed: {e}"))),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn default_storage_survives_a_system_reboot() {
        use keyring::credential::CredentialPersistence;
        assert!(
            matches!(
                keyring::default::default_credential_builder().persistence(),
                CredentialPersistence::UntilDelete
            ),
            "saved credentials must use durable storage, not the kernel cache"
        );
    }
}

/// Resolve a credential from the encrypted vault when it exists there, and
/// otherwise from the legacy OS keyring entry.
pub fn resolve(name: &str) -> AppResult<String> {
    let path = vault::default_path();
    if vault::is_initialized_at(&path) && vault::has_secret_at(&path, name) {
        let vault = Vault::resume(&path)?.ok_or(AppError::VaultLocked)?;
        return vault
            .get_secret(name)
            .ok_or_else(|| AppError::NotFound(name.to_string()));
    }
    get(name)
}

/// Store a secret in the vault when one is configured, otherwise in the OS
/// keyring. Any legacy keyring copy is removed after a successful vault write.
pub fn store(name: &str, secret: &str) -> AppResult<()> {
    let path = vault::default_path();
    if vault::is_initialized_at(&path) {
        let mut vault = Vault::resume(&path)?.ok_or(AppError::VaultLocked)?;
        vault.set_secret(name, secret)?;
        let _ = delete(name);
        Ok(())
    } else {
        set(name, secret)
    }
}

/// Remove a credential from both the vault and the legacy keyring.
pub fn remove(name: &str) -> AppResult<()> {
    let path = vault::default_path();
    if vault::is_initialized_at(&path) {
        let _ = vault::remove_secret_at(&path, name);
    }
    delete(name)
}

/// Pick the credential name for a device password. An existing reference is
/// reused; otherwise a readable, unique name is derived from the device and
/// host so the device form can store a password without opening Credentials.
pub fn device_credential_name(
    db: &Database,
    device_name: &str,
    host: &str,
    existing: Option<&str>,
) -> String {
    if let Some(name) = existing.map(str::trim).filter(|name| !name.is_empty()) {
        return name.to_string();
    }
    let device = device_name.trim();
    let host = host.trim();
    let base = match (device.is_empty(), host.is_empty()) {
        (false, false) => format!("{device} ({host})"),
        (false, true) => device.to_string(),
        (true, false) => host.to_string(),
        (true, true) => "device".to_string(),
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while db.get_credential(&candidate).ok().flatten().is_some() {
        candidate = format!("{base} {suffix}");
        suffix += 1;
    }
    candidate
}

/// Store a device login password and its metadata. The secret follows the
/// normal rule: encrypted vault when one exists, otherwise the OS keyring.
/// Existing tags and kind are preserved when the credential already exists.
pub fn store_device_password(
    db: &Database,
    name: &str,
    username: Option<&str>,
    secret: &str,
) -> AppResult<()> {
    store(name, secret)?;
    let existing = db.get_credential(name)?;
    let kind = existing
        .as_ref()
        .map(|record| record.kind.clone())
        .unwrap_or_else(|| "password".to_string());
    let tags = existing
        .as_ref()
        .map(|record| record.tags.clone())
        .unwrap_or_default();
    let username = username
        .map(str::trim)
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .or_else(|| existing.and_then(|record| record.username));
    db.upsert_credential(name, username.as_deref(), &kind, &tags)
}

/// Copy named keyring secrets into the vault and delete each keyring copy
/// after a successful write. Returns `(imported, missing_or_failed)`.
pub fn import_keyring_into(vault: &mut Vault, names: &[String]) -> (usize, usize) {
    let mut imported = 0;
    let mut failed = 0;
    for name in names {
        if vault.has_secret(name) {
            continue;
        }
        match get(name) {
            Ok(secret) => {
                if vault.set_secret(name, &secret).is_ok() {
                    let _ = delete(name);
                    imported += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => failed += 1,
        }
    }
    (imported, failed)
}
