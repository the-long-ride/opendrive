//! Coverage tests for mount_manager (live, backend, virtual_drive, sync_ops, mod) and release_check.

mod common_auth_helpers;

use common_auth_helpers::{
    mock_token_server, sample_service_account_json, FailingSecretStore, RecordingBackend,
};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::live::{cleanup_os_mounts_for_pid, LiveMountBackend};
use opendrive_app::runtime::mount_manager::{MountBackend, MountManager, NullMountBackend};
use opendrive_auth::{MemorySecretStore, SecretStore, DEFAULT_TOKEN_URI};
use opendrive_core::{Account, AccountStatus, AppController, CredentialSet, Settings};
use opendrive_fake_drive::{FakeAbout, FakeDrive, FakeDriveServer};
use opendrive_platform::ExplorerQuota;
use opendrive_store::Store;
use opendrive_sync::{SyncEngine, VfsEngine};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

struct FailThisPcBackend;

impl MountBackend for FailThisPcBackend {
    fn prepare_root(&self, _: &Path) -> Result<(), String> {
        Ok(())
    }
    fn connect_account(
        &self,
        _: &Account,
        _: &Path,
        _: u64,
        _: Arc<dyn VfsEngine>,
        _: &tokio::runtime::Handle,
    ) -> Result<Box<dyn Any + Send + Sync>, String> {
        Ok(Box::new(()))
    }
    fn set_virtual_drive(&self, _: bool, letter: &str, _: &Path) -> Result<(), String> {
        opendrive_platform::normalize_drive_letter(letter)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn sync_this_pc_namespace(
        &self,
        _: bool,
        _: &Path,
        _: Option<ExplorerQuota>,
    ) -> Result<(), String> {
        Err("this-pc registry blocked".into())
    }
}

fn acct(id: Uuid, cid: Uuid, name: &str) -> Account {
    Account {
        id,
        nickname: name.into(),
        email: format!("{name}@example.com"),
        credential_set_id: cid,
        folder_name: name.into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    }
}

#[test]
fn live_and_null_mount_backends_and_watchdog_branches() {
    let tmp = std::env::temp_dir().join(format!("od-rt-live-{}", Uuid::new_v4()));
    let nested = tmp.join("sub/deep");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("f.bin"), b"data").unwrap();

    let null = NullMountBackend;
    null.unregister_account(&tmp);
    null.unregister_all_accounts();
    null.reconcile_placeholders(&tmp, &[PathBuf::from("sub")]);
    null.dehydrate_path(&tmp);
    null.cleanup_legacy_subst("O", &tmp).unwrap();
    null.sync_this_pc_namespace(false, &tmp, None).unwrap();

