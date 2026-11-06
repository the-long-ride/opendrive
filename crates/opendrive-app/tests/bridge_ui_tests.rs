//! Unit/integration tests for Bridge channel helpers, UI starter, and persistence edge cases.

use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::ui::{
    bridge_from_controller, build_bridge_in_dirs, build_persistent_bridge,
};
use opendrive_auth::SecretStore;
use opendrive_core::{
    load_settings, AccountStatus, AppCommand, AppController, AppEvent, DocsMode, Settings,
};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn bridge_channel_helpers_and_builders_work() {
    let (bridge, mut rx) = Bridge::new(Settings::default());
    bridge.send(AppCommand::AddAccount {
        nickname: "Demo".into(),
    });
    let events = Bridge::process_rx(&bridge.controller_handle(), &mut rx);
    assert!(matches!(events.first(), Some(AppEvent::AccountUpdated(_))));
    assert_eq!(
        bridge.with_controller(|c| c.accounts.list_accounts().len()),
        1
    );
    let pause_ev = bridge.handle_blocking(AppCommand::PauseAll);
    assert_eq!(pause_ev.len(), 1);

    let mut ctrl = AppController::new(Settings::default());
    ctrl.handle(AppCommand::AddAccount {
        nickname: "Seeded".into(),
    });
    let ui_bridge = bridge_from_controller(ctrl, None, Store::open_in_memory().unwrap()).unwrap();
    assert_eq!(ui_bridge.state_snapshot().accounts.len(), 1);

    let data_dir = std::env::temp_dir().join(format!("od-ui-dir-{}", Uuid::new_v4()));
    let persisted = build_bridge_in_dirs(
        AppController::new(Settings::default()),
        None,
        Some(&data_dir),
    )
    .unwrap();
    assert!(persisted.state_snapshot().accounts.is_empty());

    let mem_bridge =
        build_bridge_in_dirs(AppController::new(Settings::default()), None, None).unwrap();
    assert!(mem_bridge.state_snapshot().accounts.is_empty());

    let persistent = build_persistent_bridge(AppController::new(Settings::default())).unwrap();
    assert_eq!(persistent.state_snapshot().platform, std::env::consts::OS);

    let _ = std::fs::remove_dir_all(data_dir);
}

#[test]
fn bridge_accounts_settings_eviction_and_validation_work() {
    let cfg = std::env::temp_dir().join(format!("od-bridge-cfg-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cfg).unwrap();
    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(cfg.clone()));

    assert_eq!(
        bridge.add_account("   ".into(), "a@b.com".into(), "a".into(), false),
        Err("nickname must not be empty".to_string())
    );

    let connected = bridge
        .add_account(
            "Work".into(),
            "work@example.com".into(),
            "work".into(),
            false,
        )
        .unwrap();
    assert_eq!(connected.accounts.len(), 1);
    assert_eq!(connected.accounts[0].status, AccountStatus::Connected);
    let id = connected.accounts[0].id.to_string();

    let reauth = bridge
        .add_account("Pending".into(), String::new(), String::new(), true)
        .unwrap();
    assert_eq!(reauth.accounts.len(), 2);
    assert_eq!(reauth.accounts[1].status, AccountStatus::NeedsReauth);
    assert_eq!(reauth.accounts[1].folder_name, "Pending");

    let paused = bridge.account_action("pause", Some(&id)).unwrap();
    assert_eq!(paused.accounts[0].status, AccountStatus::Paused);
    let resumed = bridge.account_action("resume", Some(&id)).unwrap();
    assert_eq!(resumed.accounts[0].status, AccountStatus::Connected);
    let ro = bridge.account_action("toggle_readonly", Some(&id)).unwrap();
    assert!(ro.accounts[0].read_only);
    let all_paused = bridge.account_action("pause_all", None).unwrap();
    assert!(all_paused
        .accounts
        .iter()
        .all(|a| a.status == AccountStatus::Paused));
    let all_resumed = bridge.account_action("resume_all", None).unwrap();
    assert!(all_resumed
        .accounts
        .iter()
        .all(|a| a.status == AccountStatus::Connected));

    assert!(bridge.account_action("pause", None).is_err());
    assert!(bridge.account_action("pause", Some("   ")).is_err());
    assert!(bridge.account_action("pause", Some("not-a-uuid")).is_err());
    assert!(bridge.account_action("bogus", None).is_err());
    assert!(bridge
        .account_action("remove", Some(&Uuid::new_v4().to_string()))
        .is_err());

    let removed = bridge.account_action("remove", Some(&id)).unwrap();
    assert_eq!(removed.accounts.len(), 1);

    let updated = bridge
        .update_settings(2048, Some(5_000_000), false, DocsMode::ExportOffice)
        .unwrap();
    assert_eq!(updated.settings.cache_max_size, 2048);
    assert_eq!(updated.settings.bandwidth_limit, Some(5_000_000));
    assert_eq!(updated.settings.docs_mode, DocsMode::ExportOffice);
    assert_eq!(load_settings(&cfg).unwrap().cache_max_size, 2048);

    {
        let store = bridge.engine().store().lock();
        let nid = store.next_id().unwrap();
        store
            .upsert_node(&Node {
                id: nid,
                account_id: Uuid::new_v4(),
                drive_id: "d1".into(),
                parent_id: Some(0),
                name: "f.bin".into(),
                display_name: "f.bin".into(),
                kind: "file".into(),
                mime: None,
                size: 512,
                md5: None,
                modified_ms: 0,
                head_revision: None,
                trashed: false,
                shared_drive_id: None,
                state: NodeState::Cached,
                pinned: false,
                last_access_ms: 1,
            })
            .unwrap();
        store
            .set_cache_entry(nid, "/tmp/nonexistent.bin", 512, false)
            .unwrap();
    }
    let after_evict = bridge.evict_cache(0).unwrap();
    assert_eq!(after_evict.stats.cache_bytes, 0);

    let _ = std::fs::remove_dir_all(cfg);
}

#[test]
fn bridge_returns_errors_when_config_dir_is_unwritable() {
    let blocker = std::env::temp_dir().join(format!("od-blocker-{}", Uuid::new_v4()));
    std::fs::write(&blocker, b"not-a-dir").unwrap();
    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(blocker.clone()));

    assert!(bridge
        .add_account("Test".into(), "t@e.com".into(), "t".into(), false)
        .is_err());
    assert!(bridge.account_action("pause_all", None).is_err());
    assert!(bridge
        .update_settings(1024, None, false, DocsMode::Link)
        .is_err());

    let _ = std::fs::remove_file(blocker);
}

