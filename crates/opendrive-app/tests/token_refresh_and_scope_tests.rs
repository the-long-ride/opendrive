//! Tests for Explorer My Drive / Shared with me dropdown switcher, TokenRefresher mount wiring, and virtual drive fallback stability.

mod common_auth_helpers;
mod support;

use common_auth_helpers::RecordingBackend;
use eframe::egui::{self, PointerButton};
use opendrive_app::desktop::explorer::ExplorerScope;
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::live::LiveMountBackend;
use opendrive_app::runtime::mount_manager::{MountBackend, MountManager};
use opendrive_auth::{MemorySecretStore, SecretStore};
use opendrive_core::{Account, AccountStatus, AppController, CredentialSet, Settings};
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use support::{add_acct, click_at, seed_drive, send_frame, text_pos, texts, wait_entry};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_scope_switcher_and_upload_guard_work() {
    let tmp = std::env::temp_dir().join(format!("od-scope-ui-{}", Uuid::new_v4()));
    let (cfg, mnt) = (tmp.join("cfg"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let fdir = "application/vnd.google-apps.folder";
    let drive = seed_drive(
        &[
            ("d-my", "MyDoc.txt", "text/plain", "root", b"mine"),
            ("d-sf", "SharedFolder", fdir, "external-owner", &[]),
            ("f-si", "SharedItem.txt", "text/plain", "d-sf", b"shared"),
        ],
        1024,
        4096,
    );
    drive.mark_shared_with_me("d-sf");
    let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br.with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let aid = add_acct(&bridge, &eng, "Alpha", addr, 1024, 4096);

    let mut app = ControlCenterApp::new(bridge);
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);
    app.apply_action(UiAction::OpenExplorer(aid.to_string()));
    wait_entry(&ctx, &mut app, aid, "MyDoc.txt");

    assert!(!app.explorer.sessions[&aid]
        .entries
        .iter()
        .any(|e| e.name == "Shared with me"));

    let click_text = |app: &mut ControlCenterApp, needle: &str| {
        let s = send_frame(&ctx, app, Vec::new()).shapes;
        click_at(
            &ctx,
            app,
            text_pos(&s, needle),
            PointerButton::Primary,
            Default::default(),
        );
    };

    // Open breadcrumb dropdown on "My Drive" and switch to "Shared with me".
    click_text(&mut app, "My Drive");
    click_text(&mut app, "Shared with me");
    wait_entry(&ctx, &mut app, aid, "SharedFolder");
    assert_eq!(
        app.explorer.sessions[&aid].scope,
        ExplorerScope::SharedWithMe
    );
    assert_eq!(app.explorer.sessions[&aid].breadcrumbs.len(), 1);

    // At Shared with me root, Upload is disabled (does not trigger drawer/picker).
    click_text(&mut app, "Upload");
    assert!(app.notice.is_none());

    // Enter SharedFolder -> breadcrumbs len 2, SharedItem.txt listed.
    let sf_id = app.explorer.sessions[&aid]
        .entries
        .iter()
        .find(|e| e.name == "SharedFolder")
        .unwrap()
        .id;
    app.apply_action(UiAction::ExplorerOpenItem(sf_id));
    wait_entry(&ctx, &mut app, aid, "SharedItem.txt");
    assert_eq!(app.explorer.sessions[&aid].breadcrumbs.len(), 2);

    // Switch back to My Drive via dropdown.
    click_text(&mut app, "Shared with me");
    click_text(&mut app, "My Drive");
    wait_entry(&ctx, &mut app, aid, "MyDoc.txt");
    assert_eq!(app.explorer.sessions[&aid].scope, ExplorerScope::MyDrive);

    h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn mount_manager_attaches_refresher_and_stabilizes_fallback_drive_letter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tok_calls = Arc::new(AtomicUsize::new(0));
    let tc = Arc::clone(&tok_calls);
    let (tok_url, tok_h) = rt.block_on(async move {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        let h = tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                let tc = Arc::clone(&tc);
                tokio::spawn(async move {
                    let mut b = [0u8; 2048];
                    let _ = s.read(&mut b).await;
                    let n = tc.fetch_add(1, Ordering::SeqCst);
                    let tok = if n == 0 { "expired-tok" } else { "fresh-tok" };
                    let body = format!(r#"{{"access_token":"{tok}","expires_in":3600,"token_type":"Bearer"}}"#);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        (format!("http://{a}/token"), h)
    });
    let (drive_addr, drive_h) = rt.block_on(async move {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        let h = tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 4096];
                    let n = s.read(&mut b).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&b[..n]);
                    let resp = if req.contains("Bearer fresh-tok") {
                        let body = r#"{"user":{"emailAddress":"a@x.com","displayName":"A"},"storageQuota":{"usage":"10","limit":"100"},"startPageToken":"1","files":[]}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                    } else {
                        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
                    };
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        (a, h)
    });

    let tmp = std::env::temp_dir().join(format!("od-mm-fb-{}", Uuid::new_v4()));
    let fb = Arc::new(RecordingBackend::default());
    let mounts = MountManager::new(Arc::clone(&fb) as Arc<dyn MountBackend>)
        .with_mount_root(tmp.clone())
        .with_token_url(tok_url)
        .with_drive_urls(
            format!("http://{drive_addr}/drive/v3"),
            format!("http://{drive_addr}/upload/drive/v3"),
        );
    let ctrl = Arc::new(Mutex::new(AppController::new(Settings {
        virtual_drive_enabled: true,
        virtual_drive_letter: "O".into(),
        ..Default::default()
    })));
    let (aid, cid) = (Uuid::new_v4(), Uuid::new_v4());
    {
        let mut g = ctrl.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cid,
            label: "c".into(),
            client_id: "id".into(),
        });
        g.accounts.add_account(Account {
            id: aid,
            nickname: "N".into(),
            email: "n@x.com".into(),
            credential_set_id: cid,
            folder_name: "N".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::Connected,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
    }
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
    secrets.store_refresh_token(aid, "rt-1").unwrap();
    secrets.store_client_secret(cid, "sec").unwrap();
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));

    // First sync mints expired-tok, about() gets 401 -> AccountTokenRefresher refreshes to fresh-tok!
    mounts.sync_now(&ctrl, &eng, &secrets).unwrap();
    assert!(tok_calls.load(Ordering::SeqCst) >= 2);
    assert_eq!(
        ctrl.lock()
            .unwrap()
            .accounts
            .get_account(aid)
            .unwrap()
            .storage_quota,
        100
    );
    // Repeated sync_now must NOT thrash This PC registration when quota is unchanged.
    let ns_after_mount = fb.this_pc_namespaces.lock().unwrap().len();
    mounts.sync_now(&ctrl, &eng, &secrets).unwrap();
    assert_eq!(fb.this_pc_namespaces.lock().unwrap().len(), ns_after_mount);
    mounts.shutdown_all();
    assert!(fb
        .this_pc_namespaces
        .lock()
        .unwrap()
        .contains(&(false, tmp.clone(), None)));

    std::fs::create_dir_all(&tmp).unwrap();
    let blocker = tmp.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    assert!(LiveMountBackend
        .set_virtual_drive(true, "O", &blocker.join("child"))
        .is_err());

    tok_h.abort();
    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
