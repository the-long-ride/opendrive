//! Targeted branch coverage tests for sync file_ops, replica_ops, replica_upload, and shutdown.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::Store;
use opendrive_sync::{Kind, SyncEngine, VfsEngine};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

async fn setup_engine() -> (
    std::path::PathBuf,
    Arc<FakeDrive>,
    SyncEngine,
    Uuid,
    u64,
    tokio::task::JoinHandle<()>,
) {
    let stage = std::env::temp_dir().join(format!("od-sync-branch-{}", Uuid::new_v4()));
    let drive = Arc::new(FakeDrive::new());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let client = DriveClient::with_base_urls(
        "tok".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );
    let store = Store::open_in_memory().unwrap();
    let engine = SyncEngine::new(store).with_staging_dir(&stage).unwrap();
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root_id = engine.seed_root_account(acct, "MyDrive").unwrap();
    engine.hydrate_folder(acct, root_id).await.unwrap();
    (stage, drive, engine, acct, root_id, handle)
}

#[tokio::test]
async fn file_ops_everywhere_replica_sync_and_error_branches() {
    let (stage, drive, engine, acct, root_id, handle) = setup_engine().await;
    let local_root = stage.join("local_root");
    let cache_dir = stage.join("cache");
    std::fs::create_dir_all(local_root.join("dest")).unwrap();

    drive.add_file(FakeFile {
        id: "folder-dest".into(),
        name: "dest".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: Vec::new(),
        head_revision_id: None,
    });
    drive.add_file(FakeFile {
        id: "file-doc".into(),
        name: "note.txt".into(),
        mime_type: "text/plain".into(),
        size: 9,
        parents: vec!["root".into()],
        trashed: false,
        content: b"note-body".to_vec(),
        head_revision_id: Some("rev-1".into()),
    });
    engine.hydrate_folder(acct, root_id).await.unwrap();

    let dest = engine
        .lookup(root_id, OsStr::new("dest"))
        .await
        .unwrap()
        .unwrap();
    let file = engine
        .lookup(root_id, OsStr::new("note.txt"))
        .await
        .unwrap()
        .unwrap();

    // Sync down single file to replica + cache, then sync down folder
    let (sum_f, path_f) = engine
        .sync_down_node(acct, root_id, file.id, Some(&local_root), &cache_dir)
        .await
        .unwrap();
    assert_eq!(sum_f.downloaded_files, 1);
    assert!(path_f.unwrap().exists());

    let (sum_d, _) = engine
        .sync_down_node(acct, root_id, dest.id, Some(&local_root), &cache_dir)
        .await
        .unwrap();
    assert_eq!(sum_d.downloaded_files, 0);
    assert!(engine
        .sync_down_node(acct, root_id, 999_999, None, &cache_dir)
        .await
        .is_err());

    // Rename & move everywhere (with local replica present)
    let renamed = engine
        .rename_node_everywhere(acct, root_id, file.id, "note2.txt", Some(&local_root))
        .await
        .unwrap();
    assert_eq!(renamed.name, "note2.txt");
    assert!(local_root.join("note2.txt").exists());
    assert!(engine
        .rename_node_everywhere(acct, root_id, file.id, "", Some(&local_root))
        .await
        .is_err());

    let moved = engine
        .move_node_everywhere(acct, root_id, file.id, dest.id, Some(&local_root))
        .await
        .unwrap();
    assert_eq!(moved.parent_id, Some(dest.id as i64));
    assert!(local_root.join("dest").join("note2.txt").exists());
    assert!(engine
        .move_node_everywhere(acct, root_id, file.id, file.id, Some(&local_root))
        .await
        .is_err());

    // Delete everywhere cleans both remote and local replica
    engine
        .delete_node_everywhere(acct, root_id, file.id, Some(&local_root))
        .await
        .unwrap();
    assert!(!local_root.join("dest").join("note2.txt").exists());
    assert!(engine
        .delete_node_everywhere(acct, root_id, 999_999, None)
        .await
        .is_err());

    handle.abort();
    let _ = std::fs::remove_dir_all(&stage);
}

#[tokio::test]
async fn replica_upload_folder_tree_and_shutdown_timeout_branches() {
    let (stage, _drive, engine, acct, root_id, handle) = setup_engine().await;
    let src_dir = stage.join("upload_tree");
    let nested = src_dir.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(src_dir.join("a.txt"), b"alpha").unwrap();
    std::fs::write(nested.join("b.txt"), b"beta").unwrap();

    let sum = engine
        .sync_up_local_path(acct, root_id, &src_dir, None)
        .await
        .unwrap();
    assert!(sum.uploaded_files >= 2);

    // Update existing file via sync_up_local_path + missing source error branch
    std::fs::write(src_dir.join("a.txt"), b"alpha-v2").unwrap();
    let sum2 = engine
        .sync_up_local_path(acct, root_id, &src_dir.join("a.txt"), Some(&stage))
        .await
        .unwrap();
    assert_eq!(sum2.uploaded_files, 1);
    assert!(engine
        .sync_up_local_path(acct, root_id, &stage.join("missing.bin"), None)
        .await
        .is_err());

    // Queue an upload and verify wait_for_uploads times out cleanly while non-empty
    let created = engine
        .create(root_id, OsStr::new("queued.txt"), Kind::File)
        .await
        .unwrap();
    engine.open_write(created.id).await.unwrap();
    engine.write(created.id, 0, b"pending").await.unwrap();
    assert!(engine.pending_uploads() >= 1);
    let rem = engine.wait_for_uploads(Duration::from_millis(40)).await;
    assert!(rem >= 1);

    handle.abort();
    let _ = std::fs::remove_dir_all(&stage);
}
