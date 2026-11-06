//! Password-encrypted cross-machine credential & account backup (.odcred) using PBKDF2-HMAC-SHA256 + AES-256-GCM.

use crate::{AuthError, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::pbkdf2::{derive, PBKDF2_HMAC_SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

const VAULT_VERSION: u32 = 1;
const VAULT_KDF_NAME: &str = "pbkdf2-hmac-sha256";
const VAULT_CIPHER_NAME: &str = "aes-256-gcm";
const VAULT_KDF_ITERATIONS: u32 = 600_000;
const VAULT_AAD: &[u8] = b"opendrive-vault-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportedCredentialSet {
    pub label: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportedAccount {
    pub nickname: String,
    pub email: String,
    pub folder_name: String,
    pub read_only: bool,
    pub enabled: bool,
    pub credential_client_id: Option<String>,
    pub refresh_token: Option<String>,
    pub service_account_json: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultPayload {
    pub exported_at: String,
    pub credentials: Vec<ExportedCredentialSet>,
    pub accounts: Vec<ExportedAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedVaultFile {
    pub version: u32,
    pub kdf: String,
    pub iterations: u32,
    pub cipher: String,
    pub salt_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

pub fn validate_vault_password(password: &str) -> std::result::Result<(), &'static str> {
    if password.chars().count() <= 8 {
        return Err("Password must be more than 8 characters (9+ characters).");
    }
    if !password.chars().any(char::is_uppercase) {
        return Err("Password must include at least one uppercase letter.");
    }
    if !password.chars().any(char::is_lowercase) {
        return Err("Password must include at least one lowercase letter.");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        return Err("Password must include at least one number.");
    }
    if !password
        .chars()
        .any(|c| !c.is_alphanumeric() && !c.is_whitespace())
    {
        return Err("Password must include at least one symbol.");
    }
    Ok(())
}

fn vault_err<T, E>(res: std::result::Result<T, E>, msg: &str) -> Result<T> {
    res.map_err(|_| AuthError::Vault(msg.into()))
}

fn derive_key(password: &str, salt: &[u8], iterations: u32) -> Result<[u8; 32]> {
    let iters = NonZeroU32::new(iterations)
        .ok_or_else(|| AuthError::Vault("invalid KDF iteration count".into()))?;
    let mut key = [0u8; 32];
    derive(
        PBKDF2_HMAC_SHA256,
        iters,
        salt,
        password.as_bytes(),
        &mut key,
    );
    Ok(key)
}

pub fn encrypt_vault(payload: &VaultPayload, password: &str) -> Result<Vec<u8>> {
    if let Err(msg) = validate_vault_password(password) {
        return Err(AuthError::Vault(msg.into()));
    }
    let rng = SystemRandom::new();
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    vault_err(rng.fill(&mut salt), "failed to generate salt")?;
    vault_err(rng.fill(&mut nonce_bytes), "failed to generate nonce")?;

    let mut key = derive_key(password, &salt, VAULT_KDF_ITERATIONS)?;
    let unbound = vault_err(
        UnboundKey::new(&AES_256_GCM, &key),
        "cipher key setup failed",
    )?;
    key.fill(0);
    let sealing_key = LessSafeKey::new(unbound);

    let mut buf = vault_err(serde_json::to_vec(payload), "failed to encode payload")?;
    vault_err(
        sealing_key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(VAULT_AAD),
            &mut buf,
        ),
        "encryption failed",
    )?;

    let envelope = EncryptedVaultFile {
        version: VAULT_VERSION,
        kdf: VAULT_KDF_NAME.into(),
        iterations: VAULT_KDF_ITERATIONS,
        cipher: VAULT_CIPHER_NAME.into(),
        salt_b64: STANDARD.encode(salt),
        nonce_b64: STANDARD.encode(nonce_bytes),
        ciphertext_b64: STANDARD.encode(&buf),
    };
    vault_err(
        serde_json::to_vec_pretty(&envelope),
        "failed to encode envelope",
    )
}

pub fn decrypt_vault(bytes: &[u8], password: &str) -> Result<VaultPayload> {
    if password.is_empty() {
        return Err(AuthError::Vault("Enter the backup password.".into()));
    }
    let envelope: EncryptedVaultFile =
        vault_err(serde_json::from_slice(bytes), "Invalid backup file format.")?;
    if envelope.version != VAULT_VERSION
        || envelope.kdf != VAULT_KDF_NAME
        || envelope.cipher != VAULT_CIPHER_NAME
        || envelope.iterations < 100_000
    {
        return Err(AuthError::Vault(
            "Unsupported backup format version.".into(),
        ));
    }
    let salt = vault_err(
        STANDARD.decode(&envelope.salt_b64),
        "Corrupted salt in backup file.",
    )?;
    let nonce_vec = vault_err(
        STANDARD.decode(&envelope.nonce_b64),
        "Corrupted nonce in backup file.",
    )?;
    let nonce_bytes: [u8; 12] =
        vault_err(nonce_vec.try_into(), "Invalid nonce length in backup file.")?;
    let mut ciphertext = vault_err(
        STANDARD.decode(&envelope.ciphertext_b64),
        "Corrupted ciphertext in backup file.",
    )?;

    let mut key = derive_key(password, &salt, envelope.iterations)?;
    let unbound = vault_err(
        UnboundKey::new(&AES_256_GCM, &key),
        "cipher key setup failed",
    )?;
    key.fill(0);
    let opening_key = LessSafeKey::new(unbound);

    let plain = vault_err(
        opening_key.open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(VAULT_AAD),
            &mut ciphertext,
        ),
        "Incorrect password or corrupted credential backup.",
    )?;

    vault_err(
        serde_json::from_slice(plain),
        "Corrupted credential payload.",
    )
}
