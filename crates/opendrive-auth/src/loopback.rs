//! Ephemeral loopback HTTP listener for OAuth redirect.

use std::net::SocketAddr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoopbackError {
    #[error("bind: {0}")]
    Bind(String),
    #[error("timeout")]
    Timeout,
    #[error("bad request")]
    BadRequest,
}

/// Result of a successful redirect: authorization code + state.
#[derive(Debug)]
pub struct RedirectParams {
    pub code: String,
    pub state: String,
}

/// Placeholder: real impl binds 127.0.0.1:0, serves one GET, returns code/state.
pub async fn wait_for_redirect(
    _expected_state: &str,
    _timeout_secs: u64,
) -> Result<(RedirectParams, SocketAddr), LoopbackError> {
    Err(LoopbackError::Timeout)
}
