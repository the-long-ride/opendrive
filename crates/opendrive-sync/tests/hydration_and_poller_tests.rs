//! Hydration of partial placeholder folders, BFS local reconcile ordering, and poller auth backoff.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::{SyncEngine, VfsEngine, SHARED_WITH_ME_DRIVE_ID};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn placeholder_folder_with_partial_children_hydrates_and_offline_falls_back() {
    let drive = Arc::new(FakeDrive::new());
    let fdir = "application/vnd.google-apps.folder";
    for (id, name, mime, parent, bytes) in [
        ("d-novels", "Novels", fdir, "root", &b""[..]),
        ("d-c1", "01 - Intro", fdir, "d-novels", b""),
        ("d-c2", "02 - Chapters", fdir, "d-novels", b""),
        ("f-deep", "ch1.mp3", "audio/mpeg", "d-c1", b"audio"),
    ] {
        drive.add_file(FakeFile {
            id: id.into(),
            name: name.into(),
            mime_type: mime.into(),
            size: bytes.len() as u64,
            parents: vec![parent.into()],
            content: bytes.to_vec(),
            head_revision_id: Some("rev-1".into()),
            ..Default::default()
        });
    }
    let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let acct = Uuid::new_v4();
    eng.set_drive_client(
        acct,
        DriveClient::with_base_urls(
            "t".into(),
            format!("http://{addr}/drive/v3"),
            format!("http://{addr}/upload/drive/v3"),
        ),
    );
    let root = eng.seed_root_account(acct, "Root").unwrap();
    eng.hydrate_folder(acct, root).await.unwrap();
    let novels = eng
        .resolve_remote_path(root, "Novels")
        .expect("Novels seeded as placeholder");

    // Seed only 1 partial local child inside Placeholder `Novels` (reproduces partial-children bug).
    {
        let st = eng.store().lock();
        let pid = st.next_id().unwrap();
        st.upsert_node(&Node {
            id: pid,
            account_id: acct,
            drive_id: "d-c1".into(),
            parent_id: Some(novels as i64),
            name: "01 - Intro".into(),
            display_name: "01 - Intro".into(),
            kind: "folder".into(),
            mime: Some(fdir.into()),
            size: 0,
            md5: None,
            modified_ms: 0,
            head_revision: Some("rev-1".into()),
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Cached,
            pinned: false,
            last_access_ms: 0,
        })
        .unwrap();
        // Update seeded sharedWithMe to simulate legacy parent_id = Some(root) migration.
        let mut leg = st
            .find_by_drive_id(acct, SHARED_WITH_ME_DRIVE_ID)
            .unwrap()
            .unwrap();
        leg.parent_id = Some(root as i64);
        st.upsert_node(&leg).unwrap();
    }

    let swm_id = eng.seed_shared_with_me_root(acct).unwrap();
    assert_eq!(eng.get_node(swm_id).unwrap().unwrap().parent_id, Some(0));

    // VfsEngine::list must still hydrate missing sibling `02 - Chapters` for Placeholder `Novels`.
    let page = eng.list(novels, None).await.unwrap();
    let names: Vec<&str> = page.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"01 - Intro") && names.contains(&"02 - Chapters"));

    // BFS local_reconcile_entries visits parents strictly before children.
    let c1_id = eng.resolve_remote_path(root, "Novels/01 - Intro").unwrap();
    eng.hydrate_folder(acct, c1_id).await.unwrap();
    let bfs = eng.local_reconcile_entries(root);
    let idx = |p: &str| bfs.iter().position(|e| e.0 == Path::new(p)).unwrap();
    assert!(idx("Novels") < idx("Novels/01 - Intro"));
    assert!(idx("Novels/01 - Intro") < idx("Novels/01 - Intro/ch1.mp3"));

    // Offline fallback on a Placeholder folder with partial children returns partial children.
    let c2_id = eng
        .resolve_remote_path(root, "Novels/02 - Chapters")
        .unwrap();
    {
        let st = eng.store().lock();
        let pid = st.next_id().unwrap();
        st.upsert_node(&Node {
            id: pid,
            account_id: acct,
            drive_id: "f-local-only".into(),
            parent_id: Some(c2_id as i64),
            name: "partial.txt".into(),
            display_name: "partial.txt".into(),
            kind: "file".into(),
            mime: Some("text/plain".into()),
            size: 4,
            md5: None,
            modified_ms: 0,
            head_revision: None,
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Cached,
            pinned: false,
            last_access_ms: 0,
        })
        .unwrap();
    }
    drive.push_error(500);
    let fallback = eng.list(c2_id, None).await.unwrap();
    assert_eq!(fallback.entries[0].name, "partial.txt");

    // Poller handles 401 auth failure cleanly with OD-AUTH-002 backoff path.
    drive.push_error(401);
    let p = eng.spawn_change_poller(acct, Duration::from_millis(5));
    tokio::time::sleep(Duration::from_millis(20)).await;
    p.abort();
    h.abort();
}
