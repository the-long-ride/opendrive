//! Tests for lazy subfolder hydration in `SyncEngine::list`.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::{Kind, SyncEngine, VfsEngine, SHARED_WITH_ME_DRIVE_ID};
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
async fn list_lazily_hydrates_placeholder_subfolder_and_marks_cached() {
    let (drive, client, handle) = start_drive().await;

    drive.add_file(FakeFile {
        id: "folder-sub".into(),
        name: "Projects".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-sub".into()),
    });
    drive.add_file(FakeFile {
        id: "nested-1".into(),
        name: "plan.txt".into(),
        mime_type: "text/plain".into(),
        size: 4,
        parents: vec!["folder-sub".into()],
        trashed: false,
        content: b"plan".to_vec(),
        head_revision_id: Some("rev-1".into()),
    });

    let engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();
    engine.hydrate_folder(acct, root).await.unwrap();

    let sub = engine
        .lookup(root, OsStr::new("Projects"))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(sub.kind, Kind::Folder));
    assert_eq!(
        engine
            .store()
            .lock()
            .get_node(sub.id as i64)
            .unwrap()
            .unwrap()
            .state,
        NodeState::Placeholder
    );

    // Listing the placeholder subfolder lazily fetches its remote children and marks it Cached.
    let page = engine.list(sub.id, None).await.unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].name, "plan.txt");
    assert_eq!(
        engine
            .store()
            .lock()
            .get_node(sub.id as i64)
            .unwrap()
            .unwrap()
            .state,
        NodeState::Cached
    );

    handle.abort();
}

#[tokio::test]
async fn shared_with_me_shortcuts_and_offline_placeholder_errors() {
    let (drive, client, handle) = start_drive().await;

    // Shortcut inside My Drive pointing to a shared folder.
    drive.add_file(FakeFile {
        id: "sc-folder".into(),
        name: "Team Shared Shortcut".into(),
        mime_type: "application/vnd.google-apps.shortcut".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });
    drive.set_shortcut(
        "sc-folder",
        "target-dir-99",
        "application/vnd.google-apps.folder",
    );
    drive.add_file(FakeFile {
        id: "broken-sc".into(),
        name: "Dangling Shortcut".into(),
        mime_type: "application/vnd.google-apps.shortcut".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });
    drive.add_file(FakeFile {
        id: "target-child".into(),
        name: "roadmap.md".into(),
        mime_type: "text/plain".into(),
        size: 7,
        parents: vec!["target-dir-99".into()],
        trashed: false,
        content: b"roadmap".to_vec(),
        head_revision_id: Some("rev-1".into()),
    });
    // Shared-with-me folder not directly in My Drive root.
    drive.add_file(FakeFile {
        id: "swm-folder".into(),
        name: "Shared Novels".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["someone-else-root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });
    drive.mark_shared_with_me("swm-folder");
    drive.add_file(FakeFile {
        id: "swm-ch1".into(),
        name: "chapter1.mp3".into(),
        mime_type: "audio/mpeg".into(),
        size: 100,
        parents: vec!["swm-folder".into()],
        trashed: false,
        content: vec![1; 100],
        head_revision_id: Some("rev-1".into()),
    });

    // Offline unhydrated placeholder folder MUST err rather than return empty DirPage.
    let offline_engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let off_acct = Uuid::new_v4();
    let off_root = offline_engine.seed_root_account(off_acct, "Off").unwrap();
    let ph_id = offline_engine.store().lock().next_id().unwrap();
    offline_engine
        .store()
        .lock()
        .upsert_node(&Node {
            id: ph_id,
            account_id: off_acct,
            drive_id: "pending-dir".into(),
            parent_id: Some(off_root as i64),
            name: "Unauthenticated".into(),
            display_name: "Unauthenticated".into(),
            kind: "folder".into(),
            mime: Some("application/vnd.google-apps.folder".into()),
            size: 0,
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
    assert!(offline_engine.list(ph_id as u64, None).await.is_err());

    let engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();
    engine.hydrate_folder(acct, root).await.unwrap();

    // Verify shortcut folder resolves target folder children cleanly.
    let sc = engine
        .lookup(root, OsStr::new("Team Shared Shortcut"))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(sc.kind, Kind::Folder));
    let sc_page = engine.list(sc.id, None).await.unwrap();
    assert_eq!(sc_page.entries.len(), 1);
    assert_eq!(sc_page.entries[0].name, "roadmap.md");

    // Verify dangling shortcut without target stays non-crashing Shortcut kind.
    let broken = engine
        .lookup(root, OsStr::new("Dangling Shortcut"))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(broken.kind, Kind::Shortcut));

    // Verify virtual "Shared with me" folder is hydrated on list.
    let shared_root = engine
        .lookup(root, OsStr::new("Shared with me"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .store()
            .lock()
            .get_node(shared_root.id as i64)
            .unwrap()
            .unwrap()
            .drive_id,
        SHARED_WITH_ME_DRIVE_ID
    );
    let shared_page = engine.list(shared_root.id, None).await.unwrap();
    assert_eq!(shared_page.entries.len(), 1);
    assert_eq!(shared_page.entries[0].name, "Shared Novels");

    let novel_page = engine.list(shared_page.entries[0].id, None).await.unwrap();
    assert_eq!(novel_page.entries.len(), 1);
    assert_eq!(novel_page.entries[0].name, "chapter1.mp3");

    handle.abort();
}

#[tokio::test]
async fn shared_with_me_survives_poll_changes_and_repairs_orphaned_cached_state() {
    let (drive, client, handle) = start_drive().await;
    let fdir = "application/vnd.google-apps.folder";
    drive.add_file(FakeFile {
        id: "swm-folder".into(),
        name: "Shared Novels".into(),
        mime_type: fdir.into(),
        parents: vec!["external-owner-root".into()],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.mark_shared_with_me("swm-folder");

    let engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();
    engine.hydrate_folder(acct, root).await.unwrap();
    let swm_id = engine
        .lookup(root, OsStr::new("Shared with me"))
        .await
        .unwrap()
        .unwrap()
        .id;

    let first = engine
        .list_explorer_nodes(acct, swm_id, false)
        .await
        .unwrap();
    assert!(first.len() == 1 && first[0].display_name == "Shared Novels");

    assert_eq!(engine.poll_changes(acct).await.unwrap(), 0);
    drive.update_metadata("swm-folder", Some("Shared Novels"), None, None);
    assert!(engine.poll_changes(acct).await.unwrap() >= 1);
    let after = engine
        .list_explorer_nodes(acct, swm_id, false)
        .await
        .unwrap();
    assert!(after.iter().any(|n| n.display_name == "Shared Novels"));

    {
        let st = engine.store().lock();
        let mut swm_child = st.find_by_drive_id(acct, "swm-folder").unwrap().unwrap();
        swm_child.parent_id = None;
        st.upsert_node(&swm_child).unwrap();
    }
    let repaired = engine.list(swm_id, None).await.unwrap();
    assert_eq!(repaired.entries.len(), 1);
    assert_eq!(repaired.entries[0].name, "Shared Novels");

    drive.add_file(FakeFile {
        id: "new-share".into(),
        name: "shared.zip".into(),
        mime_type: "application/zip".into(),
        size: 12,
        content: vec![3; 12],
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    drive.mark_shared_with_me("new-share");
    let refreshed = engine
        .list_explorer_nodes(acct, swm_id, false)
        .await
        .unwrap();
    assert!(refreshed.iter().any(|n| n.display_name == "shared.zip"));
    handle.abort();
}
