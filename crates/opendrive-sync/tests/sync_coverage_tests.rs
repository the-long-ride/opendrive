//! Coverage tests for `opendrive-sync` engine, streaming, eviction, poller, hydrate, and paths.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::{EngineEvent, StreamCache, SyncEngine, VfsEngine, STREAM_CHUNK_BYTES};
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
async fn engine_setup_errors_and_stream_cache_edge_branches() {
    let base = std::env::temp_dir().join(format!("od-sync-cov1-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let blocker = base.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();

    assert!(SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&blocker)
        .is_err());
    assert!(SyncEngine::new(Store::open_in_memory().unwrap())
        .with_stream_cache_dir(&blocker)
        .is_err());

    let _sc_default = StreamCache::default();
    let stream_dir = base.join("stream");
    let (drive, client, handle) = start_drive().await;

    let big_len = (STREAM_CHUNK_BYTES as usize) * 26 + 32;
    let big_payload = vec![0xABu8; big_len];
    drive.add_file(ffile(
        "f-0",
        "empty.bin",
        "application/octet-stream",
        "root",
        &[],
    ));
    drive.add_file(ffile(
        "f-small",
        "small.bin",
        "application/octet-stream",
        "root",
        b"0123456789abcdef",
    ));
    drive.add_file(ffile("f-big", "big.bin", "video/mp4", "root", &big_payload));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    eng.set_stream_cache_dir(&stream_dir).unwrap();
    assert!(eng.set_stream_cache_dir(&blocker).is_err());
    eng.set_bandwidth_limit(Some(100_000_000));
    eng.set_bandwidth_limit(None);

    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    assert_eq!(eng.seed_root_account(acct, "Root").unwrap(), root);
    eng.hydrate_folder(acct, root).await.unwrap();

    let id_empty = eng.resolve_remote_path(root, "empty.bin").unwrap();
    let id_small = eng.resolve_remote_path(root, "small.bin").unwrap();
    let id_big = eng.resolve_remote_path(root, "big.bin").unwrap();

    assert!(eng.prewarm_head_and_tail(99999).await.is_err());
    eng.prewarm_head_and_tail(id_empty).await.unwrap();
    eng.prewarm_head_and_tail(id_small).await.unwrap();
    assert!(eng
        .read_cached_range(id_small, 0, 0, 0)
        .await
        .unwrap()
        .is_empty());
    assert!(eng
        .read_cached_range(id_small, 999, 10, 0)
        .await
        .unwrap()
        .is_empty());
    assert!(eng
        .read_cached_range(id_empty, 0, 4, 0)
        .await
        .unwrap()
        .is_empty());
    drive.push_error(500);
    assert!(eng.read_cached_range(id_empty, 0, 4, 0).await.is_err());

    // Multi-chunk boundary read across chunk 0 -> chunk 1, then >24 chunks to trigger MemChunkLru eviction.
    let boundary_off = STREAM_CHUNK_BYTES - 8;
    let cross = eng
        .read_cached_range(id_big, boundary_off, 16, 0)
        .await
        .unwrap();
    assert_eq!(cross.len(), 16);
    for c in 0..26u64 {
        let _ = eng
            .read_cached_range(id_big, c * STREAM_CHUNK_BYTES, 4, 0)
            .await
            .unwrap();
    }

    // Revision change on f-small wipes stale disk stream cache dir (.rev mismatch).
    drive.update_metadata("f-small", Some("small.bin"), None, None);
    eng.hydrate_folder(acct, root).await.unwrap();
    assert_eq!(
        eng.read_cached_range(id_small, 0, 5, 0)
            .await
            .unwrap()
            .as_ref(),
        b"01234"
    );

    // Prewarm error on tail chunk and evict_to when total <= max_bytes.
    drive.push_error(500);
    let _ = eng.prewarm_head_and_tail(id_big).await;
    assert_eq!(eng.evict_to(1024).unwrap(), 0);
    // Evict entry whose disk file was already deleted.
    {
        let st = eng.store().lock();
        st.set_cache_entry(
            id_small as i64,
            base.join("gone.chk").to_str().unwrap(),
            100,
            false,
        )
        .unwrap();
    }
    assert_eq!(eng.evict_to(0).unwrap(), 1);

    handle.abort();
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn hydrate_shared_with_me_shortcuts_and_poller_conflict_paths() {
    let stage = std::env::temp_dir().join(format!("od-sync-cov2-{}", Uuid::new_v4()));
    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("swm-dir", "SharedDir", fld, "other", &[]));
    drive.mark_shared_with_me("swm-dir");
    drive.add_file(ffile(
        "swm-file",
        "SharedDoc.txt",
        "text/plain",
        "swm-dir",
        b"hi",
    ));
    drive.mark_shared_with_me("swm-file");
    drive.add_file(ffile(
        "swm-loose",
        "LooseShared.txt",
        "text/plain",
        "",
        b"loose",
    ));
    drive.mark_shared_with_me("swm-loose");
    drive.add_file(ffile(
        "sc-good",
        "LinkToSmall",
        "application/vnd.google-apps.shortcut",
        "root",
        &[],
    ));
    drive.add_file(ffile("sc-target", "Target.txt", "text/plain", "root", b"t"));
    drive.set_shortcut("sc-good", "sc-target", "text/plain");
    drive.add_file(ffile(
        "sc-dangling",
        "BrokenLink",
        "application/vnd.google-apps.shortcut",
        "root",
        &[],
    ));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&stage)
        .unwrap();
    let mut rx = eng.subscribe();
    let acct = Uuid::new_v4();
    assert!(eng.poll_changes(acct).await.is_err());

    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    eng.hydrate_folder(acct, root).await.unwrap();
    assert!(!eng
        .placeholder_folder_rel_paths(root)
        .iter()
        .any(|p| p == std::path::Path::new("Shared with me")));

    let swm_id = eng.seed_shared_with_me_root(acct).unwrap();
    eng.hydrate_folder(acct, swm_id).await.unwrap();
    assert!(eng
        .resolve_remote_path(swm_id, "SharedDir/SharedDoc.txt")
        .is_some());
    assert!(eng
        .lookup(swm_id, OsStr::new("LooseShared.txt"))
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        eng.node_rel_path(root, root).unwrap(),
        std::path::PathBuf::new()
    );
    assert!(eng.node_rel_path(root, 99999).is_none());
    assert!(eng.resolve_remote_path(99999, "a").is_none());
    assert!(eng.resolve_remote_path(root, "./missing").is_none());

    // Poller conflict with staged bytes vs dirty without conflict + mpsc event emitted.
    eng.poll_changes(acct).await.unwrap();
    let target_id = eng.resolve_remote_path(root, "Target.txt").unwrap();
    let loose_id = eng.resolve_remote_path(swm_id, "LooseShared.txt").unwrap();
    {
        let st = eng.store().lock();
        let mut t = st.get_node(target_id as i64).unwrap().unwrap();
        t.state = NodeState::Dirty;
        t.head_revision = Some("rev-1".into());
        st.upsert_node(&t).unwrap();
        let mut l = st.get_node(loose_id as i64).unwrap().unwrap();
        l.state = NodeState::Dirty;
        l.head_revision = Some("rev-2".into());
        st.upsert_node(&l).unwrap();
    }
    eng.write(target_id, 0, b"staged-local").await.unwrap();

    // Update target on remote -> bumps rev-1 to rev-2 -> conflict sibling created with staged bytes.
    drive.update_metadata("sc-target", Some("Target.txt"), None, None);
    // Unrelated update on LooseShared with same rev-2 -> no revision conflict -> skipped.
    drive.update_metadata("swm-loose", Some("LooseShared.txt"), None, None);
    // New unparented sharedWithMe file added to changes feed.
    drive.add_file(ffile("swm-new", "NewShared.txt", "text/plain", "", b"new"));
    drive.mark_shared_with_me("swm-new");

    let applied = eng.poll_changes(acct).await.unwrap();
    assert!(applied >= 2);
    assert!(matches!(rx.try_recv(), Ok(EngineEvent::NodeChanged(0))));
    let root_kids = eng.list_cached_children(root).unwrap();
    assert!(root_kids
        .iter()
        .any(|n: &Node| n.state == NodeState::Conflict));

    handle.abort();
    let _ = std::fs::remove_dir_all(&stage);
}
