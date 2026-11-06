//! OAuth loopback + PKCE, token store (keyring).
//!
//! Flow (design §7.3):
//! 1. Generate PKCE verifier/challenge (S256) + state
//! 2. Listen on 127.0.0.1:<ephemeral>
//! 3. Open browser → accounts.google.com with scope, access_type=offline, prompt=consent
//! 4. Exchange code + verifier for tokens
//! 5. Store refresh_token in keychain; access_token in memory

mod loopback;

pub use loopback::{wait_for_redirect, LoopbackError, RedirectParams};

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("oauth failed: {0}")]
    OAuth(String),
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("invalid grant")]
    InvalidGrant,
    #[error("timeout waiting for redirect")]
    Timeout,
    #[error("state mismatch")]
    StateMismatch,
    #[error(transparent)]
    Loopback(#[from] LoopbackError),
}

pub type Result<T> = std::result::Result<T, AuthError>;

pub struct AuthClient {
    pub client_id: String,
    pub client_secret: String,
}

impl AuthClient {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }

    pub async fn authorize(&self, read_only: bool) -> Result<(String, String)> {
        let _scope = if read_only {
            "https://www.googleapis.com/auth/drive.readonly"
        } else {
            "https://www.googleapis.com/auth/drive"
        };
        // TODO: PKCE + wait_for_redirect + token exchange
        Err(AuthError::OAuth("not implemented".into()))
    }

    pub async fn refresh(&self, _refresh_token: &str) -> Result<String> {
        Err(AuthError::OAuth("not implemented".into()))
    }
}

pub fn store_refresh_token(_account_id: Uuid, _token: &str) -> Result<()> {
    Ok(())
}

pub fn load_refresh_token(_account_id: Uuid) -> Result<Option<String>> {
    Ok(None)
}

pub fn store_client_secret(_cred_id: Uuid, _secret: &str) -> Result<()> {
    Ok(())
}

pub fn load_client_secret(_cred_id: Uuid) -> Result<Option<String>> {
    Ok(None)
}