#[test]
fn bridge_rename_account_and_encrypted_vault_export_import_work() {
    let dir_a = std::env::temp_dir().join(format!("od-vault-a-{}", Uuid::new_v4()));
    let dir_b = std::env::temp_dir().join(format!("od-vault-b-{}", Uuid::new_v4()));
    let store_a = Store::open_in_memory().unwrap();
    let store_b = Store::open_in_memory().unwrap();
    let secrets_a = Arc::new(opendrive_auth::MemorySecretStore::new());
    let secrets_b = Arc::new(opendrive_auth::MemorySecretStore::new());

    let (bridge_a, _rx_a) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(store_a)),
        Some(dir_a.clone()),
        secrets_a.clone(),
    );
    let cred_id = bridge_a
        .save_oauth_client_credentials("cid-vault", "secret-vault")
        .unwrap();
    let state = bridge_a
        .add_account(
            "alice".into(),
            "alice@example.com".into(),
            "Alice".into(),
            false,
        )
        .unwrap();
    let acct_id = state.accounts[0].id;
    {
        let ctrl = bridge_a.controller_handle();
        let mut g = ctrl.lock().unwrap();
        g.accounts
            .get_account_mut(acct_id)
            .unwrap()
            .credential_set_id = cred_id;
    }
    secrets_a.store_refresh_token(acct_id, "rt-vault").unwrap();
    assert_eq!(state.accounts[0].nickname, "alice");

    let (renamed, folder_changed) = bridge_a
        .rename_account(&acct_id.to_string(), "Personal Alias".into(), None)
        .unwrap();
    assert!(!folder_changed);
    assert_eq!(renamed.accounts[0].nickname, "Personal Alias");

    let (renamed_folder, folder_changed) = bridge_a
        .rename_account(
            &acct_id.to_string(),
            "Work Alias".into(),
            Some("WorkMount".into()),
        )
        .unwrap();
    assert!(folder_changed);
    assert_eq!(renamed_folder.accounts[0].folder_name, "WorkMount");

    assert!(bridge_a.export_encrypted_vault("weak").is_err());
    let encrypted = bridge_a.export_encrypted_vault("VaultPass9!").unwrap();

    let (bridge_b, _rx_b) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(store_b)),
        Some(dir_b.clone()),
        secrets_b.clone(),
    );
    assert!(bridge_b
        .import_encrypted_vault(&encrypted, "WrongPass9!")
        .is_err());
    let (imported_state, count) = bridge_b
        .import_encrypted_vault(&encrypted, "VaultPass9!")
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(imported_state.accounts.len(), 1);
    let imported_acct = &imported_state.accounts[0];
    assert_eq!(imported_acct.nickname, "Work Alias");
    assert_eq!(imported_acct.folder_name, "WorkMount");
    assert_eq!(
        secrets_b
            .load_refresh_token(imported_acct.id)
            .unwrap()
            .as_deref(),
        Some("rt-vault")
    );
    let (cid, csec) = bridge_b.saved_oauth_credentials();
    assert_eq!((cid.as_str(), csec.as_str()), ("cid-vault", "secret-vault"));

    // Add a Service Account entry, re-export, and re-import into bridge_b (exercising merge + SA secrets).
    let sa_state = bridge_a
        .add_account("sa-bot".into(), "bot@proj.iam".into(), "SaBot".into(), true)
        .unwrap();
    let sa_id = sa_state.accounts[1].id;
    secrets_a
        .store_service_account_json(sa_id, r#"{"type":"service_account"}"#)
        .unwrap();
    let encrypted2 = bridge_a.export_encrypted_vault("VaultPass9!").unwrap();
    let (merged, count2) = bridge_b
        .import_encrypted_vault(&encrypted2, "VaultPass9!")
        .unwrap();
    assert_eq!(count2, 2);
    let merged_sa = merged
        .accounts
        .iter()
        .find(|a| a.email == "bot@proj.iam")
        .unwrap();
    assert_eq!(
        secrets_b
            .load_service_account_json(merged_sa.id)
            .unwrap()
            .as_deref(),
        Some(r#"{"type":"service_account"}"#)
    );

    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}
