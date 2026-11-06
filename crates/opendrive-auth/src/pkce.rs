//! PKCE (S256) helpers.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PkceError {
    #[error("rng: {0}")]
    Rng(String),
}

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

/// Generate a high-entropy verifier and its S256 challenge.
/// Real impl: 32+ random bytes, base64url, SHA-256.
pub fn generate_pkce() -> Result<PkcePair, PkceError> {
    // Placeholder values for compile/type shape; replace with real crypto.
    Ok(PkcePair {
        verifier: "pending-real-rng".into(),
        challenge: "pending-real-sha256".into(),
    })
}

pub fn random_state() -> String {
    "pending-real-state".into()
}
