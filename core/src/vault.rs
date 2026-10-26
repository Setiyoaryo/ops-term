//! Master-password credential vault.
//!
//! Secrets are encrypted with AES-256-GCM using a key derived from the master
//! password with Argon2id. Only the encrypted vault file is persisted; the
//! derived key is held in memory while unlocked and, for the configurable
//! unlock window, in the OS keyring so the app can resume without prompting.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

const VAULT_VERSION: u32 = 1;
const VERIFIER_AAD: &[u8] = b"opsterm.vault.verifier";
const VERIFIER_PLAINTEXT: &[u8] = b"opsterm-vault-v1";
const SESSION_SERVICE: &str = "org.opsterm.app.vault";
const SESSION_USER: &str = "session";

pub const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;
pub const TTL_CHOICES: [u64; 5] = [
    60 * 60,
    8 * 60 * 60,
    24 * 60 * 60,
    DEFAULT_TTL_SECS,
    30 * 24 * 60 * 60,
];

const M_COST: u32 = 19_456;
const T_COST: u32 = 2;
const P_COST: u32 = 1;

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    base.join("org.opsterm.app").join("vault.json")
}

pub fn is_initialized_at(path: &Path) -> bool {
    path.is_file()
}

pub fn is_initialized() -> bool {
    is_initialized_at(&default_path())
}

#[derive(Clone, Serialize, Deserialize)]
struct SecretEntry {
    nonce: String,
    ciphertext: String,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct VaultFile {
    version: u32,
    salt: String,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    verifier_nonce: String,
    verifier_ciphertext: String,
    #[serde(default)]
    unlock_until: Option<String>,
    #[serde(default = "default_ttl")]
    unlock_ttl_secs: u64,
    #[serde(default)]
    secrets: BTreeMap<String, SecretEntry>,
}

fn default_ttl() -> u64 {
    DEFAULT_TTL_SECS
}

pub struct Vault {
    path: PathBuf,
    key: Zeroizing<[u8; 32]>,
    file: VaultFile,
}

impl Vault {
    pub fn create(path: &Path, password: &str, ttl_secs: u64) -> AppResult<Self> {
        if path.is_file() {
            return Err(AppError::Validation("vault already exists".into()));
        }
        validate_password(password)?;
        let salt = random_bytes::<16>();
        let key = derive_key(password, &salt, M_COST, T_COST, P_COST)?;
        let (verifier_nonce, verifier_ciphertext) =
            encrypt(&key, VERIFIER_PLAINTEXT, VERIFIER_AAD)?;
        let file = VaultFile {
            version: VAULT_VERSION,
            salt: STANDARD.encode(salt),
            m_cost: M_COST,
            t_cost: T_COST,
            p_cost: P_COST,
            verifier_nonce: STANDARD.encode(verifier_nonce),
            verifier_ciphertext: STANDARD.encode(verifier_ciphertext),
            unlock_until: Some(expiry(ttl_secs)),
            unlock_ttl_secs: ttl_secs.max(60),
            secrets: BTreeMap::new(),
        };
        let vault = Self {
            path: path.to_path_buf(),
            key,
            file,
        };
        vault.save()?;
        persist_session(&vault.key);
        Ok(vault)
    }

    pub fn unlock(path: &Path, password: &str) -> AppResult<Self> {
        let mut file = load_file(path)?;
        if file.version != VAULT_VERSION {
            return Err(AppError::Validation("unsupported vault version".into()));
        }
        let salt = decode(&file.salt)?;
        let key = derive_key(password, &salt, file.m_cost, file.t_cost, file.p_cost)?;
        if !verify(&key, &file)? {
            return Err(AppError::Validation("wrong master password".into()));
        }
        file.unlock_until = Some(expiry(file.unlock_ttl_secs));
        let vault = Self {
            path: path.to_path_buf(),
            key,
            file,
        };
        vault.save()?;
        persist_session(&vault.key);
        Ok(vault)
    }

