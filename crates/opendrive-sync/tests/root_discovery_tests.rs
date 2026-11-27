//! Regression tests for real root folder ID resolution, unparented root files, and root refresh.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::{SyncEngine, VfsEngine};
use std::ffi::OsStr;
use std::sync::Arc;
use uuid::Uuid;

async fn start_drive() -> (Arc<FakeDrive>, DriveClient, tokio::task::JoinHandle<()>) {
    let drive = Arc::new(FakeDrive::new());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let client = DriveClient::with_base_urls(
        "tok".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );
    (drive, client, handle)
}

#[tokio::test]
async fn root_discovers_real_root_changes_unparented_and_link_files() {
    let (drive, client, handle) = start_drive().await;
    drive.set_root_real_id("0AF-real-root-999");

    // Seed initial root child so root becomes Cached with >= 1 child.
    drive.add_file(FakeFile {
        id: "seed-file".into(),
        name: "Initial.txt".into(),
        mime_type: "text/plain".into(),
        size: 4,
        parents: vec!["root".into()],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    // Shared-with-me folder + nested child (must NOT leak into My Drive root).
    drive.add_file(FakeFile {
        id: "swm-folder".into(),
        name: "Shared Novels".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        parents: vec!["ext-owner".into()],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.mark_shared_with_me("swm-folder");
    drive.add_file(FakeFile {
        id: "swm-nested".into(),
        name: "Nested.wav".into(),
        mime_type: "audio/wav".into(),
        size: 8,
        parents: vec!["swm-folder".into()],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });

    let engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();
    let initial = engine.list_explorer_nodes(acct, root, false).await.unwrap();
    assert!(initial.iter().any(|n| n.display_name == "Initial.txt"));

    // Initialize poll_changes change token before remote mutations.
    assert_eq!(engine.poll_changes(acct).await.unwrap(), 0);

    // 1) File pushed with real root ID in `parents` via /changes.
    drive.add_file(FakeFile {
        id: "real-root-zip".into(),
        name: "RealRoot.zip".into(),
        mime_type: "application/zip".into(),
        size: 100,
        parents: vec!["0AF-real-root-999".into()],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    // 2) Unparented owned file arriving via /changes (`parents = []`, `ownedByMe = true`).
    drive.add_file(FakeFile {
        id: "unparented-owned-zip".into(),
        name: "UnparentedOwned.zip".into(),
        mime_type: "application/zip".into(),
        size: 200,
        parents: vec![],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    // 3) Top-level anyoneWithLink file (`parents = []`, `ownedByMe = false`, `sharedWithMe = false`).
    drive.add_file(FakeFile {
        id: "link-zip".into(),
        name: "AnyoneLink.zip".into(),
        mime_type: "application/zip".into(),
        size: 300,
        parents: vec![],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.mark_unowned("link-zip");

    assert_eq!(engine.poll_changes(acct).await.unwrap(), 3);
    let after_poll = engine.list_explorer_nodes(acct, root, false).await.unwrap();
    for expected in [
        "Initial.txt",
        "RealRoot.zip",
        "UnparentedOwned.zip",
        "AnyoneLink.zip",
    ] {
        assert!(
            after_poll.iter().any(|n| n.display_name == expected),
            "missing {expected} in root after poll_changes"
        );
    }
    assert!(!after_poll.iter().any(|n| n.display_name == "Nested.wav"));
    assert!(!after_poll.iter().any(|n| n.display_name == "Shared Novels"));

    // Simulate pre-existing orphaned rows (`parent_id = None`) in SQLite (as in existing meta.db)
    // and an account whose root is already `Cached` with >= 1 child: list_explorer_nodes(false)
    // must still re-hydrate root and re-attach both orphaned unparented files cleanly.
    {
        let store = engine.store().lock();
        for did in ["unparented-owned-zip", "link-zip"] {
            let mut node = store.find_by_drive_id(acct, did).unwrap().unwrap();
            node.parent_id = None;
            store.upsert_node(&node).unwrap();
        }
        // Also insert an unparented sharedWithMe item with parent_id = None to ensure it routes to SWM.
        let sid = store.next_id().unwrap();
        store
            .upsert_node(&Node {
                id: sid,
                account_id: acct,
                drive_id: "swm-orphan".into(),
                parent_id: None,
                name: "SharedOrphan.zip".into(),
                display_name: "SharedOrphan.zip".into(),
                kind: "file".into(),
                mime: Some("application/zip".into()),
                size: 50,
                md5: None,
                modified_ms: 0,
                head_revision: None,
                trashed: false,
                shared_drive_id: None,
                state: NodeState::Placeholder,
                pinned: false,
                last_access_ms: 0,
            })
            .unwrap();
    }
    drive.add_file(FakeFile {
        id: "swm-orphan".into(),
        name: "SharedOrphan.zip".into(),
        mime_type: "application/zip".into(),
        size: 50,
        parents: vec![],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.mark_shared_with_me("swm-orphan");

    let repaired = engine.list_explorer_nodes(acct, root, false).await.unwrap();
    assert!(repaired
        .iter()
        .any(|n| n.display_name == "UnparentedOwned.zip"));
    assert!(repaired.iter().any(|n| n.display_name == "AnyoneLink.zip"));
    assert!(!repaired
        .iter()
        .any(|n| n.display_name == "SharedOrphan.zip"));

    assert!(engine
        .lookup(root, OsStr::new("Shared with me"))
        .await
        .unwrap()
        .is_none());
    let swm_id = engine.seed_shared_with_me_root(acct).unwrap();
    let swm_nodes = engine
        .list_explorer_nodes(acct, swm_id, false)
        .await
        .unwrap();
    assert!(swm_nodes
        .iter()
        .any(|n| n.display_name == "SharedOrphan.zip"));

    engine.remove_account(acct).await.unwrap();
    handle.abort();
}
