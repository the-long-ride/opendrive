//! OAuth loopback + PKCE, token store (keyring).

mod loopback;
mod pkce;

pub use loopback::{bind_loopback, wait_for_redirect, LoopbackError, RedirectParams};
pub use pkce::{generate_pkce, random_state, PkceError, PkcePair};

use thiserror::Error;
use url::Url;
use uuid::Uuid;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

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
    #[error(transparent)]
    Pkce(#[from] PkceError),
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

    pub fn authorization_url(
        &self,
        redirect_uri: &str,
        challenge: &str,
        state: &str,
        read_only: bool,
    ) -> String {
        let scope = if read_only {
            "https://www.googleapis.com/auth/drive.readonly"
        } else {
            "https://www.googleapis.com/auth/drive"
        };
        let mut u = Url::parse(AUTH_URL).expect("static");
        u.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", scope)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);
        u.to_string()
    }

    /// Full browser flow: bind loopback → open URL (caller) → wait → exchange.
    /// Returns (email placeholder, refresh_token). Email filled after about() later.
    pub async fn authorize(&self, read_only: bool) -> Result<(String, String)> {
        let pkce = generate_pkce()?;
        let state = random_state();
        let (listener, redirect_uri, _) = bind_loopback().await?;
        let _auth_url = self.authorization_url(&redirect_uri, &pkce.challenge, &state, read_only);
        // Caller / UI should open _auth_url in system browser.
        // For headless tests this will timeout unless a client hits the redirect.
        let redirect = wait_for_redirect(listener, &state, 180).await?;
        self.exchange_code(&redirect.code, &pkce.verifier, &redirect_uri)
            .await
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<(String, String)> {
        // TODO: HTTP POST TOKEN_URL with reqwest when wired
        let _ = (code, verifier, redirect_uri, TOKEN_URL);
        Err(AuthError::OAuth(
            "token exchange not implemented (need HTTP client)".into(),
        ))
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