    pub fn resume(path: &Path) -> AppResult<Option<Self>> {
        if !path.is_file() {
            return Ok(None);
        }
        let mut file = load_file(path)?;
        if !unlock_valid(&file) {
            clear_session();
            if file.unlock_until.is_some() {
                file.unlock_until = None;
                save_file(path, &file)?;
            }
            return Ok(None);
        }
        let Some(key) = load_session() else {
            return Ok(None);
        };
        if !verify(&key, &file).unwrap_or(false) {
            clear_session();
            return Ok(None);
        }
        Ok(Some(Self {
            path: path.to_path_buf(),
            key,
            file,
        }))
    }

    pub fn lock(&mut self) {
        self.file.unlock_until = None;
        let _ = self.save();
        clear_session();
    }

    pub fn is_expired(&self) -> bool {
        !unlock_valid(&self.file)
    }

    pub fn unlock_until(&self) -> Option<DateTime<Utc>> {
        self.file.unlock_until.as_deref().and_then(parse_time)
    }

    pub fn ttl_secs(&self) -> u64 {
        self.file.unlock_ttl_secs
    }

    pub fn set_ttl(&mut self, ttl_secs: u64) {
        self.file.unlock_ttl_secs = ttl_secs.max(60);
        self.file.unlock_until = Some(expiry(self.file.unlock_ttl_secs));
        let _ = self.save();
    }

    pub fn has_secret(&self, name: &str) -> bool {
        self.file.secrets.contains_key(name)
    }

    pub fn secret_names(&self) -> Vec<String> {
        self.file.secrets.keys().cloned().collect()
    }

    pub fn set_secret(&mut self, name: &str, secret: &str) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::Validation(
                "credential name must not be empty".into(),
            ));
        }
        let plaintext = Zeroizing::new(secret.as_bytes().to_vec());
        let (nonce, ciphertext) = encrypt(&self.key, &plaintext, name.as_bytes())?;
        self.file.secrets.insert(
            name.to_string(),
            SecretEntry {
                nonce: STANDARD.encode(nonce),
                ciphertext: STANDARD.encode(ciphertext),
                updated_at: Utc::now().to_rfc3339(),
            },
        );
        self.save()
    }

    pub fn get_secret(&self, name: &str) -> Option<String> {
        let entry = self.file.secrets.get(name)?;
        let nonce = decode(&entry.nonce).ok()?;
        let ciphertext = decode(&entry.ciphertext).ok()?;
        let plaintext = decrypt(&self.key, &nonce, &ciphertext, name.as_bytes()).ok()?;
        String::from_utf8(plaintext).ok()
    }

    pub fn delete_secret(&mut self, name: &str) -> AppResult<()> {
        self.file.secrets.remove(name);
        self.save()
    }

    pub fn change_password(&mut self, password: &str) -> AppResult<()> {
        validate_password(password)?;
        let salt = random_bytes::<16>();
        let key = derive_key(password, &salt, M_COST, T_COST, P_COST)?;
        let (verifier_nonce, verifier_ciphertext) =
            encrypt(&key, VERIFIER_PLAINTEXT, VERIFIER_AAD)?;

        let mut secrets = BTreeMap::new();
        for (name, entry) in &self.file.secrets {
            let nonce = decode(&entry.nonce)?;
            let ciphertext = decode(&entry.ciphertext)?;
            let plaintext = decrypt(&self.key, &nonce, &ciphertext, name.as_bytes())?;
            let (nonce, ciphertext) = encrypt(&key, &plaintext, name.as_bytes())?;
            secrets.insert(
                name.clone(),
                SecretEntry {
                    nonce: STANDARD.encode(nonce),
                    ciphertext: STANDARD.encode(ciphertext),
                    updated_at: entry.updated_at.clone(),
                },
            );
        }
        self.key = key;
        self.file.salt = STANDARD.encode(salt);
        self.file.m_cost = M_COST;
        self.file.t_cost = T_COST;
        self.file.p_cost = P_COST;
        self.file.verifier_nonce = STANDARD.encode(verifier_nonce);
        self.file.verifier_ciphertext = STANDARD.encode(verifier_ciphertext);
        self.file.secrets = secrets;
        self.save()?;
        persist_session(&self.key);
        Ok(())
    }

    fn save(&self) -> AppResult<()> {
        save_file(&self.path, &self.file)
    }
}

