//! Coverage tests for cli, cli_commands, explorer_open, explorer_file_ops, explorer_ops, and media_server.

use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::{
    cmd_explore, cmd_open_item, cmd_stream, cmd_sync_down, cmd_sync_up, dispatch, dispatch_bridge,
    list_root_with_base, resolve_remote_node,
};
use opendrive_app::runtime::media_server::{parse_http_range, MediaStreamServer};
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_app::runtime::ui::build_bridge_in_dirs;
use opendrive_core::{AppController, OpenWithRule, Settings};
use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::Store;
use opendrive_sync::{NodeId, SyncEngine};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

fn setup_fake_env(
    tmp: &std::path::Path,
) -> (
    tokio::runtime::Runtime,
    Arc<FakeDrive>,
    tokio::task::JoinHandle<()>,
    Bridge,
    Uuid,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = Arc::new(FakeDrive::new());
    drive.add_file(FakeFile {
        id: "dir-docs".into(),
        name: "Docs".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        parents: vec!["root".into()],
        ..Default::default()
    });
    drive.add_file(FakeFile {
        id: "dir-sub".into(),
        name: "Sub".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        parents: vec!["dir-docs".into()],
        ..Default::default()
    });
    drive.add_file(FakeFile {
        id: "file-note".into(),
        name: "Note.txt".into(),
        mime_type: "text/plain".into(),
        size: 5,
        parents: vec!["dir-sub".into()],
        content: b"hello".to_vec(),
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.add_file(FakeFile {
        id: "vid-1".into(),
        name: "clip.mp4".into(),
        mime_type: "video/mp4".into(),
        size: 16,
        parents: vec!["root".into()],
        content: b"0123456789abcdef".to_vec(),
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.add_file(FakeFile {
        id: "zero-1".into(),
        name: "empty.mp4".into(),
        mime_type: "video/mp4".into(),
        size: 0,
        parents: vec!["root".into()],
        content: vec![],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.add_file(FakeFile {
        id: "gdoc-1".into(),
        name: "Plan".into(),
        mime_type: "application/vnd.google-apps.document".into(),
        parents: vec!["root".into()],
        ..Default::default()
    });
    let (addr, h) = rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));
    let mnt = tmp.join("mnt");
    let br = build_bridge_in_dirs(AppController::new(Settings::default()), None, Some(tmp))
        .unwrap()
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let acct = br
        .add_account("W".into(), "w@x.com".into(), "w".into(), false)
        .unwrap()
        .accounts[0]
        .id;
    br.engine().set_drive_client(
        acct,
        DriveClient::with_base_urls(
            "tok".into(),
            format!("http://{addr}/drive/v3"),
            format!("http://{addr}/upload/drive/v3"),
        ),
    );
    (rt, drive, h, br, acct)
}

#[test]
fn cli_dispatch_commands_and_file_ops_paths() {
    let tmp = std::env::temp_dir().join(format!("od-cli-cov-{}", Uuid::new_v4()));
    let (rt, _drive, h, br, acct) = setup_fake_env(&tmp);

    dispatch("--help", std::iter::empty()).unwrap();
    dispatch("-h", std::iter::empty()).unwrap();
    assert!(rt
        .block_on(list_root_with_base("tok", "http://127.0.0.1:1/drive/v3"))
        .is_err());

    for bad in [
        "rename",
        "rm",
        "delete",
        "cp",
        "copy",
        "mv",
        "move",
        "open-item",
        "stream",
    ] {
        assert!(dispatch_bridge(bad, std::iter::empty(), &br).is_err());
    }

    // resolve_remote_node nested walk + case-insensitive match when Docs/Sub aren't listed yet.
    let note_nid = resolve_remote_node(&br, acct, "docs/sub/note.txt").unwrap();
    assert!(note_nid > 0);
    assert!(resolve_remote_node(&br, acct, "docs/missing").is_err());

    cmd_explore(&br, "W", Some("Docs")).unwrap();
    cmd_sync_down(&br, "W", "Docs").unwrap();
    let up = tmp.join("local.txt");
    std::fs::write(&up, b"up").unwrap();
    cmd_sync_up(&br, "W", up.to_str().unwrap(), Some("Docs")).unwrap();
    cmd_open_item(&br, "W", "Plan", true).unwrap();
    assert!(cmd_stream(&br, "W", "empty.mp4", true).is_err());

    // dispatch_bridge file ops: cp, mv, rename, rm, open-item --dry-run (flag last), stream --probe.
    for args in [
        vec!["open-item", "W", "clip.mp4", "--dry-run"],
        vec!["stream", "--probe", "W", "clip.mp4"],
        vec!["cp", "W", "clip.mp4"],
        vec!["copy", "W", "clip.mp4", "Docs"],
        vec!["rename", "W", "Docs/clip.mp4", "clip2.mp4"],
        vec!["mv", "W", "clip.mp4", "Docs"],
        vec!["rm", "W", "Docs/clip2.mp4"],
    ] {
        dispatch_bridge(args[0], args[1..].iter().map(|s| s.to_string()), &br).unwrap();
    }

    // Read-only and missing account guards across file_ops & explorer_ops.
    let ro_id = br
        .add_account("RO".into(), "r@x.com".into(), "ro".into(), true)
        .unwrap()
        .accounts[1]
        .id;
    assert!(br.explorer_delete_item(ro_id, note_nid).is_err());
    assert!(br.explorer_rename_item(ro_id, note_nid, "x").is_err());
    assert!(br.explorer_move_item(ro_id, note_nid, note_nid).is_err());
    assert!(br.explorer_copy_item(ro_id, note_nid, note_nid).is_err());
    assert!(br.explorer_sync_up(ro_id, note_nid, &[]).is_err());
    assert!(br.explorer_sync_up_node(ro_id, note_nid).is_err());
    assert!(br.explorer_sync_up_and_clear(ro_id, note_nid).is_err());
    assert!(br.explorer_delete_item(Uuid::new_v4(), note_nid).is_err());
    assert!(br.explorer_list(ro_id, None, true).is_err());
    assert!(br.explorer_stream_url(ro_id, note_nid).is_err());
    assert!(br.explorer_stream_url(acct, 999999).is_err());
    assert!(br
        .update_open_with_rules(vec![OpenWithRule {
            pattern: "".into(),
            app_command: "x".into()
        }])
        .is_err());
    br.remove_open_with_rule(99).unwrap();

    h.abort();
    drop(br);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_server_range_and_malformed_socket_edges() {
    for bad in [
        "items=0-1",
        "bytes=0-1,2-3",
        "bytes=0",
        "bytes=-bad",
        "bytes=bad-1",
        "bytes=0-bad",
    ] {
        assert_eq!(parse_http_range(bad, 100), None);
    }
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let root = eng.seed_root_account(Uuid::new_v4(), "w").unwrap();
    let mut n = eng.store().lock().get_node(root as i64).unwrap().unwrap();
    (
        n.id,
        n.parent_id,
        n.drive_id,
        n.kind,
        n.size,
        n.name,
        n.display_name,
    ) = (
        0,
        Some(root as i64),
        "m1".into(),
        "file".into(),
        64,
        "m.mp4".into(),
        "m.mp4".into(),
    );
    let nid = eng.store().lock().upsert_node(&n).unwrap() as NodeId;
    let srv = MediaStreamServer::start(eng, &tokio::runtime::Handle::current()).unwrap();
    let url = srv.stream_url(nid, "m.mp4");
    let host = url
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();

    // 1. Immediate EOF (no request line).
    let s1 = TcpStream::connect(host).await.unwrap();
    drop(s1);
    // 2. Request line without blank terminator (header EOF).
    let mut s2 = TcpStream::connect(host).await.unwrap();
    let path = url.strip_prefix(&format!("http://{host}")).unwrap();
    s2.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n").as_bytes())
        .await
        .unwrap();
    s2.shutdown().await.unwrap();
    let mut buf = Vec::new();
    let _ = s2.read_to_end(&mut buf).await;
    assert!(String::from_utf8_lossy(&buf).contains("200 OK"));
}
