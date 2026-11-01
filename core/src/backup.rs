//! Password-protected backup documents.
//!
//! An export is a single JSON envelope whose payload is encrypted with
//! AES-256-GCM. The encryption key is derived from the user's backup password
//! with Argon2id. The payload contains inventory metadata and, optionally,
//! credential secrets; the envelope never stores the password or plaintext.
//!
//! This is deliberately separate from the vault file format so a backup can be
//! written with its own password and shared without exposing the master
//! password used to unlock the live vault.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

/// File extension recommended for encrypted OpsTerm backups.
pub const FILE_EXTENSION: &str = "opsterm-backup";
/// Current envelope version. Bump when the plaintext schema changes.
pub const BACKUP_VERSION: u32 = 1;
const FORMAT: &str = "opsterm-backup";

const M_COST: u32 = 19_456;
const T_COST: u32 = 2;
const P_COST: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KdfParams {
    algorithm: String,
    salt: String,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CipherData {
    algorithm: String,
    nonce: String,
    ciphertext: String,
}

/// The on-disk JSON envelope. Metadata outside the ciphertext is authenticated
/// as associated data, so editing it makes decryption fail.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub format: String,
    pub version: u32,
    pub created_at: String,
    pub contains_secrets: bool,
    kdf: KdfParams,
    cipher: CipherData,
}

/// Encrypt `plaintext` and return a self-describing JSON document.
///
/// `contains_secrets` is recorded in the envelope so a restore UI can warn
/// before handling the payload. The password must be at least 8 characters.
pub fn encrypt(plaintext: &[u8], password: &str, contains_secrets: bool) -> AppResult<String> {
    validate_password(password)?;
    let salt = random_bytes::<16>();
    let key = derive_key(password, &salt, M_COST, T_COST, P_COST)?;
    let nonce = random_bytes::<12>();
    let aad = aad(contains_secrets);
    let ciphertext = cipher(&key)?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| AppError::Internal("backup encryption failed".into()))?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: BACKUP_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        contains_secrets,
        kdf: KdfParams {
            algorithm: "argon2id".into(),
            salt: STANDARD.encode(salt),
            m_cost: M_COST,
            t_cost: T_COST,
            p_cost: P_COST,
        },
        cipher: CipherData {
            algorithm: "aes-256-gcm".into(),
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        },
    };
    serde_json::to_string_pretty(&envelope).map_err(AppError::from)
}

/// Serialize `value`, encrypt it, and clear the plaintext buffer afterwards.
///
/// Prefer this over [`encrypt`] for backup payloads that may contain secrets.
pub fn encrypt_json<T: Serialize>(
    value: &T,
    password: &str,
    contains_secrets: bool,
) -> AppResult<String> {
    let plaintext = Zeroizing::new(serde_json::to_vec_pretty(value).map_err(AppError::from)?);
    encrypt(&plaintext, password, contains_secrets)
}

/// Decrypt a document produced by [`encrypt`].
pub fn decrypt(document: &str, password: &str) -> AppResult<Vec<u8>> {
    let envelope: Envelope = serde_json::from_str(document).map_err(AppError::from)?;
    if envelope.format != FORMAT {
        return Err(AppError::Validation("not an OpsTerm backup".into()));
    }
    if envelope.version != BACKUP_VERSION {
        return Err(AppError::Validation(format!(
            "unsupported backup version {}",
            envelope.version
        )));
    }
    if !envelope.kdf.algorithm.eq_ignore_ascii_case("argon2id")
        || !envelope
            .cipher
            .algorithm
            .eq_ignore_ascii_case("aes-256-gcm")
    {
        return Err(AppError::Validation(
            "unsupported backup encryption parameters".into(),
        ));
    }
    let salt = decode(&envelope.kdf.salt)?;
    let nonce = decode(&envelope.cipher.nonce)?;
    let ciphertext = decode(&envelope.cipher.ciphertext)?;
    if nonce.len() != 12 {
        return Err(AppError::Validation("invalid backup nonce".into()));
    }
    let key = derive_key(
        password,
        &salt,
        envelope.kdf.m_cost,
        envelope.kdf.t_cost,
        envelope.kdf.p_cost,
    )?;
    cipher(&key)?
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad(envelope.contains_secrets),
            },
        )
        .map_err(|_| AppError::Validation("wrong password or corrupted backup".into()))
}

fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < 8 {
        return Err(AppError::Validation(
            "backup password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

fn aad(contains_secrets: bool) -> Vec<u8> {
    format!("{FORMAT}:{BACKUP_VERSION}:{contains_secrets}").into_bytes()
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
        .map_err(|e| AppError::Internal(format!("backup KDF parameters: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key[..])
        .map_err(|e| AppError::Internal(format!("backup KDF failed: {e}")))?;
    Ok(key)
}

fn cipher(key: &[u8; 32]) -> AppResult<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::Internal("backup cipher init failed".into()))
}

fn decode(value: &str) -> AppResult<Vec<u8>> {
    STANDARD
        .decode(value)
        .map_err(|e| AppError::Validation(format!("invalid backup data: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_backup() {
        let document = encrypt(b"{\"devices\":[]}", "correct horse battery", true).unwrap();
        assert!(!document.contains("devices"));
        let plaintext = decrypt(&document, "correct horse battery").unwrap();
        assert_eq!(plaintext, b"{\"devices\":[]}");
    }

    #[test]
    fn serializes_and_encrypts_structured_payloads() {
        let document = encrypt_json(
            &serde_json::json!({"secret": "s3cret"}),
            "correct horse battery",
            true,
        )
        .unwrap();
        assert!(!document.contains("s3cret"));
        let plaintext = decrypt(&document, "correct horse battery").unwrap();
        let value: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(value["secret"], "s3cret");
    }

    #[test]
    fn rejects_a_wrong_password() {
        let document = encrypt(b"secret", "correct horse battery", true).unwrap();
        let error = decrypt(&document, "wrong password").unwrap_err();
        assert!(error.to_string().contains("wrong password"));
    }

    #[test]
    fn rejects_tampered_metadata() {
        let document = encrypt(b"secret", "correct horse battery", false).unwrap();
        let tampered = document.replace("\"containsSecrets\": false", "\"containsSecrets\": true");
        assert!(decrypt(&tampered, "correct horse battery").is_err());
    }

    #[test]
    fn rejects_weak_passwords_and_foreign_documents() {
        assert!(encrypt(b"payload", "short", false).is_err());
        assert!(decrypt("{\"format\":\"something-else\"}", "correct horse battery").is_err());
    }
}
