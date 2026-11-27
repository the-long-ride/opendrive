//! End-to-end tests for `SyncEngine`, `PlaceholderEngine`, `BandwidthLimiter`, and `FakeDriveServer`.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{
    BandwidthLimiter, EngineEvent, Kind, PlaceholderEngine, SyncEngine, SyncError, VfsEngine,
};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
async fn placeholder_engine_and_bandwidth_limiter() {
    let pe = PlaceholderEngine;
    assert!(pe.list(1, None).await.unwrap().entries.is_empty());
    assert!(pe.lookup(1, OsStr::new("x")).await.unwrap().is_none());
    assert!(matches!(pe.stat(1).await, Err(SyncError::NotFound)));
    assert!(matches!(pe.read(1, 0, 10).await, Err(SyncError::NotFound)));
    assert_eq!(pe.open_write(2).await.unwrap().node, 2);
    assert_eq!(pe.write(2, 0, b"abc").await.unwrap(), 3);
    assert!(pe.create(1, OsStr::new("a"), Kind::File).await.is_err());
    pe.rename(1, 2, OsStr::new("b")).await.unwrap();
    pe.remove(1).await.unwrap();
    pe.set_pinned(1, true).await.unwrap();
    let _rx = pe.subscribe();
    let acct = Uuid::new_v4();
    pe.add_account(acct).await.unwrap();
    pe.remove_account(acct).await.unwrap();
    assert_eq!(pe.stats().await.transfers, 0);

    let lim = BandwidthLimiter::new(None);
    lim.take(1024).await;
    lim.set_limit(Some(2000));
    let t0 = Instant::now();
    lim.take(2000).await;
    lim.take(500).await;
    assert!(t0.elapsed() >= Duration::from_millis(150));
}

