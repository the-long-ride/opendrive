//! Tests for hierarchical ExplorerSyncStatus classification and sync-down pinning.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::{ExplorerSyncStatus, SyncEngine};
use std::sync::Arc;
use uuid::Uuid;

fn mk(
    id: i64,
    account_id: Uuid,
    p: i64,
    name: &str,
    kind: &str,
    state: NodeState,
    pinned: bool,
) -> Node {
    let mime = if kind == "folder" {
        "application/vnd.google-apps.folder"
    } else {
        "text/plain"
    };
    Node {
        id,
        account_id,
        drive_id: format!("d-{id}"),
        parent_id: Some(p),
        name: name.into(),
        display_name: name.into(),
        kind: kind.into(),
        mime: Some(mime.into()),
        size: if kind == "folder" { 0 } else { 10 },
        md5: None,
        modified_ms: 1000,
        head_revision: None,
        trashed: false,
        shared_drive_id: None,
        state,
        pinned,
        last_access_ms: 0,
    }
}

#[test]
fn compute_sync_status_distinguishes_cloud_indexed_partial_full_and_states() {
    let store = Store::open_in_memory().unwrap();
    store.migrate().unwrap();
    let acct = Uuid::new_v4();
    let r = 1;
    let nodes = [
        mk(r, acct, 0, "Root", "folder", NodeState::Cached, false),
        mk(
            10,
            acct,
            r,
            "CloudOnly",
            "folder",
            NodeState::Placeholder,
            false,
        ),
        mk(
            11,
            acct,
            r,
            "IndexedEmpty",
            "folder",
            NodeState::Cached,
            false,
        ),
        mk(
            12,
            acct,
            r,
            "IndexedParent",
            "folder",
            NodeState::Cached,
            false,
        ),
        mk(
            13,
            acct,
            12,
            "remote.txt",
            "file",
            NodeState::Placeholder,
            false,
        ),
        mk(
            14,
            acct,
            r,
            "PartialMix",
            "folder",
            NodeState::Cached,
            false,
        ),
        mk(15, acct, 14, "local.mp3", "file", NodeState::Cached, false),
        mk(
            16,
            acct,
            14,
            "cloud.mp3",
            "file",
            NodeState::Placeholder,
            false,
        ),
        mk(
            17,
            acct,
            r,
            "PartialSub",
            "folder",
            NodeState::Cached,
            false,
        ),
        mk(18, acct, 17, "local2.mp3", "file", NodeState::Cached, false),
        mk(
            19,
            acct,
            17,
            "UnindexedSub",
            "folder",
            NodeState::Placeholder,
            false,
        ),
        mk(
            20,
            acct,
            r,
            "UnindexedWithLocal",
            "folder",
            NodeState::Placeholder,
            false,
        ),
        mk(21, acct, 20, "x.txt", "file", NodeState::Dirty, false),
        mk(22, acct, r, "FullDir", "folder", NodeState::Cached, true),
        mk(23, acct, 22, "Sub", "folder", NodeState::Cached, true),
        mk(
            24,
            acct,
            23,
            "Note",
            "native_doc",
            NodeState::Placeholder,
            false,
        ),
        mk(25, acct, 23, "done.mp3", "file", NodeState::Cached, false),
        mk(
            26,
            acct,
            r,
            "PinnedEmpty",
            "folder",
            NodeState::Cached,
            true,
        ),
        mk(27, acct, r, "DirtyDir", "folder", NodeState::Dirty, false),
        mk(
            28,
            acct,
            r,
            "ConflictDir",
            "folder",
            NodeState::Conflict,
            false,
        ),
        mk(
            29,
            acct,
            r,
            "HydratingDir",
            "folder",
            NodeState::Hydrating,
            false,
        ),
        mk(
            30,
            acct,
            r,
            "NestedConflict",
            "folder",
            NodeState::Cached,
            false,
        ),
        mk(31, acct, 30, "bad.txt", "file", NodeState::Conflict, false),
        mk(32, acct, r, "dl.bin", "file", NodeState::Hydrating, false),
    ];
    for n in &nodes {
        store.upsert_node(n).unwrap();
    }
    let engine = SyncEngine::new(store);
    let expect = |idx: usize, st| assert_eq!(engine.compute_sync_status(&nodes[idx]), st);
    expect(1, ExplorerSyncStatus::Cloud);
    expect(2, ExplorerSyncStatus::Indexed);
    expect(3, ExplorerSyncStatus::Indexed);
    expect(4, ExplorerSyncStatus::Cloud);
    expect(5, ExplorerSyncStatus::PartialLocal);
    expect(8, ExplorerSyncStatus::PartialLocal);
    expect(11, ExplorerSyncStatus::PartialLocal);
    expect(12, ExplorerSyncStatus::PendingUpload);
    expect(13, ExplorerSyncStatus::FullLocal);
    expect(16, ExplorerSyncStatus::FullLocal);
    expect(17, ExplorerSyncStatus::FullLocal);
    expect(18, ExplorerSyncStatus::PendingUpload);
    expect(19, ExplorerSyncStatus::Conflict);
    expect(20, ExplorerSyncStatus::Syncing);
    expect(21, ExplorerSyncStatus::Conflict);
    expect(22, ExplorerSyncStatus::Conflict);
    expect(23, ExplorerSyncStatus::Syncing);
}

#[tokio::test]
async fn sync_down_and_clear_transition_indexed_full_and_cloud_folder_statuses() {
    let drive = Arc::new(FakeDrive::new());
    let (addr, srv) = FakeDriveServer::start(Arc::clone(&drive)).await;
    drive.add_file(FakeFile {
        id: "alb-1".into(),
        name: "Album".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: None,
    });
    drive.add_file(FakeFile {
        id: "trk-1".into(),
        name: "track1.mp3".into(),
        mime_type: "audio/mpeg".into(),
        size: 3,
        parents: vec!["alb-1".into()],
        trashed: false,
        content: b"one".to_vec(),
        head_revision_id: Some("r1".into()),
    });
    let client = DriveClient::with_base_urls(
        "tok".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );

    let store = Store::open_in_memory().unwrap();
    store.migrate().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root_id = engine.seed_root_account(acct, "Album").unwrap();

    let top = engine
        .list_explorer_nodes(acct, root_id, true)
        .await
        .unwrap();
    let album = top.into_iter().find(|n| n.name == "Album").unwrap();
    assert_eq!(
        engine.compute_sync_status(&album),
        ExplorerSyncStatus::Cloud
    );

    let _ = engine
        .list_explorer_nodes(acct, album.id as u64, false)
        .await
        .unwrap();
    let album_idx = engine.get_node(album.id as u64).unwrap().unwrap();
    assert_eq!(
        engine.compute_sync_status(&album_idx),
        ExplorerSyncStatus::Indexed
    );

    let tmp = std::env::temp_dir().join(format!("od-st-{}", Uuid::new_v4()));
    let (mount, cache) = (tmp.join("mnt"), tmp.join("cache"));
    engine
        .sync_down_node(acct, root_id, album.id as u64, Some(&mount), &cache)
        .await
        .unwrap();
    let album_full = engine.get_node(album.id as u64).unwrap().unwrap();
    assert_eq!(
        engine.compute_sync_status(&album_full),
        ExplorerSyncStatus::FullLocal
    );

    engine
        .sync_up_and_clear_local(acct, root_id, album.id as u64, Some(&mount))
        .await
        .unwrap();
    let album_clr = engine.get_node(album.id as u64).unwrap().unwrap();
    assert_eq!(
        engine.compute_sync_status(&album_clr),
        ExplorerSyncStatus::Cloud
    );

    srv.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
