//! Integration tests for stream caching, path resolution, hydration preservation, and replica ops.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{SyncEngine, VfsEngine, STREAM_CHUNK_BYTES};
use std::ffi::OsStr;
use std::fs;
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

fn ffile(id: &str, name: &str, mime: &str, parent: &str, data: &[u8]) -> FakeFile {
    FakeFile {
        id: id.into(),
        name: name.into(),
        mime_type: mime.into(),
        size: data.len() as u64,
        parents: vec![parent.into()],
        trashed: false,
        content: data.to_vec(),
        head_revision_id: Some("r1".into()),
    }
}

#[tokio::test]
async fn hydrate_preserves_cached_and_dirty_and_resolves_paths() {
    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("dir-1", "Media", fld, "root", b""));
    drive.add_file(ffile(
        "f-cached",
        "Clip.mp4",
        "video/mp4",
        "dir-1",
        b"0123456789",
    ));
    drive.add_file(ffile(
        "f-dirty",
        "Notes.txt",
        "text/plain",
        "dir-1",
        b"hello",
    ));

    let engine = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();
    engine.hydrate_folder(acct, root).await.unwrap();

    let media_id = engine.resolve_remote_path(root, "/media").unwrap();
    engine.hydrate_folder(acct, media_id).await.unwrap();
    let clip_id = engine.resolve_remote_path(root, "Media/clip.mp4").unwrap();
    let notes_id = engine
        .resolve_remote_path(root, "Media\\Notes.txt")
        .unwrap();
    assert_eq!(
        engine.node_rel_path(root, clip_id).unwrap(),
        std::path::PathBuf::from("Media").join("Clip.mp4")
    );
    assert!(engine.resolve_remote_path(root, "../bad").is_none());

    {
        let st = engine.store().lock();
        let mut clip = st.get_node(clip_id as i64).unwrap().unwrap();
        clip.state = NodeState::Cached;
        clip.pinned = true;
        clip.last_access_ms = 4242;
        st.upsert_node(&clip).unwrap();

        let mut notes = st.get_node(notes_id as i64).unwrap().unwrap();
        notes.state = NodeState::Dirty;
        notes.size = 999;
        st.upsert_node(&notes).unwrap();
    }

    engine.hydrate_folder(acct, media_id).await.unwrap();
    {
        let st = engine.store().lock();
        let clip_after = st.get_node(clip_id as i64).unwrap().unwrap();
        assert_eq!(clip_after.state, NodeState::Cached);
        assert!(clip_after.pinned);
        assert_eq!(clip_after.last_access_ms, 4242);
        let notes_after = st.get_node(notes_id as i64).unwrap().unwrap();
        assert_eq!(notes_after.state, NodeState::Dirty);
        assert_eq!(notes_after.size, 999);
    }

    drive.update_metadata("f-cached", Some("Clip.mp4"), None, None);
    engine.hydrate_folder(acct, media_id).await.unwrap();
    assert_eq!(
        engine
            .store()
            .lock()
            .get_node(clip_id as i64)
            .unwrap()
            .unwrap()
            .state,
        NodeState::Placeholder
    );
    handle.abort();
}

