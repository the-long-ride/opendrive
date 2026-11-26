//! UI entrypoints (pure-Rust desktop Control Center + optional tray runner).

use crate::runtime::bridge::Bridge;
use crate::runtime::mount_manager::MountManager;
use crate::shell::window;
use opendrive_auth::{MemorySecretStore, OsKeyringStore, SecretStore};
use opendrive_core::{load_settings, AppController};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn load_controller() -> AppController {
    let settings = opendrive_platform::config_dir()
        .ok()
        .and_then(|d| load_settings(&d).ok())
        .unwrap_or_default();
    let mut ctrl = AppController::new(settings);
    if let Ok(dir) = opendrive_platform::config_dir() {
        let _ = ctrl.accounts.load_from(&dir);
    }
    ctrl
}

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
        bridge
            .engine()
            .set_stream_cache_dir(dir.join("stream_cache"))
            .map_err(anyhow::Error::msg)?;
        bridge = bridge.with_data_dir(dir.to_path_buf());
    }
    Ok(bridge)
}

pub fn build_persistent_bridge(ctrl: AppController) -> anyhow::Result<Bridge> {
    let vd_enabled = ctrl.settings.virtual_drive_enabled;
    let vd_letter = ctrl.settings.virtual_drive_letter.clone();
    let config_dir = opendrive_platform::config_dir().ok();
    let _ = opendrive_platform::ensure_dirs();
    let data_dir = opendrive_platform::data_dir().ok();
    let bridge = build_bridge_in_dirs_with_secrets(
        ctrl,
        config_dir,
        data_dir.as_deref(),
        Arc::new(OsKeyringStore),
    )?;
    if !crate::runtime::mount_manager::live::is_foreign_mount_owner_alive() {
        if let Ok(root) = opendrive_platform::mount_root() {
            crate::runtime::mount_manager::live::install_process_mount_guards(
                &root,
                Some(&vd_letter),
            );
            if !vd_enabled {
                let _ = opendrive_platform::sync_this_pc_namespace(false, &root, None, None);
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_core::{Account, AccountStatus, CredentialSet, OpenWithRule, Settings};
    use opendrive_store::NodeState;
    use uuid::Uuid;

    #[test]
    fn bridge_builder_and_open_target_cached_branches() {
        let tmp = std::env::temp_dir().join(format!("od-ui-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut ctrl = AppController::new(Settings::default());
        let cid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        ctrl.accounts.add_credential(CredentialSet {
            id: cid,
            label: "L".into(),
            client_id: "c".into(),
        });
        ctrl.accounts.add_account(Account {
            id: aid,
            nickname: "N".into(),
            email: "n@x.com".into(),
            credential_set_id: cid,
            folder_name: "n".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::Connected,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
        let br = build_bridge_in_dirs(ctrl, None, Some(&tmp)).unwrap();
        let root = br.explorer_root_node(aid).unwrap();
        let st = br.engine().store();
        let mut base = st.lock().get_node(root as i64).unwrap().unwrap();
        (base.id, base.parent_id, base.kind, base.state, base.size) =
            (0, Some(root as i64), "file".into(), NodeState::Cached, 4);

        let vid_p = tmp.join("v.mp4");
        std::fs::write(&vid_p, b"1234").unwrap();
        (base.drive_id, base.name, base.display_name) =
            ("v1".into(), "v.mp4".into(), String::new());
        let vid_id = st.lock().upsert_node(&base).unwrap() as u64;
        st.lock()
            .set_cache_entry(vid_id as i64, vid_p.to_str().unwrap(), 4, false)
            .unwrap();
        assert!(
            !br.explorer_open_item(aid, vid_id, true)
                .unwrap()
                .is_stream_url
        );

        let doc_p = tmp.join("d.txt");
        std::fs::write(&doc_p, b"abcd").unwrap();
        (base.drive_id, base.name) = ("d1".into(), "d.txt".into());
        let doc_id = st.lock().upsert_node(&base).unwrap() as u64;
        st.lock()
            .set_cache_entry(doc_id as i64, doc_p.to_str().unwrap(), 4, false)
            .unwrap();
        assert_eq!(
            br.explorer_open_item(aid, doc_id, true).unwrap().target,
            doc_p.to_string_lossy()
        );

        br.update_open_with_rules(vec![OpenWithRule {
            pattern: "*".into(),
            app_command: "opendrive-missing-bin-xyz".into(),
        }])
        .unwrap();
        assert!(br.explorer_open_item(aid, doc_id, false).is_err());

        (base.drive_id, base.kind, base.name) = ("nd1".into(), "native_doc".into(), "doc".into());
        let nd_id = st.lock().upsert_node(&base).unwrap() as u64;
        assert!(br.explorer_stream_url(aid, nd_id).is_err());

        drop(br);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
