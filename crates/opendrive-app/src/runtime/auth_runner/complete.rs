use super::{OAuthStartParams, ServiceAccountParams};
use crate::design::formatters::default_folder_name;
use crate::runtime::bridge::Bridge;
use opendrive_auth::SecretStore;
use opendrive_core::{default_account_alias, Account, AccountStatus, CredentialSet};
use opendrive_drive::{About, DriveClient};
use std::sync::Arc;
use uuid::Uuid;

fn load_secret_logged(secrets: &Arc<dyn SecretStore>, cred_id: Uuid) -> String {
    match secrets.load_client_secret(cred_id) {
        Ok(Some(secret)) => secret,
        Ok(None) => String::new(),
        Err(err) => {
            tracing::warn!(%cred_id, %err, "failed to load OAuth client secret from secret store");
            String::new()
        }
    }
}

fn env_fallback_secret() -> String {
    std::env::var("OPENDRIVE_CLIENT_SECRET").unwrap_or_default()
}

pub(super) fn resolve_account_names(
    about: &About,
    nickname_override: &str,
    folder_override: &str,
) -> (String, String) {
    let nickname = default_account_alias(nickname_override, &about.email);
    let folder_name = default_folder_name(folder_override, &nickname, &about.email);
    (nickname, folder_name)
}

impl Bridge {
    pub fn saved_oauth_credentials(&self) -> (String, String) {
        let creds = {
            let g = self.ctrl.lock().expect("controller lock");
            g.accounts.list_credentials().to_vec()
        };
        let (mut client_id, mut client_secret) = creds
            .iter()
            .rev()
            .find_map(|c| {
                let s = load_secret_logged(&self.secrets, c.id);
                (!c.client_id.trim().is_empty() && !s.trim().is_empty())
                    .then(|| (c.client_id.clone(), s))
            })
            .unwrap_or_else(|| {
                let last_id = creds
                    .last()
                    .map(|c| c.client_id.clone())
                    .unwrap_or_default();
                (last_id, String::new())
            });
        if client_id.trim().is_empty() {
            if let Ok(env_id) = std::env::var("OPENDRIVE_CLIENT_ID") {
                client_id = env_id;
            }
        }
        if client_secret.trim().is_empty() {
            let env_secret = env_fallback_secret();
            if !env_secret.trim().is_empty() {
                client_secret = env_secret;
            }
        }
        (client_id, client_secret)
    }