#[tokio::test]
async fn stream_cache_prewarm_single_flight_read_ahead_and_eviction() {
    let stream_dir = std::env::temp_dir().join(format!("opendrive-stream-{}", Uuid::new_v4()));
    let (drive, client, handle) = start_drive().await;
    let total_len = (STREAM_CHUNK_BYTES as usize) * 2 + 128;
    let payload: Vec<u8> = (0..total_len).map(|i| (i % 251) as u8).collect();
    drive.add_file(ffile("vid-1", "movie.mp4", "video/mp4", "root", &payload));

    let engine = Arc::new(
        SyncEngine::new(Store::open_in_memory().unwrap())
            .with_stream_cache_dir(&stream_dir)
            .unwrap(),
    );
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();
    engine.hydrate_folder(acct, root).await.unwrap();
    let vid = engine
        .lookup(root, OsStr::new("movie.mp4"))
        .await
        .unwrap()
        .unwrap();

    engine.prewarm_head_and_tail(vid.id).await.unwrap();
    assert_eq!(engine.stats().await.transfers, 2);
    assert!(stream_dir.join(vid.id.to_string()).join("0.chk").exists());
    assert!(stream_dir.join(vid.id.to_string()).join("2.chk").exists());

    assert_eq!(
        engine.read(vid.id, 10, 32).await.unwrap().as_ref(),
        &payload[10..42]
    );
    let tail_off = total_len as u64 - 64;
    assert_eq!(
        engine
            .read_cached_range(vid.id, tail_off, 64, 0)
            .await
            .unwrap()
            .as_ref(),
        &payload[tail_off as usize..]
    );
    assert_eq!(engine.stats().await.transfers, 2);

    let mid = STREAM_CHUNK_BYTES + 16;
    let (a, b) = tokio::join!(
        engine.read_cached_range(vid.id, mid, 32, 0),
        engine.read_cached_range(vid.id, mid + 8, 32, 0)
    );
    assert_eq!(
        a.unwrap().as_ref(),
        &payload[mid as usize..mid as usize + 32]
    );
    assert_eq!(
        b.unwrap().as_ref(),
        &payload[mid as usize + 8..mid as usize + 40]
    );
    assert_eq!(engine.stats().await.transfers, 3);

    engine.evict_node_stream_chunks(vid.id);
    assert!(!stream_dir.join(vid.id.to_string()).exists());
    let _ = engine.read_cached_range(vid.id, 0, 16, 1).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(stream_dir.join(vid.id.to_string()).join("1.chk").exists());

    handle.abort();
    let _ = fs::remove_dir_all(&stream_dir);
}

#[tokio::test]
async fn explorer_sync_down_sync_up_and_clear_safety_invariant_3() {
    let base = std::env::temp_dir().join(format!("opendrive-rep-{}", Uuid::new_v4()));
    let (local_root, cache_dir, stage_dir) = (base.join("m"), base.join("c"), base.join("s"));
    fs::create_dir_all(&local_root).unwrap();

    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("f-a", "a.txt", "text/plain", "root", b"alpha"));
    drive.add_file(ffile("d-sub", "Sub", fld, "root", b""));
    drive.add_file(ffile("f-b", "b.txt", "text/plain", "d-sub", b"beta"));

    let engine = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&stage_dir)
        .unwrap();
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, client);
    let root = engine.seed_root_account(acct, "Root").unwrap();

    let nodes = engine.list_explorer_nodes(acct, root, true).await.unwrap();
    assert_eq!(nodes[0].kind, "folder");
    let (down, _) = engine
        .sync_down_node(acct, root, root, Some(&local_root), &cache_dir)
        .await
        .unwrap();
    assert_eq!(down.downloaded_files, 2);
    assert_eq!(fs::read(local_root.join("a.txt")).unwrap(), b"alpha");
    assert_eq!(
        fs::read(local_root.join("Sub").join("b.txt")).unwrap(),
        b"beta"
    );

    let up_src = base.join("UploadFolder");
    fs::create_dir_all(&up_src).unwrap();
    fs::write(up_src.join("up.bin"), b"uploaded-bytes").unwrap();
    let up = engine
        .sync_up_local_path(acct, root, &up_src, Some(&local_root))
        .await
        .unwrap();
    assert_eq!(up.uploaded_files, 1);

    let a_id = engine.resolve_remote_path(root, "a.txt").unwrap();
    engine.open_write(a_id).await.unwrap();
    engine.write(a_id, 0, b"edited-local").await.unwrap();
    drive.push_error(500);
    assert!(engine
        .sync_up_and_clear_local(acct, root, a_id, Some(&local_root))
        .await
        .is_err());
    assert!(local_root.join("a.txt").exists());
    assert_eq!(
        engine
            .store()
            .lock()
            .get_node(a_id as i64)
            .unwrap()
            .unwrap()
            .state,
        NodeState::Dirty
    );

    let clr = engine
        .sync_up_and_clear_local(acct, root, a_id, Some(&local_root))
        .await
        .unwrap();
    assert_eq!((clr.uploaded_files, clr.cleared_files), (1, 1));
    assert!(!local_root.join("a.txt").exists());
    let a_node = engine
        .store()
        .lock()
        .get_node(a_id as i64)
        .unwrap()
        .unwrap();
    assert_eq!(a_node.state, NodeState::Placeholder);
    assert!(!a_node.pinned);

    handle.abort();
    let _ = fs::remove_dir_all(&base);
}
