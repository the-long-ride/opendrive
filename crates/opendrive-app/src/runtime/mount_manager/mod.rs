//! Background multi-account OS mount orchestration, token refresh, and virtual drive lifecycle.

mod backend;
pub mod live;
mod sync_ops;
mod virtual_drive;

pub use backend::{MountBackend, NullMountBackend};
pub use live::{cleanup_os_mounts, cleanup_os_mounts_for_pid, spawn_mount_watchdog};

use crate::runtime::bridge_types::str_err;
use live::LiveMountBackend;
use opendrive_auth::SecretStore;
use opendrive_core::{Account, AccountStatus, AppController};
use opendrive_sync::SyncEngine;
use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

pub(super) struct MountedAccount {
    pub(super) folder: String,
    pub(super) poller: tokio::task::JoinHandle<()>,
    pub(super) path: PathBuf,
    pub(super) _conn: Box<dyn Any + Send + Sync>,
}

impl Drop for MountedAccount {
    fn drop(&mut self) {
        self.poller.abort();
    }
}

pub struct MountManager {
    backend: Arc<dyn MountBackend>,
    mount_root_override: Option<PathBuf>,
    pub(super) token_url_override: Option<String>,
    pub(super) api_base_override: Option<String>,
    pub(super) upload_base_override: Option<String>,
    runtime: Mutex<Option<Arc<tokio::runtime::Runtime>>>,
    worker_started: Mutex<bool>,
    pub(super) mounted: Mutex<HashMap<Uuid, MountedAccount>>,
    pub(super) last_virtual_drive:
        Mutex<Option<(bool, String, Option<opendrive_platform::ExplorerQuota>)>>,
    sync_mutex: Mutex<()>,
    pub(super) pending_notice: Mutex<Option<(bool, String)>>,
    pub(super) needs_refresh: AtomicBool,
}

impl MountManager {
    pub fn data_dir_or_default(&self) -> Option<PathBuf> {
        self.mount_root_override
            .as_ref()
            .map(|r| r.join(".avatars"))
            .or_else(|| opendrive_platform::data_dir().ok())
    }

    pub fn new(backend: Arc<dyn MountBackend>) -> Self {
        Self {
            backend,
            mount_root_override: None,
            token_url_override: None,
            api_base_override: None,
            upload_base_override: None,
            runtime: Mutex::new(None),
            worker_started: Mutex::new(false),
            mounted: Mutex::new(HashMap::new()),
            last_virtual_drive: Mutex::new(None),
            sync_mutex: Mutex::new(()),
            pending_notice: Mutex::new(None),
            needs_refresh: AtomicBool::new(false),
        }
    }

    pub fn new_null() -> Self {
        Self::new(Arc::new(NullMountBackend))
    }

    pub fn new_live() -> Self {
        Self::new(Arc::new(LiveMountBackend))
    }

    pub fn with_mount_root(mut self, path: PathBuf) -> Self {
        self.mount_root_override = Some(path);
        self
    }

    pub fn with_token_url(mut self, url: String) -> Self {
        self.token_url_override = Some(url);
        self
    }

    pub fn with_drive_urls(mut self, api_base: String, upload_base: String) -> Self {
        self.api_base_override = Some(api_base);
        self.upload_base_override = Some(upload_base);
        self
    }

    pub fn mount_root(&self) -> Result<PathBuf, String> {
        match &self.mount_root_override {
            Some(path) => Ok(path.clone()),
            None => opendrive_platform::mount_root().map_err(str_err),
        }
    }

    pub fn mounted_ids(&self) -> Vec<Uuid> {
        self.mounted
            .lock()
            .expect("mounted lock")
            .keys()
            .copied()
            .collect()
    }

    pub fn is_virtual_drive_active(&self) -> bool {
        self.last_virtual_drive
            .lock()
            .expect("virtual drive lock")
            .as_ref()
            .is_some_and(|(on, _, _)| *on)
    }

    pub fn is_account_mounted(&self, account_id: Uuid) -> bool {
        self.mounted
            .lock()
            .expect("mounted lock")
            .contains_key(&account_id)
    }

    pub(crate) fn mounted_account_path(&self, account_id: Uuid) -> Option<PathBuf> {
        self.mounted
            .lock()
            .expect("mounted lock")
            .get(&account_id)
            .map(|m| m.path.clone())
    }

    pub(crate) fn reconcile_account_placeholders(&self, account_id: Uuid, rel_dirs: &[PathBuf]) {
        if let Some(path) = self.mounted_account_path(account_id) {
            self.backend.reconcile_placeholders(&path, rel_dirs);
        }
    }

    pub(crate) fn dehydrate_path(&self, path: &std::path::Path) {
        self.backend.dehydrate_path(path);
    }