#[tokio::test]
async fn sync_engine_vfs_upload_and_remote_changes() {
    let stage_dir = std::env::temp_dir().join(format!("opendrive-sync-stage-{}", Uuid::new_v4()));
    let (drive, client, handle) = start_drive().await;
    drive.add_file(FakeFile {
        id: "remote-1".into(),
        name: "readme.txt".into(),
        mime_type: "text/plain".into(),
        size: 11,
        parents: vec!["root".into()],
        trashed: false,
        content: b"hello world".to_vec(),
        head_revision_id: Some("rev-1".into()),
    });

    let store = Store::open_in_memory().unwrap();
    let engine = SyncEngine::new(store).with_staging_dir(&stage_dir).unwrap();
    let mut rx = engine.subscribe();
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    engine.add_account(acct).await.unwrap();
    assert!(matches!(rx.recv().await, Some(EngineEvent::AccountAdded(id)) if id == acct));

    let root_id = engine.seed_root_account(acct, "MyDrive").unwrap();
    engine.hydrate_folder(acct, root_id).await.unwrap();
    let page = engine.list(root_id, None).await.unwrap();
    assert_eq!(page.entries.len(), 1);
    let remote_attr = engine
        .lookup(root_id, OsStr::new("readme.txt"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(engine.stat(remote_attr.id).await.unwrap().size, 11);
    assert_eq!(
        engine.read(remote_attr.id, 6, 5).await.unwrap().as_ref(),
        b"world"
    );

    // Local create, write staged, read staged, upload
    let created = engine
        .create(root_id, OsStr::new("local.txt"), Kind::File)
        .await
        .unwrap();
    engine.open_write(created.id).await.unwrap();
    engine.write(created.id, 0, b"draft data").await.unwrap();
    assert_eq!(
        engine.read(created.id, 0, 10).await.unwrap().as_ref(),
        b"draft data"
    );
    assert!(engine.pending_uploads() >= 2);
    while engine.process_one_upload().await.unwrap() {}
    assert_eq!(engine.wait_for_uploads(Duration::from_secs(1)).await, 0);
    assert!(engine.stats().await.transfers >= 2);

    // Rename and remove
    engine
        .rename(created.id, root_id, OsStr::new("renamed.txt"))
        .await
        .unwrap();
    assert!(engine.process_one_upload().await.unwrap());
    assert!(engine
        .lookup(root_id, OsStr::new("renamed.txt"))
        .await
        .unwrap()
        .is_some());

    engine.remove(created.id).await.unwrap();
    assert!(engine.process_one_upload().await.unwrap());
    assert!(engine
        .lookup(root_id, OsStr::new("renamed.txt"))
        .await
        .unwrap()
        .is_none());

    // Error paths on rename, trash, enqueue_upload, drive_client_for, hydrate_folder
    let err_node = engine
        .create(root_id, OsStr::new("err.txt"), Kind::File)
        .await
        .unwrap();
    assert!(engine.process_one_upload().await.unwrap());
    drive.push_error(500);
    engine
        .rename(err_node.id, root_id, OsStr::new("err2.txt"))
        .await
        .unwrap();
    assert!(engine.process_one_upload().await.is_err());

    let err_node2 = engine
        .create(root_id, OsStr::new("err_rm.txt"), Kind::File)
        .await
        .unwrap();
    assert!(engine.process_one_upload().await.unwrap());
    drive.push_error(500);
    engine.remove(err_node2.id).await.unwrap();
    assert!(engine.process_one_upload().await.is_err());

    assert!(engine.enqueue_upload(999_999, "create", None).is_ok());
    assert!(engine.process_one_upload().await.is_err());
    assert!(engine.drive_client_for(Uuid::new_v4()).is_err());
    assert!(engine.hydrate_folder(acct, 999_999).await.is_err());
    assert!(engine
        .hydrate_folder(Uuid::new_v4(), root_id)
        .await
        .is_err());
    assert_eq!(engine.seed_root_account(acct, "Root").unwrap(), root_id);
    engine.hydrate_folder(acct, root_id).await.unwrap();

    // Resume upload with pre-populated resumable_uri and empty-file error path
    let empty_f = engine
        .create(root_id, OsStr::new("empty_err.txt"), Kind::File)
        .await
        .unwrap();
    let dc = engine.drive_client_for(acct).unwrap();
    let uri = dc
        .create_resumable(&serde_json::json!({ "name": "empty_err.txt", "parents": ["root"] }))
        .await
        .unwrap();
    {
        let st = engine.store().lock();
        let next = st.next_upload().unwrap();
        if let Some(row) = next {
            let _ = st.update_upload(row.op_id, "pending", 0, 0, Some(&uri));
        }
    }
    drive.push_error(500);
    assert!(engine.process_one_upload().await.is_err());
    let _ = empty_f;

    handle.abort();
    let _ = std::fs::remove_dir_all(&stage_dir);
}

#[tokio::test]
async fn poller_conflict_sibling_remote_remove_and_eviction() {
    let stage_dir = std::env::temp_dir().join(format!("opendrive-sync-conf-{}", Uuid::new_v4()));
    let cache_dir = std::env::temp_dir().join(format!("opendrive-sync-cache-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cache_dir).unwrap();
    let (drive, client, handle) = start_drive().await;
    drive.add_file(FakeFile {
        id: "r1".into(),
        name: "spec.md".into(),
        mime_type: "text/plain".into(),
        size: 5,
        parents: vec!["root".into()],
        trashed: false,
        content: b"v1doc".to_vec(),
        head_revision_id: Some("rev-1".into()),
    });

    let engine = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&stage_dir)
        .unwrap();
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root_id = engine.seed_root_account(acct, "Root").unwrap();
    engine.hydrate_folder(acct, root_id).await.unwrap();
    assert_eq!(engine.poll_changes(acct).await.unwrap(), 0);

    let spec = engine
        .lookup(root_id, OsStr::new("spec.md"))
        .await
        .unwrap()
        .unwrap();
    engine.open_write(spec.id).await.unwrap();
    engine.write(spec.id, 0, b"local edit").await.unwrap();
    engine.set_pinned(spec.id, true).await.unwrap();

    // Simulate remote update bumping head_revision while local is Dirty
    drive.update_metadata("r1", Some("spec.md"), None, None);
    let applied = engine.poll_changes(acct).await.unwrap();
    assert_eq!(applied, 1);

    let children = engine.list(root_id, None).await.unwrap().entries;
    assert!(children.iter().any(|c| c.name.contains("(conflict ")));

    // Simulate remote trash -> mark_drive_id_trashed
    drive.trash("r1");
    assert_eq!(engine.poll_changes(acct).await.unwrap(), 1);

    // Cache eviction deletes unpinned non-dirty file on disk
    let cache_file = cache_dir.join("evictable.bin");
    std::fs::write(&cache_file, b"0123456789").unwrap();
    let evict_node = engine
        .create(root_id, OsStr::new("evict.bin"), Kind::File)
        .await
        .unwrap();
    while engine.process_one_upload().await.unwrap() {}
    engine.set_pinned(evict_node.id, false).await.unwrap();
    let stage_blocker = stage_dir.join("999.part");
    std::fs::create_dir_all(&stage_blocker).unwrap();
    assert!(engine.open_write(999).await.is_err());
    assert!(engine.write(999, 0, b"x").await.is_err());
    drop(engine);

    let store3 = Store::open_in_memory().unwrap();
    let eng3 = SyncEngine::new(store3);
    let (d3, c3, h3) = start_drive().await;
    d3.add_file(FakeFile {
        id: "c1".into(),
        name: "c1.bin".into(),
        mime_type: "application/octet-stream".into(),
        size: 10,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![1; 10],
        head_revision_id: Some("rev-1".into()),
    });
    eng3.set_drive_client(acct, c3);
    let root3 = eng3.seed_root_account(acct, "Root").unwrap();
    eng3.hydrate_folder(acct, root3).await.unwrap();
    let c1 = eng3
        .lookup(root3, OsStr::new("c1.bin"))
        .await
        .unwrap()
        .unwrap();
    std::fs::write(&cache_file, b"0123456789").unwrap();
    {
        let st = eng3.store().lock();
        let mut node = st.get_node(c1.id as i64).unwrap().unwrap();
        node.state = NodeState::Cached;
        st.upsert_node(&node).unwrap();
        st.set_cache_entry(c1.id as i64, cache_file.to_str().unwrap(), 10, false)
            .unwrap();
    }
    assert!(cache_file.exists());
    assert_eq!(eng3.evict_to(0).unwrap(), 1);
    assert!(!cache_file.exists());
    assert_eq!(eng3.evict_to(1024).unwrap(), 0);
    eng3.set_bandwidth_limit(Some(4096));

    // Also verify subscribe + add_account + remove_account + stats
    let mut rx = eng3.subscribe();
    eng3.add_account(Uuid::new_v4()).await.unwrap();
    eng3.remove_account(acct).await.unwrap();
    assert!(rx.try_recv().is_ok());
    assert_eq!(eng3.stats().await.errors, 0);
    let _ = NodeState::Cached;

    h3.abort();
    handle.abort();
    let _ = std::fs::remove_dir_all(&stage_dir);
    let _ = std::fs::remove_dir_all(&cache_dir);
}
