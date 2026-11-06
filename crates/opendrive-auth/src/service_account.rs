//! Google Service Account JSON (no-login) JWT RS256 bearer exchange.

use crate::{post_token_form, scope_for_access, AuthError, Result};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{RsaKeyPair, RSA_PKCS1_SHA256};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const JWT_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

fn default_token_uri() -> String {
    DEFAULT_TOKEN_URI.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccountKey {
    #[serde(default)]
    pub r#type: String,
    pub client_email: String,
    pub private_key: String,
    #[serde(default = "default_token_uri")]
    pub token_uri: String,
    #[serde(default)]
    pub project_id: Option<String>,
}

pub fn parse_pkcs8_pem(pem: &str) -> Result<Vec<u8>> {
    let mut b64 = String::new();
    for line in pem.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("-----") {
            continue;
        }
        b64.push_str(trimmed);
    }
    if b64.is_empty() {
        return Err(AuthError::OAuth(
            "service account private_key PEM is empty".into(),
        ));
    }
    let der = STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| AuthError::OAuth(format!("invalid private_key base64: {e}")))?;
    RsaKeyPair::from_pkcs8(&der)
        .map_err(|_| AuthError::OAuth("invalid PKCS#8 RSA private_key".into()))?;
    Ok(der)
}

impl ServiceAccountKey {
    pub fn from_json_str(raw: &str) -> Result<Self> {
        let mut key: Self = serde_json::from_str(raw)
            .map_err(|e| AuthError::OAuth(format!("invalid service account JSON: {e}")))?;
        if !key.client_email.trim().contains('@') {
            return Err(AuthError::OAuth(
                "service account client_email must be valid".into(),
            ));
        }
        if key.token_uri.trim().is_empty() {
            key.token_uri = default_token_uri();
        }
        parse_pkcs8_pem(&key.private_key)?;
        Ok(key)
    }

    pub fn from_json_or_path(input: &str) -> Result<(Self, String)> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(AuthError::OAuth(
                "service account JSON or file path is required".into(),
            ));
        }
        if trimmed.starts_with('{') {
            let key = Self::from_json_str(trimmed)?;
            return Ok((key, trimmed.to_string()));
        }
        let path = trimmed.trim_matches(|c| c == '"' || c == '\'');
        let raw = std::fs::read_to_string(path)
            .map_err(|e| AuthError::OAuth(format!("read service account file: {e}")))?;
        let key = Self::from_json_str(&raw)?;
        Ok((key, raw))
    }

    pub fn sign_jwt(&self, read_only: bool, now_epoch_secs: i64) -> Result<String> {
        let der = parse_pkcs8_pem(&self.private_key)?;
        let key_pair = RsaKeyPair::from_pkcs8(&der)
            .map_err(|_| AuthError::OAuth("invalid PKCS#8 RSA private_key".into()))?;
        let header = serde_json::json!({ "alg": "RS256", "typ": "JWT" }).to_string();
        let claims = serde_json::json!({
            "iss": self.client_email,
            "scope": scope_for_access(read_only),
            "aud": self.token_uri,
            "iat": now_epoch_secs,
            "exp": now_epoch_secs + 3600,
        })
        .to_string();
        let signing_input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.as_bytes()),
            URL_SAFE_NO_PAD.encode(claims.as_bytes())
        );
        let rng = SystemRandom::new();
        let mut sig = vec![0u8; key_pair.public().modulus_len()];
        key_pair
            .sign(&RSA_PKCS1_SHA256, &rng, signing_input.as_bytes(), &mut sig)
            .map_err(|_| AuthError::OAuth("failed to sign JWT assertion".into()))?;
        Ok(format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(&sig)))
    }

    pub async fn fetch_access_token(&self, read_only: bool) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AuthError::OAuth(format!("system clock error: {e}")))?
            .as_secs() as i64;
        let jwt = self.sign_jwt(read_only, now)?;
        let body = post_token_form(
            &self.token_uri,
            &[("grant_type", JWT_GRANT_TYPE), ("assertion", jwt.as_str())],
        )
        .await?;
        body.access_token
            .ok_or_else(|| AuthError::OAuth("missing access_token".into()))
    }
}
