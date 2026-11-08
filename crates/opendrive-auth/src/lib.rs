//! OAuth loopback + PKCE, token store (keyring).

mod loopback;
mod pkce;

pub use loopback::{bind_loopback, wait_for_redirect, LoopbackError, RedirectParams};
pub use pkce::{generate_pkce, random_state, PkceError, PkcePair};

use serde::Deserialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SERVICE: &str = "opendrive";

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

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
}

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

    /// Browser flow: bind → open browser → wait → exchange.
    /// Returns (access_token, refresh_token).
    pub async fn authorize(&self, read_only: bool) -> Result<(String, String)> {
        let pkce = generate_pkce()?;
        let state = random_state();
        let (listener, redirect_uri, _) = bind_loopback().await?;
        let auth_url = self.authorization_url(&redirect_uri, &pkce.challenge, &state, read_only);
        open::that(&auth_url).map_err(|e| AuthError::OAuth(format!("open browser: {e}")))?;
        tracing::info!(%auth_url, "opened browser for sign-in");
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
        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .map_err(|e| AuthError::OAuth(e.to_string()))?;

        let body: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::OAuth(e.to_string()))?;

        if body.error.as_deref() == Some("invalid_grant") {
            return Err(AuthError::InvalidGrant);
        }
        if let Some(err) = body.error {
            return Err(AuthError::OAuth(err));
        }
        let refresh = body
            .refresh_token
            .ok_or_else(|| AuthError::OAuth("no refresh_token (check prompt=consent)".into()))?;
        let _ = body.expires_in;
        Ok((body.access_token, refresh))
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<String> {
        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|e| AuthError::OAuth(e.to_string()))?;

        let body: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::OAuth(e.to_string()))?;

        if body.error.as_deref() == Some("invalid_grant") {
            return Err(AuthError::InvalidGrant);
        }
        if let Some(err) = body.error {
            return Err(AuthError::OAuth(err));
        }
        Ok(body.access_token)
    }
}

fn entry(user: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, user).map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn store_refresh_token(account_id: Uuid, token: &str) -> Result<()> {
    entry(&format!("acct/{account_id}/refresh_token"))?
        .set_password(token)
        .map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn load_refresh_token(account_id: Uuid) -> Result<Option<String>> {
    match entry(&format!("acct/{account_id}/refresh_token"))?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}

pub fn delete_refresh_token(account_id: Uuid) -> Result<()> {
    match entry(&format!("acct/{account_id}/refresh_token"))?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}

pub fn store_client_secret(cred_id: Uuid, secret: &str) -> Result<()> {
    entry(&format!("cred/{cred_id}/secret"))?
        .set_password(secret)
        .map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn load_client_secret(cred_id: Uuid) -> Result<Option<String>> {
    match entry(&format!("cred/{cred_id}/secret"))?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}
