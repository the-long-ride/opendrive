//! End-to-end tests for MountManager, token refresh hydration, unmount lifecycle, and virtual drive errors.

mod common_auth_helpers;

use common_auth_helpers::{mock_token_server, sample_service_account_json, RecordingBackend};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::live::LiveMountBackend;
use opendrive_app::runtime::mount_manager::{MountBackend, MountManager, NullMountBackend};
use opendrive_auth::{MemorySecretStore, SecretStore, DEFAULT_TOKEN_URI};
use opendrive_core::{
    load_settings, Account, AccountStatus, AppCommand, AppController, CredentialSet, Settings,
};
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::Store;
use opendrive_sync::{SyncEngine, VfsEngine};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn make_account(id: Uuid, cred_id: Uuid, folder: &str, status: AccountStatus) -> Account {
    Account {
        id,
        nickname: folder.into(),
        email: format!("{folder}@example.com"),
        credential_set_id: cred_id,
        folder_name: folder.into(),
        read_only: false,
        enabled: status == AccountStatus::Connected,
        status,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    }
}

#[test]
fn mount_manager_hydrates_oauth_and_service_accounts_and_unmounts_on_pause_remove() {
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let drive = Arc::new(FakeDrive::new());
    drive.add_file(FakeFile {
        id: "doc-1".into(),
        name: "Roadmap.txt".into(),
        mime_type: "text/plain".into(),
        size: 11,
        parents: vec!["root".into()],
        content: b"hello drive".to_vec(),
        ..Default::default()
    });
    let (addr, drive_h) = server_rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));
    let (token_url, tok_h) = server_rt.block_on(mock_token_server(
        r#"{"access_token":"at-refreshed","expires_in":3600}"#,
    ));
    let api_base = format!("http://{addr}/drive/v3");
    let upload_base = format!("http://{addr}/upload/drive/v3");

    let mount_root = std::env::temp_dir().join(format!("od-mm-root-{}", Uuid::new_v4()));
    let backend = Arc::new(RecordingBackend::default());
    let mounts = MountManager::new(Arc::clone(&backend) as Arc<dyn MountBackend>)
        .with_mount_root(mount_root.clone())
        .with_token_url(token_url)
        .with_drive_urls(api_base, upload_base);

    let secrets_impl = Arc::new(MemorySecretStore::new());
    let secrets: Arc<dyn SecretStore> = secrets_impl.clone();
    let cred_id = Uuid::new_v4();
    let oauth_id = Uuid::new_v4();
    let sa_id = Uuid::new_v4();
    let paused_id = Uuid::new_v4();
    secrets.store_client_secret(cred_id, "csec-mm").unwrap();
    secrets.store_refresh_token(oauth_id, "rt-mm").unwrap();
    secrets
        .store_service_account_json(sa_id, &sample_service_account_json(DEFAULT_TOKEN_URI))
        .unwrap();

    let ctrl = Arc::new(Mutex::new(AppController::new(Settings::default())));
    {
        let mut g = ctrl.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cred_id,
            label: "Desktop".into(),
            client_id: "cid-mm".into(),
        });
        for (id, cid, dir, st) in [
            (oauth_id, cred_id, "Work", AccountStatus::Connected),
            (sa_id, Uuid::nil(), "Bot", AccountStatus::Connected),
            (paused_id, Uuid::nil(), "Paused", AccountStatus::Paused),
        ] {
            g.accounts.add_account(make_account(id, cid, dir, st));
        }
    }

    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));

    mounts.sync_now(&ctrl, &engine, &secrets).unwrap();
    let mut ids = mounts.mounted_ids();
    ids.sort();
    let mut expected = vec![oauth_id, sa_id];
    expected.sort();
    assert_eq!(ids, expected);
    assert!(engine.drive_client_for(oauth_id).is_ok());
    assert!(engine.drive_client_for(sa_id).is_ok());
    assert!(engine.drive_client_for(paused_id).is_err());
    assert!(
        ctrl.lock()
            .unwrap()
            .accounts
            .get_account(oauth_id)
            .unwrap()
            .storage_quota
            > 0
    );

    let oauth_root = engine.seed_root_account(oauth_id, "Work").unwrap();
    let page = server_rt.block_on(engine.list(oauth_root, None)).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(page.entries.iter().any(|e| e.name == "Roadmap.txt"));
    assert!(!page.entries.iter().any(|e| e.name == "Shared with me"));
    assert!(engine.seed_shared_with_me_root(oauth_id).is_ok());

    mounts.sync_now(&ctrl, &engine, &secrets).unwrap();
    assert_eq!(backend.connected.lock().unwrap().len(), 2);

    {
        let mut g = ctrl.lock().unwrap();
        g.handle(AppCommand::PauseAccount { id: oauth_id });
        g.accounts.get_account_mut(sa_id).unwrap().folder_name = "BotRenamed".into();
    }
    mounts.sync_now(&ctrl, &engine, &secrets).unwrap();
    assert_eq!(mounts.mounted_ids(), vec![sa_id]);
    let unreg = backend.unregistered.lock().unwrap().clone();
    assert!(unreg.contains(&mount_root.join("Work")));
    assert!(unreg.contains(&mount_root.join("Bot")));

    {
        let mut g = ctrl.lock().unwrap();
        g.handle(AppCommand::RemoveAccount { id: sa_id });
    }
    mounts.sync_now(&ctrl, &engine, &secrets).unwrap();
    assert!(mounts.mounted_ids().is_empty());
    assert!(backend
        .unregistered
        .lock()
        .unwrap()
        .contains(&mount_root.join("BotRenamed")));

    let missing_id = Uuid::new_v4();
    ctrl.lock().unwrap().accounts.add_account(make_account(
        missing_id,
        Uuid::nil(),
        "Missing",
        AccountStatus::Connected,
    ));
    assert!(mounts.sync_now(&ctrl, &engine, &secrets).is_err());
    assert!(mounts.mounted_ids().is_empty());
    assert_eq!(
        ctrl.lock()
            .unwrap()
            .accounts
            .get_account(missing_id)
            .unwrap()
            .status,
        AccountStatus::NeedsReauth
    );

    tok_h.abort();
    drive_h.abort();
}

