//! Regression tests for code-review fixes across read-only guards, quota, open refresh, paste, and CLI aliases.

mod common_auth_helpers;
mod support;

use common_auth_helpers::{run_frame, MockDroppedFile, RecordingBackend};
use eframe::egui;
use opendrive_app::design::formatters::format_account_storage;
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::dispatch_bridge;
use opendrive_app::runtime::mount_manager::{MountBackend, MountManager};
use opendrive_core::{Account, AccountStatus, AppCommand, AppController, Settings};
use opendrive_fake_drive::{FakeDriveServer, FakeFile};
use opendrive_platform::ExplorerQuota;
use opendrive_store::{NodeState, Store};
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, eid, seed_drive, state_of, texts, wait_entry, wait_state, wait_until};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_guards_and_cli_aliases_work() {
    let tmp = std::env::temp_dir().join(format!("od-rev-ro-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let up_file = tmp.join("dropped.txt");
    std::fs::write(&up_file, b"drop").unwrap();

    let drive = seed_drive(
        &[
            (
                "d-dst",
                "Dest",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f-a", "a.txt", "text/plain", "root", b"alpha"),
        ],
        1024,
        4096,
    );
    let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br
        .with_data_dir(data)
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 1024, 4096);
    let root = bridge.explorer_root_node(a_id).unwrap();
    let (_, entries) = bridge.explorer_list(a_id, None, true).unwrap();
    let a_nid = entries.iter().find(|e| e.name == "a.txt").unwrap().id;

    bridge
        .controller_handle()
        .lock()
        .unwrap()
        .handle(AppCommand::SetReadOnly {
            id: a_id,
            read_only: true,
        });
    assert!(bridge
        .explorer_sync_up(a_id, root, std::slice::from_ref(&up_file))
        .unwrap_err()
        .contains("read-only"));
    assert!(bridge
        .explorer_sync_up_node(a_id, a_nid)
        .unwrap_err()
        .contains("read-only"));
    assert!(bridge
        .explorer_sync_up_and_clear(a_id, a_nid)
        .unwrap_err()
        .contains("read-only"));

    let mut app = ControlCenterApp::new(bridge.clone());
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "a.txt");
    let drop: egui::DroppedFileHandle = Arc::new(MockDroppedFile {
        path: up_file,
        bytes: Err("unused".into()),
    });
    run_frame(&ctx, &mut app, vec![drop]);
    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Error));
    assert!(!drive
        .list_children("root")
        .iter()
        .any(|f| f.name == "dropped.txt"));

    bridge
        .controller_handle()
        .lock()
        .unwrap()
        .handle(AppCommand::SetReadOnly {
            id: a_id,
            read_only: false,
        });
    for args in [
        vec!["copy", "Alpha", "a.txt"],
        vec!["move", "Alpha", "a (copy).txt", "Dest"],
        vec!["delete", "Alpha", "Dest/a (copy).txt"],
    ] {
        dispatch_bridge(args[0], args[1..].iter().map(|s| s.to_string()), &bridge).unwrap();
    }

    h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn quota_guards_inspect_all_accounts_and_reject_under_quota() {
    assert_eq!(
        format_account_storage(200, 100).as_deref(),
        Some("200 B used")
    );

    let tmp = std::env::temp_dir().join(format!("od-rev-q-{}", Uuid::new_v4()));
    let backend = Arc::new(RecordingBackend::default());
    let mounts = MountManager::new(Arc::clone(&backend) as Arc<dyn MountBackend>)
        .with_mount_root(tmp.clone());
    let ctrl = Arc::new(std::sync::Mutex::new(AppController::new(Settings {
        virtual_drive_enabled: true,
        ..Settings::default()
    })));
    let a1 = Uuid::new_v4();
    let a2 = Uuid::new_v4();
    let mk = |id, folder: &str, enabled, status, used, quota| Account {
        id,
        nickname: folder.into(),
        email: format!("{folder}@x.com"),
        credential_set_id: Uuid::nil(),
        folder_name: folder.into(),
        read_only: false,
        enabled,
        status,
        storage_used: used,
        storage_quota: quota,
        avatar_url: None,
    };
    {
        let mut g = ctrl.lock().unwrap();
        g.accounts
            .add_account(mk(a1, "One", false, AccountStatus::Paused, 1024, 4096));
        g.accounts
            .add_account(mk(a2, "Two", false, AccountStatus::Paused, 100, 0));
    }
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let secrets: Arc<dyn opendrive_auth::SecretStore> =
        Arc::new(opendrive_auth::MemorySecretStore::new());
    mounts.sync_now(&ctrl, &eng, &secrets).unwrap();
    assert_eq!(
        backend.this_pc_namespaces.lock().unwrap().last().unwrap().2,
        None
    );

    ctrl.lock()
        .unwrap()
        .accounts
        .get_account_mut(a2)
        .unwrap()
        .storage_quota = 4096;
    mounts.sync_now(&ctrl, &eng, &secrets).unwrap();
    assert_eq!(
        backend.this_pc_namespaces.lock().unwrap().last().unwrap().2,
        Some(ExplorerQuota {
            used: 1124,
            total: 8192,
            free: 7068
        })
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placeholder_video_cache_ignored_open_refreshes_and_subfolder_cut_paste() {
    let tmp = std::env::temp_dir().join(format!("od-rev-ui-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();

    let drive = seed_drive(
        &[
            (
                "d-sub",
                "Sub",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f-txt", "readme.txt", "text/plain", "root", b"hello"),
        ],
        1024,
        4096,
    );
    drive.add_file(FakeFile {
        id: "f-vid".into(),
        name: "clip.mp4".into(),
        mime_type: "video/mp4".into(),
        size: 1024,
        parents: vec!["root".into()],
        content: vec![9u8; 1024],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(
        SyncEngine::new(Store::open_in_memory().unwrap())
            .with_stream_cache_dir(tmp.join("sc"))
            .unwrap(),
    );
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br
        .with_data_dir(data.clone())
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 1024, 4096);

    let launched = Arc::new(std::sync::Mutex::new(Vec::new()));
    let l_clone = Arc::clone(&launched);
    let endpoints = AuthEndpoints {
        item_launcher: Arc::new(move |app, tgt| {
            l_clone
                .lock()
                .unwrap()
                .push((app.map(str::to_string), tgt.to_string()));
            Ok(())
        }),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "readme.txt");

    let vid_id = eid(&app, a_id, "clip.mp4");
    let stale_p = tmp.join("stale.mp4");
    std::fs::write(&stale_p, b"stale-video-bytes").unwrap();
    eng.store()
        .lock()
        .set_cache_entry(vid_id as i64, &stale_p.to_string_lossy(), 17, false)
        .unwrap();
    assert_eq!(state_of(&eng, vid_id), NodeState::Placeholder);
    let vid_res = bridge.explorer_open_item(a_id, vid_id, true).unwrap();
    assert!(vid_res.is_stream_url);

    let txt_id = eid(&app, a_id, "readme.txt");
    app.apply_action(UiAction::ExplorerOpenItem(txt_id));
    wait_state(&ctx, &mut app, &eng, a_id, txt_id, NodeState::Cached);
    let _ = texts(&ctx, &mut app);
    assert_eq!(
        app.explorer.sessions[&a_id]
            .entries
            .iter()
            .find(|e| e.id == txt_id)
            .unwrap()
            .state,
        NodeState::Cached
    );

    let sub_id = eid(&app, a_id, "Sub");
    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![sub_id],
        cut: true,
    });
    app.apply_action(UiAction::ExplorerPasteClipboard(Some(sub_id)));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id) && a.notice.is_some()
    });
    assert!(app.explorer.clipboard.is_some());

    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![txt_id],
        cut: true,
    });
    app.apply_action(UiAction::ExplorerPasteClipboard(Some(sub_id)));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id) && a.explorer.clipboard.is_none()
    });
    assert!(drive
        .list_children("d-sub")
        .iter()
        .any(|f| f.name == "readme.txt"));

    h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
