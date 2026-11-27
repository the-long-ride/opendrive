//! Token refresh, single-account mount activation, virtual drive sync, and Bridge helpers.

use super::token_refresh::{attach_account_refresher, fetch_fresh_access_token};
use super::{MountManager, MountedAccount};
use crate::runtime::auth_runner::{GOOGLE_DRIVE_API_BASE, GOOGLE_DRIVE_UPLOAD_BASE};
use crate::runtime::bridge::Bridge;
use crate::runtime::bridge_types::{str_err, AppStateResponse};
use eframe::egui;
use opendrive_auth::SecretStore;
use opendrive_core::{Account, AccountStatus, AppController, CredentialSet};
use opendrive_drive::DriveClient;
use opendrive_sync::{SyncEngine, VfsEngine};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

impl MountManager {
    pub(super) fn unmount_stale(&self, accounts: &[Account]) {
        let active: HashMap<Uuid, &str> = accounts
            .iter()
            .filter(|a| a.enabled && a.status == AccountStatus::Connected)
            .map(|a| (a.id, a.folder_name.as_str()))
            .collect();
        let removed: Vec<MountedAccount> = {
            let mut guard = self.mounted.lock().expect("mounted lock");
            let stale: Vec<Uuid> = guard
                .iter()
                .filter(|(id, entry)| active.get(id).copied() != Some(entry.folder.as_str()))
                .map(|(id, _)| *id)
                .collect();
            stale
                .into_iter()
                .filter_map(|id| guard.remove(&id))
                .collect()
        };
        for entry in removed {
            let path = entry.path.clone();
            drop(entry);
            self.backend.unregister_account(&path);
        }
    }

    pub(crate) fn refresh_drive_client_for(
        &self,
        account_id: Uuid,
        ctrl: &Arc<Mutex<AppController>>,
        engine: &Arc<SyncEngine>,
        secrets: &Arc<dyn SecretStore>,
        rt: &tokio::runtime::Handle,
    ) -> Result<(), String> {
        let (acct, creds) = {
            let g = ctrl.lock().expect("controller lock");
            let acct = g
                .accounts
                .get_account(account_id)
                .map_err(|_| "account not found".to_string())?
                .clone();
            (acct, g.accounts.list_credentials().to_vec())
        };
        let refreshed = rt.block_on(self.refresh_drive_client(&acct, &creds, ctrl, secrets))?;
        let drive = refreshed.ok_or_else(|| {
            format!(
                "Missing saved credentials for {} — remove and reconnect account",
                acct.nickname
            )
        })?;
        engine.set_drive_client(account_id, drive);
        Ok(())
    }

