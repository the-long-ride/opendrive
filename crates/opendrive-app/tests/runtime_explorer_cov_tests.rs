//! Coverage tests for explorer_open video/cache branches and mounted explorer sync/file_ops paths.

mod common_auth_helpers;

use common_auth_helpers::mock_token_server;
use opendrive_app::runtime::cli::resolve_remote_node;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_app::runtime::ui::build_bridge_in_dirs;
use opendrive_core::{
    Account, AccountStatus, AppController, CredentialSet, OpenWithRule, Settings,
};
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::NodeState;
use std::sync::Arc;
use uuid::Uuid;

fn ffile(id: &str, name: &str, mime: &str, parent: &str, data: &[u8]) -> FakeFile {
    FakeFile {
        id: id.into(),
        name: name.into(),
        mime_type: mime.into(),
        size: data.len() as u64,
        parents: vec![parent.into()],
        trashed: false,
        content: data.to_vec(),
        head_revision_id: Some("rev-1".into()),
    }
}

#[test]
fn explorer_open_video_local_cache_and_mounted_ops() {
    let tmp = std::env::temp_dir().join(format!("od-open-cov-{}", Uuid::new_v4()));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = Arc::new(FakeDrive::new());
    drive.add_file(ffile(
        "d1",
        "Docs",
        "application/vnd.google-apps.folder",
        "root",
        &[],
    ));
    drive.add_file(ffile("v1", "clip.mp4", "video/mp4", "root", b"12345678"));
    drive.add_file(ffile("f1", "note.txt", "text/plain", "root", b"hello"));
    let (addr, h) = rt.block_on(FakeDriveServer::start(drive));
    let (tok_url, tok_h) = rt.block_on(mock_token_server(
        r#"{"access_token":"at","expires_in":3600}"#,
    ));
    let (api, up) = (
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );

    // Unmounted open branches: .m3u playlist, non-stream app sync_down, non-video sync_down, local_p cache fallback.
    let mnt1 = tmp.join("mnt1");
    let br1 = build_bridge_in_dirs(
        AppController::new(Settings::default()),
        None,
        Some(&tmp.join("d1")),
    )
    .unwrap()
    .with_mount_manager(Arc::new(
        MountManager::new_null().with_mount_root(mnt1.clone()),
    ));
    let a1 = br1
        .add_account("W".into(), "w@x.com".into(), "w".into(), false)
        .unwrap()
        .accounts[0]
        .id;
    br1.engine().set_drive_client(
        a1,
        opendrive_drive::DriveClient::with_base_urls("t".into(), api.clone(), up.clone()),
    );
    br1.explorer_list(a1, None, true).unwrap();
    let _r1 = br1.explorer_root_node(a1).unwrap();
    let v1 = resolve_remote_node(&br1, a1, "clip.mp4").unwrap();
    let f1 = resolve_remote_node(&br1, a1, "note.txt").unwrap();

    let m3u = br1.explorer_open_item(a1, v1, true).unwrap();
    assert!(m3u.is_stream_url && m3u.target.ends_with(".m3u"));
    br1.update_open_with_rules(vec![OpenWithRule {
        pattern: "@video".into(),
        app_command: "notepad".into(),
    }])
    .unwrap();
    assert!(!br1.explorer_open_item(a1, v1, true).unwrap().is_stream_url);
    assert!(!br1.explorer_open_item(a1, f1, true).unwrap().is_stream_url);

    let local_note = mnt1.join("w/note.txt");
    std::fs::create_dir_all(local_note.parent().unwrap()).unwrap();
    std::fs::write(&local_note, b"hello").unwrap();
    {
        let st = br1.engine().store();
        let g = st.lock();
        let mut n = g.get_node(f1 as i64).unwrap().unwrap();
        n.state = NodeState::Cached;
        g.upsert_node(&n).unwrap();
    }
    assert!(!br1.explorer_open_item(a1, f1, true).unwrap().is_stream_url);
    drop(br1);

    // Mounted account branches: resolve_video_target (mounted+no-rule, mounted+notepad), sync_down folder, sync_up_node, sync_up_and_clear dehydrate_path, rename/delete mounted.
    let mnt2 = tmp.join("mnt2");
    let mm2 = Arc::new(
        MountManager::new_null()
            .with_mount_root(mnt2.clone())
            .with_token_url(tok_url)
            .with_drive_urls(api, up),
    );
    let (cid, aid) = (Uuid::new_v4(), Uuid::new_v4());
    let mut c2 = AppController::new(Settings::default());
    c2.accounts.add_credential(CredentialSet {
        id: cid,
        label: "L".into(),
        client_id: "c".into(),
    });
    c2.accounts.add_account(Account {
        id: aid,
        nickname: "W".into(),
        email: "w@x.com".into(),
        credential_set_id: cid,
        folder_name: "W".into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    });
    let br2 = build_bridge_in_dirs(c2, None, Some(&tmp.join("d2")))
        .unwrap()
        .with_mount_manager(Arc::clone(&mm2));
    br2.secrets().store_client_secret(cid, "s").unwrap();
    br2.secrets().store_refresh_token(aid, "rt").unwrap();
    mm2.sync_now(&br2.controller_handle(), br2.engine(), br2.secrets())
        .unwrap();

    let v2 = resolve_remote_node(&br2, aid, "clip.mp4").unwrap();
    let f2 = resolve_remote_node(&br2, aid, "note.txt").unwrap();
    let d2 = resolve_remote_node(&br2, aid, "Docs").unwrap();
    std::fs::create_dir_all(mnt2.join("W")).unwrap();
    std::fs::write(mnt2.join("W/clip.mp4"), b"12345678").unwrap();
    std::fs::write(mnt2.join("W/note.txt"), b"edited").unwrap();
    assert!(!br2.explorer_open_item(aid, v2, true).unwrap().is_stream_url);
    br2.update_open_with_rules(vec![OpenWithRule {
        pattern: "@video".into(),
        app_command: "notepad".into(),
    }])
    .unwrap();
    assert!(!br2.explorer_open_item(aid, v2, true).unwrap().is_stream_url);

    br2.explorer_sync_down(aid, d2).unwrap();
    br2.explorer_sync_up_node(aid, f2).unwrap();
    br2.explorer_sync_up_and_clear(aid, f2).unwrap();
    br2.explorer_rename_item(aid, f2, "note2.txt").unwrap();
    br2.explorer_delete_item(aid, f2).unwrap();

    tok_h.abort();
    h.abort();
    drop(br2);
    let _ = std::fs::remove_dir_all(&tmp);
}
