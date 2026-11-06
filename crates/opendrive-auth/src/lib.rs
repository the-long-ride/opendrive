//! OAuth loopback + PKCE, Service Account JWT auth, and token store (keyring).

mod keyring;
mod loopback;
mod pkce;
mod secret_store;
mod service_account;
pub mod vault;

pub use keyring::{
    delete_refresh_token, delete_service_account_json, load_client_secret, load_refresh_token,
    load_service_account_json, store_client_secret, store_refresh_token,
    store_service_account_json, OsKeyringStore,
};
pub use loopback::{bind_loopback, wait_for_redirect, LoopbackError, RedirectParams};
pub use pkce::{generate_pkce, random_state, PkceError, PkcePair};
pub use secret_store::{MemorySecretStore, SecretStore};
pub use service_account::{parse_pkcs8_pem, ServiceAccountKey, DEFAULT_TOKEN_URI};
pub use vault::{
    decrypt_vault, encrypt_vault, validate_vault_password, EncryptedVaultFile, ExportedAccount,
    ExportedCredentialSet, VaultPayload,
};

use serde::Deserialize;
use thiserror::Error;
use url::Url;

pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("oauth failed: {0}")]
    OAuth(String),
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("vault: {0}")]
    Vault(String),
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
pub(crate) struct TokenResponse {
    pub(crate) access_token: Option<String>,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: Option<u64>,
    pub(crate) error: Option<String>,
}

pub(crate) fn scope_for_access(read_only: bool) -> &'static str {
    if read_only {
        "https://www.googleapis.com/auth/drive.readonly"
    } else {
        "https://www.googleapis.com/auth/drive"
    }
}

pub(crate) async fn post_token_form(
    token_url: &str,
    params: &[(&str, &str)],
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post(token_url)
        .form(params)
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
    Ok(body)
}

pub struct AuthClient {
    pub client_id: String,
    pub client_secret: String,
    auth_url: String,
    token_url: String,
}

impl AuthClient {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self::with_endpoints(client_id, client_secret, AUTH_URL, TOKEN_URL)
    }

    pub fn with_endpoints(
        client_id: String,
        client_secret: String,
        auth_url: impl Into<String>,
        token_url: impl Into<String>,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: auth_url.into(),
            token_url: token_url.into(),
        }
    }

    pub fn authorization_url(
        &self,
        redirect_uri: &str,
        challenge: &str,
        state: &str,
        read_only: bool,
    ) -> String {
        let mut u = Url::parse(&self.auth_url).expect("valid auth url");
        u.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", scope_for_access(read_only))
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);
        u.to_string()
    }

    /// Browser flow: bind -> open browser -> wait -> exchange.
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
        let body = post_token_form(
            &self.token_url,
            &[
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier),
            ],
        )
        .await?;
        let access = body
            .access_token
            .ok_or_else(|| AuthError::OAuth("missing access_token".into()))?;
        let refresh = body
            .refresh_token
            .ok_or_else(|| AuthError::OAuth("no refresh_token (check prompt=consent)".into()))?;
        let _ = body.expires_in;
        Ok((access, refresh))
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<String> {
        let body = post_token_form(
            &self.token_url,
            &[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ],
        )
        .await?;
        body.access_token
            .ok_or_else(|| AuthError::OAuth("missing access_token".into()))
    }
}