    pub(crate) fn ensure_runtime(&self) -> Result<Arc<tokio::runtime::Runtime>, String> {
        let mut guard = self.runtime.lock().expect("runtime lock");
        if let Some(rt) = guard.as_ref() {
            return Ok(Arc::clone(rt));
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(str_err)?;
        let arc = Arc::new(rt);
        *guard = Some(Arc::clone(&arc));
        Ok(arc)
    }

    pub fn sync_now(
        &self,
        ctrl: &Arc<Mutex<AppController>>,
        engine: &Arc<SyncEngine>,
        secrets: &Arc<dyn SecretStore>,
    ) -> Result<(), String> {
        let _sync_guard = self.sync_mutex.lock().expect("mount sync lock");
        let (accounts, creds, settings) = {
            let g = ctrl.lock().expect("controller lock");
            (
                g.accounts.list_accounts().to_vec(),
                g.accounts.list_credentials().to_vec(),
                g.settings.clone(),
            )
        };
        let root = self.mount_root()?;
        self.unmount_stale(&accounts);
        let initial_quota = virtual_drive::compute_accounts_quota(&accounts);
        let mut first_err = self
            .sync_virtual_drive(&settings, initial_quota, &root)
            .err();
        let targets: Vec<Account> = accounts
            .into_iter()
            .filter(|a| a.enabled && a.status == AccountStatus::Connected)
            .collect();
        if targets.is_empty() {
            return first_err.map_or(Ok(()), Err);
        }
        let rt = self.ensure_runtime()?;
        {
            let mut started = self.worker_started.lock().expect("worker lock");
            if !*started {
                let _g = rt.enter();
                engine.spawn_worker(Duration::from_secs(5));
                *started = true;
            }
        }
        for acct in targets {
            if self
                .mounted
                .lock()
                .expect("mounted lock")
                .contains_key(&acct.id)
            {
                continue;
            }
            if let Err(err) = self.mount_one(acct, &root, &creds, ctrl, engine, secrets, &rt) {
                first_err.get_or_insert(err);
            }
        }
        if self
            .needs_refresh
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            let fresh_accounts = ctrl
                .lock()
                .expect("controller lock")
                .accounts
                .list_accounts()
                .to_vec();
            let fresh_quota = virtual_drive::compute_accounts_quota(&fresh_accounts);
            if fresh_quota != initial_quota {
                let _ = self.sync_virtual_drive(&settings, fresh_quota, &root);
            }
        }
        first_err.map_or(Ok(()), Err)
    }
    /// Disconnects and unregisters all mounted OS sync roots and clears virtual drive visibility.
    pub fn shutdown_all(&self) {
        let mut had_mounts = false;
        if let Ok(mut mounted) = self.mounted.lock() {
            let entries: Vec<MountedAccount> = mounted.drain().map(|(_, v)| v).collect();
            drop(mounted);
            had_mounts = !entries.is_empty();
            for entry in entries {
                let path = entry.path.clone();
                drop(entry);
                self.backend.unregister_account(&path);
            }
        }
        if had_mounts {
            self.backend.unregister_all_accounts();
        }
        if let Ok(mut vd) = self.last_virtual_drive.lock() {
            if let Some((_, letter, _)) = vd.take() {
                drop(vd);
                if let Ok(root) = self.mount_root() {
                    let _ = self.backend.sync_this_pc_namespace(false, &root, None);
                    let _ = self.backend.cleanup_legacy_subst(&letter, &root);
                }
            }
        }
        if let Ok(mut guard) = self.runtime.lock() {
            if let Some(rt) = guard.take() {
                if let Ok(rt) = Arc::try_unwrap(rt) {
                    rt.shutdown_background();
                }
            }
        }
    }
}

impl Drop for MountManager {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

#[cfg(test)]
mod tests {
    use super::live::spawn_watchdog_with;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn spawn_watchdog_with_covers_all_paths() {
        let t = std::env::temp_dir().join(format!("od-wd-{}", uuid::Uuid::new_v4()));
        let (pid, exe, deps) = (t.join("s/p"), t.join("a"), t.join("deps/t"));
        let st = AtomicBool::new(false);
        for (p, l, dis, ex, ok) in [
            (Some(pid.as_path()), None, true, Some(exe.as_path()), true),
            (Some(pid.as_path()), None, false, None, true),
            (None, None, false, Some(deps.as_path()), true),
            (None, None, false, Some(exe.as_path()), false),
            (None, Some("Z"), false, Some(exe.as_path()), true),
            (None, None, false, Some(exe.as_path()), true),
        ] {
            let bg = |_: &std::path::Path, _: &[&str]| {
                ok.then_some(()).ok_or_else(|| std::io::Error::other("e"))
            };
            spawn_watchdog_with(p, (&t, l), (dis, ex, &st), || "Q".into(), bg);
        }
        assert!(st.load(Ordering::SeqCst) && pid.is_file());
        super::MountManager::new_null().dehydrate_path(&t);
        let _ = std::fs::remove_dir_all(&t);
    }
}
