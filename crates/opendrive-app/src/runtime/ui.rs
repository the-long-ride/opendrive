//! UI entrypoints (pure-Rust desktop Control Center + optional tray runner).

use crate::runtime::bridge::Bridge;
use crate::runtime::mount_manager::MountManager;
use crate::shell::window;
use opendrive_auth::{MemorySecretStore, OsKeyringStore, SecretStore};
use opendrive_core::AppController;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn bridge_from_controller_with_secrets(
    ctrl: AppController,
    config_dir: Option<PathBuf>,
    store: Store,
    secrets: Arc<dyn SecretStore>,
) -> anyhow::Result<Bridge> {
    let engine = Arc::new(SyncEngine::new(store));
    engine.set_bandwidth_limit(ctrl.settings.bandwidth_limit);
    let (bridge, mut cmd_rx) =
        Bridge::new_with_secrets(ctrl.settings.clone(), engine, config_dir, secrets);
    {
        let handle = bridge.controller_handle();
        let mut g = handle.lock().expect("controller lock");
        for cred in ctrl.accounts.list_credentials() {
            g.accounts.add_credential(cred.clone());
        }
        for acct in ctrl.accounts.list_accounts() {
            g.accounts.add_account(acct.clone());
            let _ = bridge
                .engine()
                .seed_root_account(acct.id, &acct.folder_name);
        }
    }
    let _ = Bridge::process_rx(&bridge.controller_handle(), &mut cmd_rx);
    Ok(bridge)
}

pub fn bridge_from_controller(
    ctrl: AppController,
    config_dir: Option<PathBuf>,
    store: Store,
) -> anyhow::Result<Bridge> {
    bridge_from_controller_with_secrets(ctrl, config_dir, store, Arc::new(MemorySecretStore::new()))
}

pub fn build_bridge_in_dirs(
    ctrl: AppController,
    config_dir: Option<PathBuf>,
    data_dir: Option<&Path>,
) -> anyhow::Result<Bridge> {
    build_bridge_in_dirs_with_secrets(
        ctrl,
        config_dir,
        data_dir,
        Arc::new(MemorySecretStore::new()),
    )
}

pub fn build_bridge_in_dirs_with_secrets(
    ctrl: AppController,
    config_dir: Option<PathBuf>,
    data_dir: Option<&Path>,
    secrets: Arc<dyn SecretStore>,
) -> anyhow::Result<Bridge> {
    let store = match data_dir {
        Some(dir) => {
            std::fs::create_dir_all(dir)?;
            let db = dir.join("meta.db");
            Store::open(db.to_str().unwrap_or("meta.db"))?
        }
        None => Store::open_in_memory()?,
    };
    let mut bridge = bridge_from_controller_with_secrets(ctrl, config_dir, store, secrets)?;
    if let Some(dir) = data_dir {
        bridge = bridge.with_data_dir(dir.to_path_buf());
    }
    Ok(bridge)
}

pub fn build_persistent_bridge(ctrl: AppController) -> anyhow::Result<Bridge> {
    let config_dir = opendrive_platform::config_dir().ok();
    let _ = opendrive_platform::ensure_dirs();
    let data_dir = opendrive_platform::data_dir().ok();
    let bridge = build_bridge_in_dirs_with_secrets(
        ctrl,
        config_dir,
        data_dir.as_deref(),
        Arc::new(OsKeyringStore),
    )?;
    Ok(bridge.with_mount_manager(Arc::new(MountManager::new_live())))
}

pub fn run_desktop_ui(ctrl: AppController) -> anyhow::Result<()> {
    let bridge = build_persistent_bridge(ctrl)?;
    window::run_native_window(bridge)
}

#[cfg(feature = "tray")]
pub fn run_tray_loop(ctrl: AppController) -> anyhow::Result<()> {
    let bridge = build_persistent_bridge(ctrl)?;
    crate::shell::tray_loop::run_tray(bridge)
}
