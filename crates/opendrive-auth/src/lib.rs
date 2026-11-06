//! OAuth loopback + PKCE, token store (keyring).

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
}

pub type Result<T> = std::result::Result<T, AuthError>;

pub struct AuthClient {
    pub client_id: String,
}

impl AuthClient {
    pub fn new(client_id: String) -> Self {
        Self { client_id }
    }

    pub async fn authorize(&self, _read_only: bool) -> Result<(String, String)> {
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
