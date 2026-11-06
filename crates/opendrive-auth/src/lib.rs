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
    #[error("timeout waiting for redirect")]
    Timeout,
    #[error("state mismatch")]
    StateMismatch,
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

    /// Authorization-code + PKCE (S256) + loopback redirect.
    /// Opens system browser; listens on 127.0.0.1 ephemeral port.
    pub async fn authorize(&self, read_only: bool) -> Result<(String, String)> {
        let _scope = if read_only {
            "https://www.googleapis.com/auth/drive.readonly"
        } else {
            "https://www.googleapis.com/auth/drive"
        };
        // TODO: generate PKCE verifier/challenge, state, start tokio TcpListener,
        // open browser to accounts.google.com/o/oauth2/v2/auth, exchange code.
        Err(AuthError::OAuth("not implemented".into()))
    }

    pub async fn refresh(&self, _refresh_token: &str) -> Result<String> {
        // TODO: POST token endpoint with client_id/secret + refresh_token
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
