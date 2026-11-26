//! Regression tests for remote deletion pruning and stale cache eviction on hydration/poll.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{Kind, SyncEngine, VfsEngine};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_drive() -> (Arc<FakeDrive>, DriveClient, tokio::task::JoinHandle<()>) {
    let drive = Arc::new(FakeDrive::new());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let client = DriveClient::with_base_urls(
        "tok".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );
    (drive, client, handle)
}

fn add_fake(drive: &FakeDrive, id: &str, name: &str, mime: &str, data: &[u8]) {
    drive.add_file(FakeFile {
        id: id.into(),
        name: name.into(),
        mime_type: mime.into(),
        size: data.len() as u64,
        parents: vec!["root".into()],
        content: data.to_vec(),
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
}

#[tokio::test]
async fn hydrate_folder_prunes_deleted_clean_children_and_keeps_dirty() {
    let tmp = std::env::temp_dir().join(format!("od-prune-{}", Uuid::new_v4()));
    let (drive, client, handle) = setup_drive().await;
    add_fake(&drive, "f-keep", "keep.txt", "text/plain", b"keep");
    add_fake(&drive, "f-gone", "gone.txt", "text/plain", b"gone");
    add_fake(&drive, "f-dirty", "dirty.txt", "text/plain", b"dirty");

    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    let initial = eng.list_explorer_nodes(acct, root, true).await.unwrap();
    let id_of = |name: &str| initial.iter().find(|n| n.name == name).unwrap().id as u64;
    let (gone_id, dirty_id) = (id_of("gone.txt"), id_of("dirty.txt"));

    let (_, gone_p) = eng
        .sync_down_node(acct, root, gone_id, None, &tmp)
        .await
        .unwrap();
    assert!(gone_p.is_some());
    let cp = eng
        .store()
        .lock()
        .get_cache_path(gone_id as i64)
        .unwrap()
        .unwrap();
    assert!(Path::new(&cp).exists());

    eng.create(root, OsStr::new("local-new.txt"), Kind::File)
        .await
        .unwrap();
    eng.store()
        .lock()
        .update_node_state(dirty_id as i64, NodeState::Dirty, None)
        .unwrap();

    assert!(drive.trash("f-gone"));
    assert!(drive.trash("f-dirty"));

    let refreshed = eng.list_explorer_nodes(acct, root, true).await.unwrap();
    assert!(refreshed.iter().any(|n| n.name == "keep.txt"));
    assert!(refreshed.iter().any(|n| n.name == "dirty.txt"));
    assert!(refreshed.iter().any(|n| n.name == "local-new.txt"));
    assert!(refreshed.iter().any(|n| n.name == "Shared with me"));
    assert!(!refreshed.iter().any(|n| n.name == "gone.txt"));

    assert!(!Path::new(&cp).exists());
    assert!(eng
        .store()
        .lock()
        .get_cache_path(gone_id as i64)
        .unwrap()
        .is_none());
    assert!(eng.get_node(gone_id).unwrap().unwrap().trashed);

    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn reconcile_cached_state_evicts_stale_cache_on_hydrate_and_poller() {
    let tmp = std::env::temp_dir().join(format!("od-reconcile-{}", Uuid::new_v4()));
    let (drive, client, handle) = setup_drive().await;
    add_fake(&drive, "v1", "v1.mp4", "video/mp4", b"video-one");
    add_fake(&drive, "v2", "v2.mp4", "video/mp4", b"video-two");

    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    let items = eng.list_explorer_nodes(acct, root, true).await.unwrap();
    let v1 = items.iter().find(|n| n.name == "v1.mp4").unwrap().id as u64;
    let v2 = items.iter().find(|n| n.name == "v2.mp4").unwrap().id as u64;

    eng.sync_down_node(acct, root, v1, None, &tmp)
        .await
        .unwrap();
    eng.sync_down_node(acct, root, v2, None, &tmp)
        .await
        .unwrap();
    let cp1 = eng
        .store()
        .lock()
        .get_cache_path(v1 as i64)
        .unwrap()
        .unwrap();
    let cp2 = eng
        .store()
        .lock()
        .get_cache_path(v2 as i64)
        .unwrap()
        .unwrap();
    assert!(Path::new(&cp1).exists() && Path::new(&cp2).exists());

    eng.poll_changes(acct).await.unwrap();
    drive.update_metadata("v1", Some("v1.mp4"), None, None);
    eng.hydrate_folder(acct, root).await.unwrap();
    assert_eq!(
        eng.get_node(v1).unwrap().unwrap().state,
        NodeState::Placeholder
    );
    assert!(eng
        .store()
        .lock()
        .get_cache_path(v1 as i64)
        .unwrap()
        .is_none());
    assert!(!Path::new(&cp1).exists());

    drive.update_metadata("v2", Some("v2.mp4"), None, None);
    assert!(eng.poll_changes(acct).await.unwrap() >= 1);
    assert_eq!(
        eng.get_node(v2).unwrap().unwrap().state,
        NodeState::Placeholder
    );
    assert!(eng
        .store()
        .lock()
        .get_cache_path(v2 as i64)
        .unwrap()
        .is_none());
    assert!(!Path::new(&cp2).exists());

    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
