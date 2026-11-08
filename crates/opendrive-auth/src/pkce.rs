//! PKCE (S256) helpers.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PkceError {
    #[error("rng failed")]
    Rng,
}

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

/// 32 random bytes → base64url verifier; challenge = BASE64URL(SHA256(verifier)).
pub fn generate_pkce() -> Result<PkcePair, PkceError> {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    let verifier = URL_SAFE_NO_PAD.encode(buf);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    Ok(PkcePair {
        verifier,
        challenge,
    })
}

pub fn random_state() -> String {
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_shape() {
        let p = generate_pkce().unwrap();
        assert!(p.verifier.len() >= 43);
        assert_eq!(p.challenge.len(), 43); // SHA256 = 32 bytes → 43 base64url
    }
}