    let live = LiveMountBackend;
    std::env::set_var("OPENDRIVE_DISABLE_WATCHDOG", "1");
    let _ = live.prepare_root(&tmp);
    opendrive_app::runtime::mount_manager::live::spawn_mount_watchdog(&tmp, Some("O"));
    live.dehydrate_path(&tmp);
    live.dehydrate_path(&tmp.join("missing"));
    live.reconcile_placeholders(&tmp, &[PathBuf::from("sub")]);
    assert!(live.set_virtual_drive(true, "C", &tmp).is_err());
    assert!(live.set_virtual_drive(false, "12", &tmp).is_err());
    assert!(live.cleanup_legacy_subst("12", &tmp).is_err());
    if !cfg!(windows) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let eng: Arc<dyn VfsEngine> = Arc::new(opendrive_sync::PlaceholderEngine);
        let a = acct(Uuid::new_v4(), Uuid::new_v4(), "nix");
        let _ = live.connect_account(&a, &tmp, 1, eng, rt.handle());
        live.unregister_account(&tmp);
        live.unregister_all_accounts();
        cleanup_os_mounts_for_pid(std::process::id(), &tmp, "P");
    }
    let _ = opendrive_app::runtime::mount_manager::live::is_foreign_mount_owner_alive();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn mount_manager_sync_ops_and_virtual_drive_error_paths() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = Arc::new(FakeDrive::new());
    drive.set_about(FakeAbout {
        email: "av@example.com".into(),
        display_name: "Avatar".into(),
        storage_quota: 4096,
        storage_used: 1024,
    });
    drive.set_static_asset("/av.png", b"png-bytes".to_vec());
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));
    drive.set_photo_url(Some(format!("http://{addr}/av.png")));

    let (tok_ok, tok_ok_h) = rt.block_on(mock_token_server(
        r#"{"access_token":"at-ok","expires_in":3600}"#,
    ));
    let listener_err = rt.block_on(TcpListener::bind("127.0.0.1:0")).unwrap();
    let err_addr = listener_err.local_addr().unwrap();
    let err_h = rt.spawn(async move {
        while let Ok((mut s, _)) = listener_err.accept().await {
            let mut b = [0u8; 512];
            let _ = s.read(&mut b).await;
            let _ = s
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\n\r\n{}")
                .await;
        }
    });
    let tok_err = format!("http://{err_addr}/token");

    let tmp = std::env::temp_dir().join(format!("od-mm-cov-{}", Uuid::new_v4()));
    let backend = Arc::new(RecordingBackend::default());
    let mounts = Arc::new(
        MountManager::new(Arc::clone(&backend) as Arc<dyn MountBackend>)
            .with_mount_root(tmp.clone())
            .with_token_url(tok_ok)
            .with_drive_urls(
                format!("http://{addr}/drive/v3"),
                format!("http://{addr}/upload/drive/v3"),
            ),
    );
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
    let (cid, aid) = (Uuid::new_v4(), Uuid::new_v4());
    secrets.store_client_secret(cid, "sec").unwrap();
    secrets.store_refresh_token(aid, "rt").unwrap();
    let ctrl = Arc::new(Mutex::new(AppController::new(Settings::default())));
    {
        let mut g = ctrl.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cid,
            label: "L".into(),
            client_id: "cid".into(),
        });
        g.accounts.add_account(acct(aid, cid, "Work"));
    }
    let engine = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    mounts.sync_now(&ctrl, &engine, &secrets).unwrap();
    assert!(mounts.is_account_mounted(aid));

    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&engine), None);
    let br = br.with_mount_manager(Arc::clone(&mounts));
    assert!(br.take_mount_refresh());
    assert!(!br.take_mount_refresh());
    assert!(br.take_mount_notice().is_none());

    // Refresh error paths via Bridge::explorer_sync_down / explorer_list + MountManager::sync_now.
    let no_cred_id = Uuid::new_v4();
    let missing_cset_id = Uuid::new_v4();
    let bad_sa_id = Uuid::new_v4();
    let fail_tok_sa = Uuid::new_v4();
    {
        let mut g = ctrl.lock().unwrap();
        g.accounts
            .add_account(acct(no_cred_id, Uuid::nil(), "NoCred"));
        g.accounts
            .add_account(acct(missing_cset_id, Uuid::new_v4(), "NoCset"));
        g.accounts
            .add_account(acct(bad_sa_id, Uuid::nil(), "BadSa"));
        g.accounts
            .add_account(acct(fail_tok_sa, Uuid::nil(), "FailSa"));
    }
    secrets.store_refresh_token(missing_cset_id, "rt2").unwrap();
    secrets
        .store_service_account_json(bad_sa_id, "not-json")
        .unwrap();
    secrets
        .store_service_account_json(fail_tok_sa, &sample_service_account_json(DEFAULT_TOKEN_URI))
        .unwrap();

    let mm_err = Arc::new(
        MountManager::new(Arc::clone(&backend) as Arc<dyn MountBackend>)
            .with_mount_root(tmp.clone())
            .with_token_url(tok_err),
    );
    let engine2 = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br_err, _) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::clone(&engine2),
        None,
        Arc::clone(&secrets),
    );
    {
        let h = br_err.controller_handle();
        let mut g = h.lock().unwrap();
        for a in ctrl.lock().unwrap().accounts.list_accounts() {
            g.accounts.add_account(a.clone());
        }
        g.accounts.add_credential(CredentialSet {
            id: cid,
            label: "L".into(),
            client_id: "cid".into(),
        });
    }
    let br_err = br_err.with_mount_manager(Arc::clone(&mm_err));
    assert!(br_err.explorer_sync_down(Uuid::new_v4(), 1).is_err());
    for bad_id in [no_cred_id, missing_cset_id, bad_sa_id, fail_tok_sa, aid] {
        assert!(br_err.explorer_list(bad_id, None, true).is_err());
    }
    let fail_sec: Arc<dyn SecretStore> = Arc::new(FailingSecretStore);
    let (br_fsec, _) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::clone(&engine2),
        None,
        Arc::clone(&fail_sec),
    );
    br_fsec
        .controller_handle()
        .lock()
        .unwrap()
        .accounts
        .add_account(acct(aid, cid, "Work"));
    assert!(br_fsec
        .with_mount_manager(Arc::clone(&mm_err))
        .explorer_list(aid, None, true)
        .is_err());
    mm_err.sync_now(&ctrl, &engine2, &fail_sec).unwrap_err();
    assert_eq!(
        ctrl.lock()
            .unwrap()
            .accounts
            .get_account(aid)
            .unwrap()
            .status,
        AccountStatus::NeedsReauth
    );

    // Virtual drive same-letter enable failure & sync_this_pc_namespace error.
    let mut s_c = Settings {
        virtual_drive_enabled: false,
        virtual_drive_letter: "C".into(),
        ..Default::default()
    };
    let vd_ctrl = Arc::new(Mutex::new(AppController::new(s_c.clone())));
    let vd_mm = MountManager::new(Arc::clone(&backend) as Arc<dyn MountBackend>)
        .with_mount_root(tmp.clone());
    vd_mm.sync_now(&vd_ctrl, &engine, &secrets).unwrap();
    s_c.virtual_drive_enabled = true;
    vd_ctrl.lock().unwrap().settings = s_c;
    assert!(vd_mm
        .sync_now(&vd_ctrl, &engine, &secrets)
        .unwrap_err()
        .contains("Virtual drive C"));

    vd_ctrl.lock().unwrap().settings.virtual_drive_letter = "O".into();
    let fail_pc = FailThisPcBackend;
    assert!(fail_pc.set_virtual_drive(true, "invalid!", &tmp).is_err());
    let sub = tmp.join("dehyd_dir");
    LiveMountBackend.prepare_root(&sub).unwrap();
    std::fs::write(sub.join("f.txt"), b"x").unwrap();
    LiveMountBackend.reconcile_placeholders(&sub, &[]);
    LiveMountBackend.dehydrate_path(&sub);
    let fail_pc_mm = MountManager::new(Arc::new(fail_pc)).with_mount_root(tmp.clone());
    assert!(fail_pc_mm
        .sync_now(&vd_ctrl, &engine, &secrets)
        .unwrap_err()
        .contains("This PC entry"));
    mounts.shutdown_all();
    err_h.abort();
    tok_ok_h.abort();
    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