#[test]
fn virtual_drive_persistence_error_notices_and_live_backend_surface() {
    let cfg = std::env::temp_dir().join(format!("od-mm-cfg-{}", Uuid::new_v4()));
    let mount_root = std::env::temp_dir().join(format!("od-mm-vd-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cfg).unwrap();

    let backend = Arc::new(RecordingBackend::default());
    let mounts = Arc::new(
        MountManager::new(Arc::clone(&backend) as Arc<dyn MountBackend>)
            .with_mount_root(mount_root.clone()),
    );
    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(cfg.clone()));
    let bridge = bridge.with_mount_manager(Arc::clone(&mounts));

    let sync_bridge = |b: &Bridge| {
        mounts
            .sync_now(&b.controller_handle(), b.engine(), b.secrets())
            .unwrap();
    };
    let state = bridge.update_virtual_drive(true).unwrap();
    assert!(state.settings.virtual_drive_enabled);
    assert!(load_settings(&cfg).unwrap().virtual_drive_enabled);

    sync_bridge(&bridge);
    assert!(mounts.is_virtual_drive_active());
    assert!(backend
        .this_pc_namespaces
        .lock()
        .unwrap()
        .contains(&(true, mount_root.clone(), None)));
    mounts.shutdown_all();
    assert!(!mounts.is_virtual_drive_active());
    assert!(load_settings(&cfg).unwrap().virtual_drive_enabled);
    assert_eq!(
        backend.this_pc_namespaces.lock().unwrap().last(),
        Some(&(false, mount_root.clone(), None))
    );
    sync_bridge(&bridge);

    bridge
        .controller_handle()
        .lock()
        .unwrap()
        .settings
        .virtual_drive_letter = "P".into();
    sync_bridge(&bridge);
    let vds = backend.virtual_drives.lock().unwrap().clone();
    assert!(!vds.iter().any(|(on, _, _)| *on));
    assert!(vds.iter().any(|(on, letter, _)| !*on && letter == "O"));
    assert!(vds.iter().any(|(on, letter, _)| !*on && letter == "P"));

    let state = bridge.update_virtual_drive(false).unwrap();
    assert!(!state.settings.virtual_drive_enabled);
    sync_bridge(&bridge);
    assert!(!mounts.is_virtual_drive_active());
    assert!(backend.this_pc_namespaces.lock().unwrap().contains(&(
        false,
        mount_root.clone(),
        None
    )));

    bridge
        .controller_handle()
        .lock()
        .unwrap()
        .accounts
        .add_account(make_account(
            Uuid::new_v4(),
            Uuid::nil(),
            "NoSec",
            AccountStatus::Connected,
        ));
    bridge.spawn_mount_sync(None);
    let deadline = Instant::now() + Duration::from_secs(3);
    let notice = loop {
        if let Some(n) = bridge.take_mount_notice() {
            break n;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for mount notice"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(notice.0 && !notice.1.is_empty());

    let null_backend = NullMountBackend;
    null_backend.prepare_root(&mount_root).unwrap();
    assert!(null_backend
        .set_virtual_drive(false, "C", &mount_root)
        .is_err());

    let _live_mgr = MountManager::new_live();
    assert!(!bridge.mounts().is_virtual_drive_active());
    let live = LiveMountBackend;
    let live_dir = std::env::temp_dir().join(format!("od-live-dir-{}", Uuid::new_v4()));
    live.prepare_root(&live_dir).unwrap();
    assert!(live_dir.is_dir());
    std::fs::write(live_dir.join("f.bin"), b"x").unwrap();
    live.dehydrate_path(&live_dir);
    live.unregister_account(&live_dir);
    assert!(live.set_virtual_drive(true, "C", &live_dir).is_err());
    assert!(live.cleanup_legacy_subst("C", &live_dir).is_err());
    let _ = live.set_virtual_drive(false, "C", &live_dir);
    let _ = live.cleanup_legacy_subst("O", &live_dir);

    let blocker = std::env::temp_dir().join(format!("od-live-blk-{}", Uuid::new_v4()));
    std::fs::write(&blocker, b"x").unwrap();
    assert!(live
        .sync_this_pc_namespace(true, &blocker.join("child"), None)
        .is_err());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let acct = make_account(Uuid::new_v4(), Uuid::nil(), "Blk", AccountStatus::Connected);
    let vfs: Arc<dyn VfsEngine> = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    assert!(live
        .connect_account(&acct, &blocker.join("child"), 1, vfs, rt.handle())
        .is_err());

    let _ = std::fs::remove_file(blocker);
    let _ = std::fs::remove_dir_all(live_dir);
    let _ = std::fs::remove_dir_all(cfg);
}
