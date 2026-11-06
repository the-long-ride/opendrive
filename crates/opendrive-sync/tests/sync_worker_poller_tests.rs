//! Additional tests for `SyncEngine` worker, poller, eviction, shutdown, and error paths.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{Kind, StagingDir, SyncEngine, VfsEngine};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;
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
async fn hydrate_kinds_rehydrate_and_poller_errors_and_new_items() {
    let (drive, client, handle) = start_drive().await;
    drive.add_file(FakeFile {
        id: "fld-1".into(),
        name: "Docs".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });
    drive.add_file(FakeFile {
        id: "doc-1".into(),
        name: "Roadmap".into(),
        mime_type: "application/vnd.google-apps.document".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });

    let engine = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();

    // Hydrate error paths: unknown node, missing client, HTTP error
    assert!(engine.hydrate_folder(acct, 9999).await.is_err());
    assert!(engine.hydrate_folder(Uuid::new_v4(), root).await.is_err());
    drive.push_error(500);
    assert!(engine.hydrate_folder(acct, root).await.is_err());

    // Hydrate folder + doc, then re-hydrate existing
    engine.hydrate_folder(acct, root).await.unwrap();
    engine.hydrate_folder(acct, root).await.unwrap();
    assert!(matches!(
        engine
            .lookup(root, OsStr::new("Docs"))
            .await
            .unwrap()
            .unwrap()
            .kind,
        Kind::Folder
    ));
    assert!(matches!(
        engine
            .lookup(root, OsStr::new("Roadmap"))
            .await
            .unwrap()
            .unwrap()
            .kind,
        Kind::NativeDoc
    ));

    // Poller error on startPageToken, then init token, then error on changes
    drive.push_error(500);
    assert!(engine.poll_changes(acct).await.is_err());
    assert_eq!(engine.poll_changes(acct).await.unwrap(), 0);
    drive.push_error(500);
    assert!(engine.poll_changes(acct).await.is_err());
    assert!(engine.stats().await.errors >= 2);

    // New remote folder + doc picked up via spawn_change_poller
    drive.add_file(FakeFile {
        id: "fld-2".into(),
        name: "Sub".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });
    drive.add_file(FakeFile {
        id: "doc-2".into(),
        name: "Sheet".into(),
        mime_type: "application/vnd.google-apps.spreadsheet".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });

    engine.spawn_change_poller(acct, Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(engine
        .lookup(root, OsStr::new("Sub"))
        .await
        .unwrap()
        .is_some());
    assert!(engine
        .lookup(root, OsStr::new("Sheet"))
        .await
        .unwrap()
        .is_some());
    handle.abort();
}

#[tokio::test]
async fn worker_multichunk_noop_empty_drive_id_errors_and_shutdown_timeout() {
    let stage = std::env::temp_dir().join(format!("opendrive-wstage-{}", Uuid::new_v4()));
    let (drive, client, handle) = start_drive().await;
    let engine = Arc::new(
        SyncEngine::new(Store::open_in_memory().unwrap())
            .with_staging_dir(&stage)
            .unwrap(),
    );
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();

    // Multi-chunk (>256 KiB) upload
    let big = engine
        .create(root, OsStr::new("big.bin"), Kind::File)
        .await
        .unwrap();
    engine.open_write(big.id).await.unwrap();
    let payload = vec![0x5Au8; 300 * 1024];
    engine.write(big.id, 0, &payload).await.unwrap();
    while engine.process_one_upload().await.unwrap() {}
    let saved = engine
        .store()
        .lock()
        .get_node(big.id as i64)
        .unwrap()
        .unwrap();
    assert!(!saved.drive_id.is_empty());
    assert_eq!(
        drive.get_file(&saved.drive_id).unwrap().content.len(),
        payload.len()
    );

    // Local rename + trash while drive_id is empty, plus unknown op
    let local = engine
        .create(root, OsStr::new("unpushed.txt"), Kind::File)
        .await
        .unwrap();
    // Mark initial create done manually so only rename/trash/noop run with empty drive_id
    let maybe_row = engine.store().lock().next_upload().unwrap();
    if let Some(row) = maybe_row {
        engine
            .store()
            .lock()
            .update_upload(row.op_id, "done", 0, 0, None)
            .unwrap();
    }
    engine
        .rename(local.id, root, OsStr::new("unpushed2.txt"))
        .await
        .unwrap();
    engine.remove(local.id).await.unwrap();
    engine
        .enqueue_upload(local.id as i64, "noop", None)
        .unwrap();
    while engine.process_one_upload().await.unwrap() {}

    // Error during upload + wait_for_uploads timeout
    let failing = engine
        .create(root, OsStr::new("fail.txt"), Kind::File)
        .await
        .unwrap();
    engine.write(failing.id, 0, b"data").await.unwrap();
    drive.set_forced_error(Some(500));
    assert!(engine.process_one_upload().await.is_err());
    let remaining = engine.wait_for_uploads(Duration::from_millis(25)).await;
    assert!(remaining >= 1);

    // Clear error, clear backed-off queue rows, let spawn_worker drain new upload
    drive.set_forced_error(None);
    for op_id in 1..=20 {
        let _ = engine
            .store()
            .lock()
            .update_upload(op_id, "done", 1, 0, None);
    }
    let auto = engine
        .create(root, OsStr::new("auto.txt"), Kind::File)
        .await
        .unwrap();
    engine.write(auto.id, 0, b"auto").await.unwrap();
    engine.spawn_worker(Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(engine.pending_uploads(), 0);

    // Remote download error increments errors
    assert!(engine.read(9999, 0, 4).await.is_err());
    drive.add_file(FakeFile {
        id: "remote-only".into(),
        name: "remote.bin".into(),
        mime_type: "application/octet-stream".into(),
        size: 16,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![1, 2, 3, 4],
        head_revision_id: Some("r1".into()),
    });
    engine.hydrate_folder(acct, root).await.unwrap();
    let remote_attr = engine
        .lookup(root, OsStr::new("remote.bin"))
        .await
        .unwrap()
        .unwrap();
    drive.push_error(500);
    assert!(engine.read(remote_attr.id, 0, 4).await.is_err());

    handle.abort();
    let _ = std::fs::remove_dir_all(&stage);
}

#[test]
fn eviction_partial_stop_and_staging_error() {
    let base = std::env::temp_dir().join(format!("opendrive-evict-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let blocker = base.join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    assert!(StagingDir::new(&blocker).is_err());

    let f1 = base.join("1.cache");
    let f2 = base.join("2.cache");
    std::fs::write(&f1, b"0123456789").unwrap();
    std::fs::write(&f2, b"0123456789").unwrap();

    let engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    let root = engine.seed_root_account(acct, "Root").unwrap();
    {
        let st = engine.store().lock();
        for (idx, path, access) in [(10, &f1, 1), (11, &f2, 2)] {
            st.upsert_node(&opendrive_store::Node {
                id: idx,
                account_id: acct,
                drive_id: format!("d-{idx}"),
                parent_id: Some(root as i64),
                name: format!("{idx}.bin"),
                display_name: format!("{idx}.bin"),
                kind: "file".into(),
                mime: None,
                size: 10,
                md5: None,
                modified_ms: 0,
                head_revision: None,
                trashed: false,
                shared_drive_id: None,
                state: NodeState::Cached,
                pinned: false,
                last_access_ms: access,
            })
            .unwrap();
            st.set_cache_entry(idx, path.to_str().unwrap(), 10, false)
                .unwrap();
        }
    }
    assert_eq!(engine.evict_to(10).unwrap(), 1);
    assert!(!f1.exists());
    assert!(f2.exists());
    let _ = std::fs::remove_dir_all(&base);
}