pub fn has_secret_at(path: &Path, name: &str) -> bool {
    load_file(path)
        .map(|file| file.secrets.contains_key(name))
        .unwrap_or(false)
}

pub fn is_locked_for(name: &str) -> bool {
    is_locked_for_at(&default_path(), name)
}

pub fn is_locked_for_at(path: &Path, name: &str) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(file) = load_file(path) else {
        return false;
    };
    if !file.secrets.contains_key(name) {
        return false;
    }
    if unlock_valid(&file) {
        if let Some(key) = load_session() {
            if verify(&key, &file).unwrap_or(false) {
                return false;
            }
        }
    }
    true
}

pub fn remove_secret_at(path: &Path, name: &str) -> AppResult<()> {
    let mut file = load_file(path)?;
    file.secrets.remove(name);
    save_file(path, &file)
}

pub fn secret_names_at(path: &Path) -> Vec<String> {
    load_file(path)
        .map(|file| file.secrets.keys().cloned().collect())
        .unwrap_or_default()
}

fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < 8 {
        return Err(AppError::Validation(
            "master password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn derive_key(
    password: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> AppResult<Zeroizing<[u8; 32]>> {
    let m_cost = m_cost.clamp(8_192, 262_144);
    let t_cost = t_cost.clamp(1, 10);
    let p_cost = p_cost.clamp(1, 4);
    let params = Params::new(m_cost, t_cost, p_cost, Some(32))
        .map_err(|e| AppError::Internal(format!("vault KDF parameters: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key[..])
        .map_err(|e| AppError::Internal(format!("vault KDF failed: {e}")))?;
    Ok(key)
}

fn cipher(key: &[u8; 32]) -> AppResult<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::Internal("vault cipher init failed".into()))
}

fn encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let nonce = random_bytes::<12>();
    let ciphertext = cipher(key)?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::Internal("vault encryption failed".into()))?;
    Ok((nonce.to_vec(), ciphertext))
}

fn decrypt(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> AppResult<Vec<u8>> {
    if nonce.len() != 12 {
        return Err(AppError::Internal("invalid vault nonce".into()));
    }
    cipher(key)?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AppError::Internal("vault decryption failed".into()))
}

fn verify(key: &[u8; 32], file: &VaultFile) -> AppResult<bool> {
    let nonce = decode(&file.verifier_nonce)?;
    let ciphertext = decode(&file.verifier_ciphertext)?;
    match decrypt(key, &nonce, &ciphertext, VERIFIER_AAD) {
        Ok(plaintext) => Ok(plaintext == VERIFIER_PLAINTEXT),
        Err(_) => Ok(false),
    }
}

fn decode(value: &str) -> AppResult<Vec<u8>> {
    STANDARD
        .decode(value)
        .map_err(|e| AppError::Internal(format!("invalid vault data: {e}")))
}

fn expiry(ttl_secs: u64) -> String {
    (Utc::now() + Duration::seconds(ttl_secs as i64)).to_rfc3339()
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn unlock_valid(file: &VaultFile) -> bool {
    file.unlock_until
        .as_deref()
        .and_then(parse_time)
        .is_some_and(|until| until > Utc::now())
}

fn load_file(path: &Path) -> AppResult<VaultFile> {
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(AppError::from)
}

fn save_file(path: &Path, file: &VaultFile) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut handle = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        handle.write_all(&serde_json::to_vec_pretty(file)?)?;
        handle.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn session_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SESSION_SERVICE, SESSION_USER).ok()
}

fn persist_session(key: &[u8; 32]) {
    if let Some(entry) = session_entry() {
        let _ = entry.set_password(&STANDARD.encode(key));
    }
}

fn load_session() -> Option<Zeroizing<[u8; 32]>> {
    let encoded = session_entry()?.get_password().ok()?;
    let bytes = STANDARD.decode(encoded).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Some(Zeroizing::new(key))
}

fn clear_session() {
    if let Some(entry) = session_entry() {
        let _ = entry.delete_credential();
    }
}

pub fn import_secrets(
    vault: &mut Vault,
    entries: impl IntoIterator<Item = (String, String)>,
) -> usize {
    let mut imported = 0;
    for (name, secret) in entries {
        if vault.has_secret(&name) {
            continue;
        }
        if vault.set_secret(&name, &secret).is_ok() {
            imported += 1;
        }
    }
    imported
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault() -> (PathBuf, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        (dir.path().join("vault.json"), dir)
    }

    #[test]
    fn round_trips_secrets_and_rejects_wrong_password() {
        let (path, _dir) = temp_vault();
        let mut vault = Vault::create(&path, "correct horse battery", 3600).unwrap();
        vault.set_secret("core-ssh", "s3cret").unwrap();
        assert_eq!(vault.get_secret("core-ssh").as_deref(), Some("s3cret"));
        drop(vault);

        assert!(Vault::unlock(&path, "wrong password").is_err());
        let vault = Vault::unlock(&path, "correct horse battery").unwrap();
        assert_eq!(vault.get_secret("core-ssh").as_deref(), Some("s3cret"));
    }

    #[test]
    fn password_change_reencrypts_every_secret() {
        let (path, _dir) = temp_vault();
        let mut vault = Vault::create(&path, "old master password", 3600).unwrap();
        vault.set_secret("a", "alpha").unwrap();
        vault.set_secret("b", "beta").unwrap();
        vault.change_password("new master password").unwrap();
        drop(vault);

        assert!(Vault::unlock(&path, "old master password").is_err());
        let vault = Vault::unlock(&path, "new master password").unwrap();
        assert_eq!(vault.get_secret("a").as_deref(), Some("alpha"));
        assert_eq!(vault.get_secret("b").as_deref(), Some("beta"));
    }

    #[test]
    fn secrets_are_bound_to_their_names() {
        let (path, _dir) = temp_vault();
        let mut vault = Vault::create(&path, "correct horse battery", 3600).unwrap();
        vault.set_secret("a", "alpha").unwrap();
        vault.set_secret("b", "beta").unwrap();
        let name_a = vault.file.secrets["a"].clone();
        let name_b = vault.file.secrets["b"].clone();
        vault.file.secrets.insert("a".into(), name_b);
        vault.file.secrets.insert("b".into(), name_a);
        assert!(vault.get_secret("a").is_none());
        assert!(vault.get_secret("b").is_none());
    }

    #[test]
    fn expiry_blocks_auto_resume() {
        let (path, _dir) = temp_vault();
        let mut vault = Vault::create(&path, "correct horse battery", 3600).unwrap();
        vault.file.unlock_until = Some((Utc::now() - Duration::seconds(1)).to_rfc3339());
        vault.save().unwrap();
        drop(vault);
        assert!(Vault::resume(&path).unwrap().is_none());
    }

    #[test]
    fn resumes_from_the_keyring_session_when_available() {
        let (path, _dir) = temp_vault();
        let mut vault = Vault::create(&path, "correct horse battery", 3600).unwrap();
        vault.set_secret("k", "v").unwrap();
        if load_session().is_some() {
            drop(vault);
            let resumed = Vault::resume(&path).unwrap().expect("keyring session");
            assert_eq!(resumed.get_secret("k").as_deref(), Some("v"));
            assert!(!is_locked_for_at(&path, "k"));
            let mut resumed = resumed;
            resumed.lock();
            assert!(is_locked_for_at(&path, "k"));
        }
    }

    #[test]
    fn import_skips_existing_entries() {
        let (path, _dir) = temp_vault();
        let mut vault = Vault::create(&path, "correct horse battery", 3600).unwrap();
        vault.set_secret("keep", "original").unwrap();
        let imported = import_secrets(
            &mut vault,
            [
                ("keep".to_string(), "replacement".to_string()),
                ("new".to_string(), "value".to_string()),
            ],
        );
        assert_eq!(imported, 1);
        assert_eq!(vault.get_secret("keep").as_deref(), Some("original"));
        assert_eq!(vault.get_secret("new").as_deref(), Some("value"));
    }
}
