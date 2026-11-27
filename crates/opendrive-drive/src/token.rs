//! Automatic OAuth2 / Service Account access-token refresh state for `DriveClient`.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

const DEFAULT_PROACTIVE_REFRESH_AGE: Duration = Duration::from_secs(50 * 60);

pub type TokenFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send + 'a>>;

pub trait TokenRefresher: Send + Sync {
    fn refresh_token(&self) -> TokenFuture<'_>;
}

pub(crate) struct TokenStore {
    state: RwLock<(String, Instant)>,
    refresher: Option<Arc<dyn TokenRefresher>>,
    refresh_gate: tokio::sync::Mutex<()>,
    proactive_age: Duration,
}

impl TokenStore {
    pub(crate) fn new(access_token: String) -> Self {
        Self {
            state: RwLock::new((access_token, Instant::now())),
            refresher: None,
            refresh_gate: tokio::sync::Mutex::new(()),
            proactive_age: DEFAULT_PROACTIVE_REFRESH_AGE,
        }
    }

    pub(crate) fn with_refresher(&self, refresher: Arc<dyn TokenRefresher>) -> Self {
        let cur = self.state.read().expect("token lock").clone();
        Self {
            state: RwLock::new(cur),
            refresher: Some(refresher),
            refresh_gate: tokio::sync::Mutex::new(()),
            proactive_age: self.proactive_age,
        }
    }

    pub(crate) fn with_proactive_age(&self, proactive_age: Duration) -> Self {
        let cur = self.state.read().expect("token lock").clone();
        Self {
            state: RwLock::new(cur),
            refresher: self.refresher.clone(),
            refresh_gate: tokio::sync::Mutex::new(()),
            proactive_age,
        }
    }

    pub(crate) fn current_token(&self) -> String {
        self.state.read().expect("token lock").0.clone()
    }

    pub(crate) async fn valid_token(&self) -> String {
        let Some(refresher) = &self.refresher else {
            return self.current_token();
        };
        let (tok, acquired_at) = self.state.read().expect("token lock").clone();
        if acquired_at.elapsed() < self.proactive_age {
            return tok;
        }
        let _gate = self.refresh_gate.lock().await;
        let (latest_tok, latest_at) = self.state.read().expect("token lock").clone();
        if latest_at.elapsed() < self.proactive_age {
            return latest_tok;
        }
        match refresher.refresh_token().await {
            Ok(fresh) if !fresh.trim().is_empty() => {
                *self.state.write().expect("token lock") = (fresh.clone(), Instant::now());
                fresh
            }
            _ => latest_tok,
        }
    }

    pub(crate) async fn refresh_after_unauthorized(&self, failed_token: &str) -> Option<String> {
        let refresher = self.refresher.as_ref()?;
        let _gate = self.refresh_gate.lock().await;
        let cur = self.current_token();
        if cur != failed_token && !cur.trim().is_empty() {
            return Some(cur);
        }
        match refresher.refresh_token().await {
            Ok(fresh) if !fresh.trim().is_empty() => {
                *self.state.write().expect("token lock") = (fresh.clone(), Instant::now());
                Some(fresh)
            }
            _ => None,
        }
    }
}

pub(crate) async fn execute_authed_request(
    token: &TokenStore,
    build: impl Fn(&str) -> reqwest::RequestBuilder,
) -> crate::types::Result<reqwest::Response> {
    let tok = token.valid_token().await;
    let resp = build(&tok)
        .send()
        .await
        .map_err(|e| crate::types::DriveError::Http(e.to_string()))?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Some(fresh) = token.refresh_after_unauthorized(&tok).await {
            return build(&fresh)
                .send()
                .await
                .map_err(|e| crate::types::DriveError::Http(e.to_string()));
        }
    }
    Ok(resp)
}
