//! Bridge extensions for persistent account alias renaming and encrypted credential vault export/import.

use crate::runtime::bridge::{AppStateResponse, Bridge};
use crate::runtime::bridge_types::str_err;
use opendrive_auth::{
    decrypt_vault, encrypt_vault, validate_vault_password, ExportedAccount, ExportedCredentialSet,
    VaultPayload,
};
use opendrive_core::{
    default_account_alias, sanitize_folder_segment, save_settings, Account, AccountStatus,
    AppCommand, CredentialSet, Settings,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

impl Bridge {
    pub(crate) fn modify_settings(
        &self,
        mutate: impl FnOnce(&mut Settings),
        after_update: impl FnOnce(&Settings) -> Result<(), String>,
    ) -> Result<AppStateResponse, String> {
        let next = {
            let mut g = self.ctrl.lock().expect("controller lock");
            let mut updated = g.settings.clone();
            mutate(&mut updated);
            g.handle(AppCommand::UpdateSettings(updated.clone()));
            updated
        };
        after_update(&next)?;
        if let Some(dir) = &self.config_dir {
            save_settings(dir, &next).map_err(str_err)?;
        }
        Ok(self.state_snapshot())
    }

    pub fn update_release_prefs(
        &self,
        check_updates: bool,
        ignored_release_version: Option<String>,
    ) -> Result<AppStateResponse, String> {
        self.modify_settings(
            |s| {
                s.check_updates = check_updates;
                s.ignored_release_version = ignored_release_version;
            },
            |_| Ok(()),
        )
    }

    pub fn rename_account(
        &self,
        id_str: &str,
        nickname: String,
        folder_name: Option<String>,
    ) -> Result<(AppStateResponse, bool), String> {
        let id = Uuid::parse_str(id_str.trim()).map_err(|_| "invalid account id".to_string())?;
        let folder_changed = {
            let mut g = self.ctrl.lock().expect("controller lock");
            let prev_folder = g
                .accounts
                .get_account(id)
                .map_err(|_| "account not found".to_string())?
                .folder_name
                .clone();
            g.handle(AppCommand::RenameAccount {
                id,
                nickname,
                folder_name,
            });
            let next_folder = g
                .accounts
                .get_account(id)
                .map_err(|_| "account not found".to_string())?
                .folder_name
                .clone();
            if let Some(dir) = &self.config_dir {
                g.accounts.save_to(dir).map_err(str_err)?;
            }
            next_folder != prev_folder
        };
        Ok((self.state_snapshot(), folder_changed))
    }

    pub fn export_encrypted_vault(&self, password: &str) -> Result<Vec<u8>, String> {
        validate_vault_password(password).map_err(|e| e.to_string())?;
        let (cred_list, acct_list) = {
            let g = self.ctrl.lock().expect("controller lock");
            (
                g.accounts.list_credentials().to_vec(),
                g.accounts.list_accounts().to_vec(),
            )
        };
        let mut exported_creds = Vec::with_capacity(cred_list.len());
        let mut cred_id_to_client_id = HashMap::new();
        for c in &cred_list {
            cred_id_to_client_id.insert(c.id, c.client_id.clone());
            let secret = self
                .secrets
                .load_client_secret(c.id)
                .map_err(str_err)?
                .unwrap_or_default();
            if !c.client_id.trim().is_empty() && !secret.trim().is_empty() {
                exported_creds.push(ExportedCredentialSet {
                    label: c.label.clone(),
                    client_id: c.client_id.clone(),
                    client_secret: secret,
                });
            }
        }

        let mut exported_accts = Vec::with_capacity(acct_list.len());
        for a in &acct_list {
            let refresh_token = self.secrets.load_refresh_token(a.id).map_err(str_err)?;
            let service_account_json = self
                .secrets
                .load_service_account_json(a.id)
                .map_err(str_err)?;
            exported_accts.push(ExportedAccount {
                nickname: a.nickname.clone(),
                email: a.email.clone(),
                folder_name: a.folder_name.clone(),
                read_only: a.read_only,
                enabled: a.enabled,
                credential_client_id: cred_id_to_client_id.get(&a.credential_set_id).cloned(),
                refresh_token,
                service_account_json,
                avatar_url: a.avatar_url.clone(),
            });
        }
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let payload = VaultPayload {
            exported_at: format!("unix:{secs}"),
            credentials: exported_creds,
            accounts: exported_accts,
        };
        encrypt_vault(&payload, password).map_err(str_err)
    }

    pub fn import_encrypted_vault(
        &self,
        bytes: &[u8],
        password: &str,
    ) -> Result<(AppStateResponse, usize), String> {
        let payload = decrypt_vault(bytes, password).map_err(str_err)?;
        let mut cred_map: HashMap<String, Uuid> = HashMap::new();
        let mut imported_accounts = 0usize;
        let mut seed_targets: Vec<(Uuid, String)> = Vec::new();
        let mut secret_writes: Vec<(Uuid, Option<String>, Option<String>)> = Vec::new();

        {
            let mut g = self.ctrl.lock().expect("controller lock");
            for existing in g.accounts.list_credentials() {
                cred_map.insert(existing.client_id.clone(), existing.id);
            }
            for ec in payload.credentials {
                if ec.client_id.trim().is_empty() {
                    continue;
                }
                let cred_id = if let Some(&id) = cred_map.get(&ec.client_id) {
                    id
                } else {
                    let id = Uuid::new_v4();
                    g.accounts.add_credential(CredentialSet {
                        id,
                        label: ec.label,
                        client_id: ec.client_id.clone(),
                    });
                    cred_map.insert(ec.client_id.clone(), id);
                    id
                };
                if !ec.client_secret.trim().is_empty() {
                    self.secrets
                        .store_client_secret(cred_id, &ec.client_secret)
                        .map_err(str_err)?;
                }
            }

            for ea in payload.accounts {
                let cred_id = ea
                    .credential_client_id
                    .as_deref()
                    .and_then(|cid| cred_map.get(cid).copied())
                    .unwrap_or(Uuid::nil());
                let alias = default_account_alias(&ea.nickname, &ea.email);
                let folder =
                    sanitize_folder_segment(&ea.folder_name).unwrap_or_else(|| alias.clone());
                let has_token = ea.refresh_token.is_some() || ea.service_account_json.is_some();
                let next_status = if !ea.enabled {
                    AccountStatus::Paused
                } else if has_token {
                    AccountStatus::Connected
                } else {
                    AccountStatus::NeedsReauth
                };

                let existing_id = g
                    .accounts
                    .list_accounts()
                    .iter()
                    .find(|a| {
                        (!ea.email.is_empty() && a.email.eq_ignore_ascii_case(&ea.email))
                            || a.folder_name == folder
                    })
                    .map(|a| a.id);

                let acct_id = if let Some(id) = existing_id {
                    let acct = g.accounts.get_account_mut(id).expect("existing account");
                    acct.nickname = alias;
                    acct.folder_name = folder.clone();
                    acct.read_only = ea.read_only;
                    acct.enabled = ea.enabled;
                    acct.status = next_status;
                    acct.credential_set_id = cred_id;
                    if ea.avatar_url.is_some() {
                        acct.avatar_url = ea.avatar_url.clone();
                    }
                    id
                } else {
                    let id = Uuid::new_v4();
                    g.accounts.add_account(Account {
                        id,
                        nickname: alias,
                        email: ea.email,
                        credential_set_id: cred_id,
                        folder_name: folder.clone(),
                        read_only: ea.read_only,
                        enabled: ea.enabled,
                        status: next_status,
                        storage_used: 0,
                        storage_quota: 0,
                        avatar_url: ea.avatar_url,
                    });
                    id
                };
                secret_writes.push((acct_id, ea.refresh_token, ea.service_account_json));
                seed_targets.push((acct_id, folder));
                imported_accounts += 1;
            }
            if let Some(dir) = &self.config_dir {
                g.accounts.save_to(dir).map_err(str_err)?;
            }
        }

        for (acct_id, rt, sa) in secret_writes {
            if let Some(tok) = rt {
                self.secrets
                    .store_refresh_token(acct_id, &tok)
                    .map_err(str_err)?;
            }
            if let Some(json) = sa {
                self.secrets
                    .store_service_account_json(acct_id, &json)
                    .map_err(str_err)?;
            }
        }
        for (acct_id, folder) in seed_targets {
            self.engine
                .seed_root_account(acct_id, &folder)
                .map_err(str_err)?;
        }
        Ok((self.state_snapshot(), imported_accounts))
    }
}