    async fn refresh_drive_client(
        &self,
        acct: &Account,
        creds: &[CredentialSet],
        ctrl: &Arc<Mutex<AppController>>,
        secrets: &Arc<dyn SecretStore>,
    ) -> Result<Option<DriveClient>, String> {
        let api_base = self
            .api_base_override
            .as_deref()
            .unwrap_or(GOOGLE_DRIVE_API_BASE);
        let upload_base = self
            .upload_base_override
            .as_deref()
            .unwrap_or(GOOGLE_DRIVE_UPLOAD_BASE);
        let access =
            fetch_fresh_access_token(acct, creds, secrets, self.token_url_override.as_deref())
                .await?;
        Ok(access.map(|at| {
            attach_account_refresher(
                DriveClient::with_base_urls(at, api_base, upload_base),
                acct.id,
                ctrl,
                secrets,
                self.token_url_override.as_deref(),
            )
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn mount_one(
        &self,
        acct: Account,
        root: &Path,
        creds: &[CredentialSet],
        ctrl: &Arc<Mutex<AppController>>,
        engine: &Arc<SyncEngine>,
        secrets: &Arc<dyn SecretStore>,
        rt: &Arc<tokio::runtime::Runtime>,
    ) -> Result<(), String> {
        let acct_path = root.join(&acct.folder_name);
        self.backend.prepare_root(&acct_path)?;
        let root_node = engine
            .seed_root_account(acct.id, &acct.folder_name)
            .map_err(str_err)?;
        let mut auth_issue: Option<String> = None;
        if engine.drive_client_for(acct.id).is_err() {
            match rt.block_on(self.refresh_drive_client(&acct, creds, ctrl, secrets)) {
                Ok(Some(drive)) => engine.set_drive_client(acct.id, drive),
                Ok(None) => {
                    auth_issue = Some(format!(
                        "Missing saved credentials for {} — remove and reconnect account",
                        acct.nickname
                    ));
                }
                Err(err) => {
                    tracing::warn!(account_id = %acct.id, %err, "token refresh failed");
                    auth_issue = Some(format!(
                        "Session refresh failed for {}: {err}",
                        acct.nickname
                    ));
                }
            }
        }
        let Ok(drive) = engine.drive_client_for(acct.id) else {
            {
                let mut g = ctrl.lock().expect("controller lock");
                if let Ok(stored) = g.accounts.get_account_mut(acct.id) {
                    stored.status = AccountStatus::NeedsReauth;
                    self.needs_refresh.store(true, Ordering::Relaxed);
                }
            }
            return Err(auth_issue
                .unwrap_or_else(|| format!("No active Drive session for {}", acct.nickname)));
        };
        if let Ok(about) = rt.block_on(drive.about()) {
            {
                let mut g = ctrl.lock().expect("controller lock");
                if let Ok(stored) = g.accounts.get_account_mut(acct.id) {
                    stored.storage_used = about.storage_used;
                    stored.storage_quota = about.storage_quota;
                    if about.photo_url.is_some() {
                        stored.avatar_url = about.photo_url.clone();
                    }
                    self.needs_refresh.store(true, Ordering::Relaxed);
                }
            }
            if let Some(dir) = self.data_dir_or_default() {
                let _ = rt.block_on(crate::design::avatars::cache_account_avatar(
                    &drive,
                    &dir,
                    acct.id,
                    about.photo_url.as_deref(),
                ));
            }
        }
        let _ = rt.block_on(engine.hydrate_folder(acct.id, root_node));
        let conn = self.backend.connect_account(
            &acct,
            &acct_path,
            root_node,
            Arc::clone(engine) as Arc<dyn VfsEngine>,
            rt.handle(),
        )?;
        let entries = engine.local_reconcile_entries(root_node);
        self.backend
            .reconcile_placeholder_entries(&acct_path, &entries);
        let poller = {
            let _g = rt.enter();
            engine.spawn_change_poller(acct.id, Duration::from_secs(30))
        };
        self.mounted.lock().expect("mounted lock").insert(
            acct.id,
            MountedAccount {
                folder: acct.folder_name,
                poller,
                path: acct_path,
                _conn: conn,
            },
        );
        Ok(())
    }
}

impl Bridge {
    pub fn spawn_mount_sync(&self, repaint_ctx: Option<egui::Context>) {
        let mounts = Arc::clone(&self.mounts);
        let ctrl = Arc::clone(&self.ctrl);
        let engine = Arc::clone(&self.engine);
        let secrets = Arc::clone(&self.secrets);
        let config_dir = self.config_dir.clone();
        std::thread::spawn(move || {
            if let Err(err) = mounts.sync_now(&ctrl, &engine, &secrets) {
                *mounts.pending_notice.lock().expect("notice lock") = Some((true, err));
            }
            if mounts.needs_refresh.load(Ordering::Relaxed) {
                if let Some(dir) = &config_dir {
                    let _ = ctrl.lock().expect("controller lock").accounts.save_to(dir);
                }
            }
            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
            }
        });
    }

    pub fn take_mount_refresh(&self) -> bool {
        self.mounts.needs_refresh.swap(false, Ordering::Relaxed)
    }

    pub fn take_mount_notice(&self) -> Option<(bool, String)> {
        self.mounts
            .pending_notice
            .lock()
            .expect("notice lock")
            .take()
    }

    pub fn update_virtual_drive(&self, enabled: bool) -> Result<AppStateResponse, String> {
        self.modify_settings(|s| s.virtual_drive_enabled = enabled, |_| Ok(()))
    }
}