    pub fn credentials_for_account(&self, account_id: Option<Uuid>) -> (String, String) {
        if let Some(id) = account_id {
            let (is_sa, cred, sibling_ids) = {
                let g = self.ctrl.lock().expect("controller lock");
                let acct = g.accounts.get_account(id).ok();
                let is_sa = acct.is_some_and(|a| a.credential_set_id.is_nil());
                let cred = acct
                    .filter(|a| !a.credential_set_id.is_nil())
                    .and_then(|a| g.accounts.get_credential(a.credential_set_id).ok().cloned());
                let siblings = match &cred {
                    Some(c) if !c.client_id.trim().is_empty() => g
                        .accounts
                        .list_credentials()
                        .iter()
                        .filter(|other| other.id != c.id && other.client_id == c.client_id)
                        .map(|other| other.id)
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                (is_sa, cred, siblings)
            };
            if is_sa {
                return (String::new(), String::new());
            }
            if let Some(c) = cred {
                let mut secret = load_secret_logged(&self.secrets, c.id);
                if secret.trim().is_empty() {
                    secret = sibling_ids
                        .into_iter()
                        .map(|sid| load_secret_logged(&self.secrets, sid))
                        .find(|s| !s.trim().is_empty())
                        .unwrap_or_else(env_fallback_secret);
                }
                return (c.client_id, secret);
            }
        }
        self.saved_oauth_credentials()
    }

    pub fn load_service_account_json(&self, account_id: Uuid) -> Option<String> {
        self.secrets
            .load_service_account_json(account_id)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn save_oauth_client_credentials(
        &self,
        client_id: &str,
        client_secret: &str,
    ) -> Result<Uuid, String> {
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        let cred_id = {
            let mut g = self.ctrl.lock().expect("controller lock");
            let cred_id = if let Some(existing) = g
                .accounts
                .list_credentials()
                .iter()
                .find(|c| c.client_id == client_id)
            {
                existing.id
            } else {
                let id = Uuid::new_v4();
                g.accounts.add_credential(CredentialSet {
                    id,
                    label: "Desktop OAuth".into(),
                    client_id: client_id.to_string(),
                });
                id
            };
            if let Some(dir) = &self.config_dir {
                g.accounts.save_to(dir).map_err(|e| e.to_string())?;
            }
            cred_id
        };
        self.secrets
            .store_client_secret(cred_id, client_secret)
            .map_err(|e| format!("Failed to save OAuth client secret in OS keyring: {e}"))?;
        Ok(cred_id)
    }

    pub(super) fn finish_or_update_account(
        &self,
        target_id: Option<Uuid>,
        new_account: Account,
        about: &About,
        drive: DriveClient,
    ) -> Result<(Uuid, u64, String), String> {
        let (account_id, folder_name, message) = {
            let mut g = self.ctrl.lock().expect("controller lock");
            let info = if let Some(existing_id) = target_id {
                if let Ok(existing) = g.accounts.get_account_mut(existing_id) {
                    existing.credential_set_id = new_account.credential_set_id;
                    if !about.email.is_empty() {
                        existing.email = about.email.clone();
                    }
                    if !new_account.nickname.trim().is_empty() {
                        existing.nickname = new_account.nickname.clone();
                    }
                    existing.read_only = new_account.read_only;
                    existing.status = AccountStatus::Connected;
                    existing.enabled = true;
                    existing.storage_used = about.storage_used;
                    existing.storage_quota = about.storage_quota;
                    if about.photo_url.is_some() {
                        existing.avatar_url = about.photo_url.clone();
                    }
                    Some((
                        existing.id,
                        existing.folder_name.clone(),
                        format!("Re-authenticated {}", existing.nickname),
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            let result = match info {
                Some(pair) => pair,
                None => {
                    let id = new_account.id;
                    let folder = new_account.folder_name.clone();
                    let msg = format!("Connected {}", new_account.nickname);
                    g.accounts.add_account(new_account);
                    (id, folder, msg)
                }
            };
            if let Some(dir) = &self.config_dir {
                g.accounts.save_to(dir).map_err(|e| e.to_string())?;
            }
            result
        };
        self.engine.set_drive_client(account_id, drive);
        let root_id = self
            .engine
            .seed_root_account(account_id, &folder_name)
            .map_err(|e| e.to_string())?;
        Ok((account_id, root_id, message))
    }

    async fn cache_avatar_if_present(
        &self,
        drive: &DriveClient,
        account_id: Uuid,
        photo_url: Option<&str>,
    ) {
        if let Some(dir) = &self.data_dir {
            let _ = crate::design::avatars::cache_account_avatar(drive, dir, account_id, photo_url)
                .await;
        }
    }

    async fn complete_resolved_account(
        &self,
        target_account_id: Option<Uuid>,
        account_id: Uuid,
        cred_id: Uuid,
        (nick, folder, read_only): (&str, &str, bool),
        about: &About,
        drive: DriveClient,
    ) -> Result<(Uuid, u64, String), String> {
        let (nickname, folder_name) = resolve_account_names(about, nick, folder);
        let account = Account {
            id: account_id,
            nickname,
            email: about.email.clone(),
            credential_set_id: cred_id,
            folder_name,
            read_only,
            enabled: true,
            status: AccountStatus::Connected,
            storage_used: about.storage_used,
            storage_quota: about.storage_quota,
            avatar_url: about.photo_url.clone(),
        };
        self.cache_avatar_if_present(&drive, account_id, about.photo_url.as_deref())
            .await;
        self.finish_or_update_account(target_account_id, account, about, drive)
    }

    pub(super) async fn complete_oauth_account(
        &self,
        params: &OAuthStartParams,
        refresh_token: &str,
        about: &About,
        drive: DriveClient,
    ) -> Result<(Uuid, u64, String), String> {
        let cred_id =
            self.save_oauth_client_credentials(&params.client_id, &params.client_secret)?;
        let account_id = params.target_account_id.unwrap_or_else(Uuid::new_v4);
        self.secrets
            .store_refresh_token(account_id, refresh_token)
            .map_err(|e| format!("Failed to save refresh token in OS keyring: {e}"))?;
        let meta = (&*params.nickname, &*params.folder_name, params.read_only);
        let target = params.target_account_id;
        self.complete_resolved_account(target, account_id, cred_id, meta, about, drive)
            .await
    }

    pub(super) async fn complete_service_account(
        &self,
        params: &ServiceAccountParams,
        raw_json: &str,
        about: &About,
        drive: DriveClient,
    ) -> Result<(Uuid, u64, String), String> {
        let account_id = params.target_account_id.unwrap_or_else(Uuid::new_v4);
        self.secrets
            .store_service_account_json(account_id, raw_json)
            .map_err(|e| e.to_string())?;
        let meta = (&*params.nickname, &*params.folder_name, params.read_only);
        let target = params.target_account_id;
        self.complete_resolved_account(target, account_id, Uuid::nil(), meta, about, drive)
            .await
    }
}
