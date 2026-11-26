//! Synchronous UI <-> controller + sync engine bridge.

use crate::runtime::bridge_types::{engine_stats_sync, str_err};
pub use crate::runtime::bridge_types::{AppStateResponse, StatsSnapshot};
use crate::runtime::media_server::MediaStreamServer;
use crate::runtime::mount_manager::MountManager;
use opendrive_auth::{MemorySecretStore, SecretStore};
use opendrive_core::{
    Account, AccountStatus, AppCommand, AppController, AppEvent, DocsMode, Settings,
};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone)]
pub struct Bridge {
    pub(crate) ctrl: Arc<Mutex<AppController>>,
    pub(crate) engine: Arc<SyncEngine>,
    pub(crate) config_dir: Option<PathBuf>,
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) mounts: Arc<MountManager>,
    pub(crate) media_server: Arc<Mutex<Option<MediaStreamServer>>>,
    cmd_tx: mpsc::UnboundedSender<AppCommand>,
}

impl Bridge {
    pub fn new_with_secrets(
        settings: Settings,
        engine: Arc<SyncEngine>,
        config_dir: Option<PathBuf>,
        secrets: Arc<dyn SecretStore>,
    ) -> (Self, mpsc::UnboundedReceiver<AppCommand>) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let ctrl = Arc::new(Mutex::new(AppController::new(settings)));
        (
            Self {
                ctrl,
                engine,
                config_dir,
                data_dir: None,
                secrets,
                mounts: Arc::new(MountManager::new_null()),
                media_server: Arc::new(Mutex::new(None)),
                cmd_tx,
            },
            cmd_rx,
        )
    }

    pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
        self.data_dir = Some(data_dir);
        self
    }

    pub fn data_dir(&self) -> Option<&std::path::Path> {
        self.data_dir.as_deref()
    }

    pub fn with_mount_manager(mut self, mounts: Arc<MountManager>) -> Self {
        self.mounts = mounts;
        self
    }

    pub fn mounts(&self) -> &Arc<MountManager> {
        &self.mounts
    }

    pub fn new_with_engine(
        settings: Settings,
        engine: Arc<SyncEngine>,
        config_dir: Option<PathBuf>,
    ) -> (Self, mpsc::UnboundedReceiver<AppCommand>) {
        Self::new_with_secrets(
            settings,
            engine,
            config_dir,
            Arc::new(MemorySecretStore::new()),
        )
    }

    pub fn new(settings: Settings) -> (Self, mpsc::UnboundedReceiver<AppCommand>) {
        let store = Store::open_in_memory().expect("in-memory store");
        let engine = Arc::new(SyncEngine::new(store));
        Self::new_with_engine(settings, engine, None)
    }

    pub fn send(&self, cmd: AppCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn with_controller<R>(&self, f: impl FnOnce(&AppController) -> R) -> R {
        let g = self.ctrl.lock().expect("controller lock");
        f(&g)
    }

    pub fn handle_blocking(&self, cmd: AppCommand) -> Vec<AppEvent> {
        let mut g = self.ctrl.lock().expect("controller lock");
        g.handle(cmd)
    }

    /// Drain commands and apply (call from worker or UI tick).
    pub fn process_rx(
        ctrl: &Arc<Mutex<AppController>>,
        rx: &mut mpsc::UnboundedReceiver<AppCommand>,
    ) -> Vec<AppEvent> {
        let mut out = Vec::new();
        while let Ok(cmd) = rx.try_recv() {
            let mut g = ctrl.lock().expect("controller lock");
            out.extend(g.handle(cmd));
        }
        out
    }

    pub fn controller_handle(&self) -> Arc<Mutex<AppController>> {
        self.ctrl.clone()
    }

    pub fn engine(&self) -> &Arc<SyncEngine> {
        &self.engine
    }

    pub fn secrets(&self) -> &Arc<dyn SecretStore> {
        &self.secrets
    }

    pub fn state_snapshot(&self) -> AppStateResponse {
        let stats = engine_stats_sync(&self.engine);
        let cache_bytes = self
            .engine
            .store()
            .lock()
            .cache_bytes_total()
            .unwrap_or(0)
            .max(0) as u64;
        let mount_root = self
            .mounts
            .mount_root()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/OpenDrive".into());
        let (accounts, settings) = {
            let g = self.ctrl.lock().expect("controller lock");
            (g.accounts.list_accounts().to_vec(), g.settings.clone())
        };
        AppStateResponse {
            accounts,
            settings,
            stats: StatsSnapshot {
                transfers: stats.transfers,
                queue_depth: stats.queue_depth,
                errors: stats.errors,
                cache_bytes,
            },
            platform: std::env::consts::OS.to_string(),
            mount_root,
        }
    }

    pub fn add_account(
        &self,
        nickname: String,
        email: String,
        folder_name: String,
        read_only: bool,
    ) -> Result<AppStateResponse, String> {
        let nickname = nickname.trim().to_string();
        if nickname.is_empty() {
            return Err("nickname must not be empty".into());
        }
        let email = email.trim().to_string();
        let folder = if folder_name.trim().is_empty() {
            nickname.clone()
        } else {
            folder_name.trim().to_string()
        };
        let status = if email.is_empty() {
            AccountStatus::NeedsReauth
        } else {
            AccountStatus::Connected
        };
        let id = Uuid::new_v4();
        let account = Account {
            id,
            nickname,
            email,
            credential_set_id: Uuid::nil(),
            folder_name: folder.clone(),
            read_only,
            enabled: true,
            status,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        };
        {
            let mut g = self.ctrl.lock().expect("controller lock");
            g.accounts.add_account(account);
            if let Some(dir) = &self.config_dir {
                g.accounts.save_to(dir).map_err(str_err)?;
            }
        }
        self.engine
            .seed_root_account(id, &folder)
            .map_err(str_err)?;
        Ok(self.state_snapshot())
    }

    pub fn account_action(
        &self,
        action: &str,
        id_opt: Option<&str>,
    ) -> Result<AppStateResponse, String> {
        let mut removed_id = None;
        {
            let mut g = self.ctrl.lock().expect("controller lock");
            match action {
                "pause_all" => {
                    g.handle(AppCommand::PauseAll);
                }
                "resume_all" => {
                    g.handle(AppCommand::ResumeAll);
                }
                "pause" | "resume" | "remove" | "toggle_readonly" | "move_up" | "move_down" => {
                    let raw = id_opt
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| "account id is required".to_string())?;
                    let id = Uuid::parse_str(raw).map_err(|_| "invalid account id".to_string())?;
                    let current = g
                        .accounts
                        .get_account(id)
                        .map_err(|_| "account not found".to_string())?
                        .read_only;
                    let cmd = match action {
                        "pause" => AppCommand::PauseAccount { id },
                        "resume" => AppCommand::ResumeAccount { id },
                        "move_up" => AppCommand::MoveAccount { id, delta: -1 },
                        "move_down" => AppCommand::MoveAccount { id, delta: 1 },
                        "remove" => {
                            removed_id = Some(id);
                            AppCommand::RemoveAccount { id }
                        }
                        _ => AppCommand::SetReadOnly {
                            id,
                            read_only: !current,
                        },
                    };
                    g.handle(cmd);
                }
                _ => return Err(format!("unknown action: {action}")),
            }
            if let Some(dir) = &self.config_dir {
                g.accounts.save_to(dir).map_err(str_err)?;
            }
        }
        if let Some(id) = removed_id {
            self.secrets.delete_account_secrets(id);
        }
        Ok(self.state_snapshot())
    }

    pub fn update_settings(
        &self,
        cache_max_size: u64,
        bandwidth_limit: Option<u64>,
        autostart: bool,
        docs_mode: DocsMode,
    ) -> Result<AppStateResponse, String> {
        self.modify_settings(
            |s| {
                s.cache_max_size = cache_max_size;
                s.bandwidth_limit = bandwidth_limit;
                s.autostart = autostart;
                s.docs_mode = docs_mode;
            },
            |s| {
                self.engine.set_bandwidth_limit(s.bandwidth_limit);
                opendrive_platform::set_autostart(s.autostart).map_err(str_err)
            },
        )
    }

    pub fn evict_cache(&self, max_bytes: u64) -> Result<AppStateResponse, String> {
        self.engine.evict_to(max_bytes).map_err(str_err)?;
        Ok(self.state_snapshot())
    }
}
