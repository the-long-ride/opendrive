//! Account OAuth2 / Service Account token refresh logic shared by MountManager & DriveClient.

use crate::runtime::bridge_types::str_err;
use opendrive_auth::{
    AuthClient, SecretStore, ServiceAccountKey, AUTH_URL, DEFAULT_TOKEN_URI, TOKEN_URL,
};
use opendrive_core::{Account, AppController, CredentialSet};
use opendrive_drive::{DriveClient, TokenFuture, TokenRefresher};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub(crate) async fn fetch_fresh_access_token(
    acct: &Account,
    creds: &[CredentialSet],
    secrets: &Arc<dyn SecretStore>,
    token_url_override: Option<&str>,
) -> Result<Option<String>, String> {
    let token_url = token_url_override.unwrap_or(TOKEN_URL);
    if let Some(rt_tok) = secrets
        .load_refresh_token(acct.id)
        .map_err(str_err)?
        .filter(|s| !s.trim().is_empty())
    {
        let cred = creds
            .iter()
            .find(|c| c.id == acct.credential_set_id)
            .ok_or_else(|| "missing OAuth credential set".to_string())?;
        let client_secret = secrets
            .load_client_secret(cred.id)
            .map_err(str_err)?
            .unwrap_or_default();
        let auth =
            AuthClient::with_endpoints(cred.client_id.clone(), client_secret, AUTH_URL, token_url);
        return auth.refresh(&rt_tok).await.map(Some).map_err(str_err);
    }
    if let Some(sa_json) = secrets
        .load_service_account_json(acct.id)
        .map_err(str_err)?
        .filter(|s| !s.trim().is_empty())
    {
        let mut key = ServiceAccountKey::from_json_str(&sa_json).map_err(str_err)?;
        if let Some(url) = token_url_override {
            if key.token_uri == DEFAULT_TOKEN_URI {
                key.token_uri = url.to_string();
            }
        }
        return key
            .fetch_access_token(acct.read_only)
            .await
            .map(Some)
            .map_err(str_err);
    }
    Ok(None)
}

pub(crate) struct AccountTokenRefresher {
    pub account_id: Uuid,
    pub ctrl: Arc<Mutex<AppController>>,
    pub secrets: Arc<dyn SecretStore>,
    pub token_url_override: Option<String>,
}

impl TokenRefresher for AccountTokenRefresher {
    fn refresh_token(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            let (acct, creds) = {
                let g = self.ctrl.lock().expect("controller lock");
                let acct = g
                    .accounts
                    .get_account(self.account_id)
                    .map_err(|_| "account not found".to_string())?
                    .clone();
                (acct, g.accounts.list_credentials().to_vec())
            };
            let refreshed = fetch_fresh_access_token(
                &acct,
                &creds,
                &self.secrets,
                self.token_url_override.as_deref(),
            )
            .await?;
            refreshed.ok_or_else(|| {
                format!(
                    "Missing saved credentials for {} — remove and reconnect account",
                    acct.nickname
                )
            })
        })
    }
}

pub(crate) fn attach_account_refresher(
    drive: DriveClient,
    account_id: Uuid,
    ctrl: &Arc<Mutex<AppController>>,
    secrets: &Arc<dyn SecretStore>,
    token_url_override: Option<&str>,
) -> DriveClient {
    drive.with_token_refresher(Arc::new(AccountTokenRefresher {
        account_id,
        ctrl: Arc::clone(ctrl),
        secrets: Arc::clone(secrets),
        token_url_override: token_url_override.map(str::to_owned),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_auth::MemorySecretStore;
    use opendrive_core::{AccountStatus, AppController};

    #[tokio::test]
    async fn account_token_refresher_covers_missing_account_secrets_and_creds() {
        let (aid, cid) = (Uuid::new_v4(), Uuid::new_v4());
        let ctrl = Arc::new(Mutex::new(AppController::new(Default::default())));
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
        let mk = |id| AccountTokenRefresher {
            account_id: id,
            ctrl: Arc::clone(&ctrl),
            secrets: Arc::clone(&secrets),
            token_url_override: None,
        };
        assert!(mk(aid)
            .refresh_token()
            .await
            .unwrap_err()
            .contains("account not found"));
        ctrl.lock().unwrap().accounts.add_account(Account {
            id: aid,
            nickname: "A".into(),
            email: "a@x.com".into(),
            credential_set_id: cid,
            folder_name: "A".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::Connected,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
        assert!(mk(aid)
            .refresh_token()
            .await
            .unwrap_err()
            .contains("Missing saved credentials"));
        secrets.store_refresh_token(aid, "rt").unwrap();
        assert!(mk(aid)
            .refresh_token()
            .await
            .unwrap_err()
            .contains("missing OAuth credential set"));
    }
}
