//! Coverage tests for `opendrive-sync` worker, file_copy, and file_ops edge branches.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::Store;
use opendrive_sync::{Kind, SyncEngine, VfsEngine};
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

#[tokio::test]
async fn worker_copy_and_file_ops_remaining_branches() {
    let base = std::env::temp_dir().join(format!("od-sync-ops2-{}", Uuid::new_v4()));
    let (cache, stage) = (base.join("cache"), base.join("stage"));
    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("d-dst", "Dest", fld, "root", &[]));
    drive.add_file(ffile("f-one", "plain", "text/plain", "root", b"hello"));
    drive.add_file(ffile("f-two", "other.txt", "text/plain", "root", b"world"));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&stage)
        .unwrap();
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    let top = eng.list_explorer_nodes(acct, root, true).await.unwrap();
    let id_of = |name: &str| top.iter().find(|n| n.name == name).unwrap().id as u64;
    let (dst_id, f1_id, f2_id) = (id_of("Dest"), id_of("plain"), id_of("other.txt"));

    // Worker: 0-byte create, non-empty drive_id rename & trash, deleted node error.
    let z = eng
        .create(root, OsStr::new("z.txt"), Kind::File)
        .await
        .unwrap();
    assert!(eng.process_one_upload().await.unwrap());
    eng.rename(z.id, root, OsStr::new("z2.txt")).await.unwrap();
    assert!(eng.process_one_upload().await.unwrap());
    eng.remove(z.id).await.unwrap();
    assert!(eng.process_one_upload().await.unwrap());
    assert!(
        drive
            .get_file(&eng.get_node(z.id).unwrap().unwrap().drive_id)
            .unwrap()
            .trashed
    );

    let ghost_op = eng.enqueue_upload(88888, "create", None).unwrap();
    assert!(eng.process_one_upload().await.is_err());
    eng.store()
        .lock()
        .update_upload(ghost_op, "done", 1, 0, None)
        .unwrap();

    // Copy without local_account_root, 3x disambiguation, folder without ext, dirty staged upload copy.
    eng.sync_down_node(acct, root, f1_id, None, &cache)
        .await
        .unwrap();
    let c1 = eng
        .copy_node_everywhere(acct, root, f1_id, root, None, &cache)
        .await
        .unwrap();
    let c2 = eng
        .copy_node_everywhere(acct, root, f1_id, root, None, &cache)
        .await
        .unwrap();
    let c3 = eng
        .copy_node_everywhere(acct, root, f1_id, root, None, &cache)
        .await
        .unwrap();
    assert_eq!(
        (c1.name.as_str(), c2.name.as_str(), c3.name.as_str()),
        ("plain (copy)", "plain (copy 2)", "plain (copy 3)")
    );

    let d_copy = eng
        .copy_node_everywhere(acct, root, dst_id, root, None, &cache)
        .await
        .unwrap();
    assert_eq!(d_copy.name, "Dest (copy)");

    eng.open_write(f2_id).await.unwrap();
    eng.write(f2_id, 0, b"dirty-copy").await.unwrap();
    let dirty_copy = eng
        .copy_node_everywhere(acct, root, f2_id, dst_id, None, &cache)
        .await
        .unwrap();
    assert_eq!(
        drive.get_file(&dirty_copy.drive_id).unwrap().content,
        b"dirty-copy"
    );

    // file_ops no-ops, conflict errors, invalid names, non-folder dest, local-only delete.
    assert_eq!(
        eng.rename_node_everywhere(acct, root, f1_id, "plain", None)
            .await
            .unwrap()
            .id as u64,
        f1_id
    );
    assert!(eng
        .rename_node_everywhere(acct, root, f1_id, "other.txt", None)
        .await
        .is_err());
    assert!(eng
        .rename_node_everywhere(acct, root, f1_id, "..", None)
        .await
        .is_err());
    assert_eq!(
        eng.move_node_everywhere(acct, root, f1_id, root, None)
            .await
            .unwrap()
            .id as u64,
        f1_id
    );
    assert!(eng
        .move_node_everywhere(acct, root, f1_id, f2_id, None)
        .await
        .is_err());
    assert!(eng
        .move_node_everywhere(acct, root, dirty_copy.id as u64, root, None)
        .await
        .is_err());

    let loc_only = eng
        .create(root, OsStr::new("local-only.txt"), Kind::File)
        .await
        .unwrap();
    eng.write(loc_only.id, 0, b"staged").await.unwrap();
    eng.delete_node_everywhere(acct, root, loc_only.id, None)
        .await
        .unwrap();
    assert!(eng.get_node(loc_only.id).unwrap().unwrap().trashed);

    handle.abort();
    let _ = std::fs::remove_dir_all(&base);
}
