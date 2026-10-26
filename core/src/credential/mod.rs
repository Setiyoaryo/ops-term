//! Credential storage via the OS keyring (FR-19).
//!
//! Secrets live in durable Secret Service storage; the app database stores names
//! (references), never secret values. There is deliberately no plaintext
//! fallback. Secrets are resolved for explicit connection requests or reveal.

use crate::error::{AppError, AppResult};
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
