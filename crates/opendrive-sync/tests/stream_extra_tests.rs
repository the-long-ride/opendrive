//! Additional integration tests for `replica_upload`, `replica_ops`, `replica_clear`, and `stream_cache`.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{Kind, SyncEngine, VfsEngine, STREAM_CHUNK_BYTES};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("od-{tag}-{}", Uuid::new_v4()))
}

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
async fn replica_upload_recursive_existing_folder_and_flush() {
    let base = tmp("up-ex");
    let lr = base.join("local");
    let nested = lr.join("Existing").join("Nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(lr.join("Existing").join("empty.txt"), b"").unwrap();
    fs::write(nested.join("deep.bin"), b"deep-data").unwrap();

    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("d-ex", "Existing", fld, "root", b""));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    eng.hydrate_folder(acct, root).await.unwrap();

    let missing = base.join("missing");
    assert!(eng
        .sync_up_local_path(acct, root, &missing, Some(&lr))
        .await
        .is_err());

    let src = lr.join("Existing");
    let up = eng
        .sync_up_local_path(acct, root, &src, Some(&lr))
        .await
        .unwrap();
    assert_eq!((up.uploaded_files, up.bytes_transferred), (2, 9));

    let created = eng
        .create(root, OsStr::new("queued.txt"), Kind::File)
        .await
        .unwrap();
    eng.write(created.id, 0, b"queued").await.unwrap();
    assert_eq!(eng.flush_account_uploads(acct).await.unwrap(), 2);

    handle.abort();
    let _ = fs::remove_dir_all(&base);
}

#[tokio::test]
async fn replica_ops_sync_down_folder_cache_only_and_clear_folder() {
    let base = tmp("clr-ex");
    let (lr, cd) = (base.join("m"), base.join("c"));
    fs::create_dir_all(&lr).unwrap();

    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("d-1", "Docs", fld, "root", b""));
    drive.add_file(ffile("d-2", "EmptySub", fld, "d-1", b""));
    drive.add_file(ffile("f-clean", "clean.txt", "text/plain", "d-1", b"clean"));
    drive.add_file(ffile("f-zero", "zero.txt", "text/plain", "d-1", b""));
    drive.add_file(ffile(
        "doc-1",
        "Spec",
        "application/vnd.google-apps.document",
        "d-1",
        b"",
    ));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    eng.list_explorer_nodes(acct, root, true).await.unwrap();
    let top = eng.list_explorer_nodes(acct, root, false).await.unwrap();
    assert!(top.iter().any(|n| n.display_name == "Docs"));
    let docs = eng.resolve_remote_path(root, "./Docs").unwrap();
    assert_eq!(eng.node_rel_path(root, root).unwrap(), PathBuf::new());
    assert!(eng.node_rel_path(root, 999_999).is_none());
    assert!(eng
        .placeholder_folder_rel_paths(root)
        .contains(&PathBuf::from("Docs")));

    // Sync down Docs into cache_dir only (local_account_root = None)
    let (down, out) = eng
        .sync_down_node(acct, root, docs, None, &cd)
        .await
        .unwrap();
    assert!(out.is_none());
    assert_eq!(down.downloaded_files, 2);
    let doc_id = eng.resolve_remote_path(root, "Docs/Spec").unwrap();
    let doc_res = eng
        .sync_down_node(acct, root, doc_id, None, &cd)
        .await
        .unwrap();
    assert_eq!(doc_res, (Default::default(), None));

    // Sync down into lr, modify clean.txt on disk without staging, then clear folder
    eng.sync_down_node(acct, root, docs, Some(&lr), &cd)
        .await
        .unwrap();
    fs::write(lr.join("Docs").join("clean.txt"), b"modified-on-disk").unwrap();
    let clr = eng
        .sync_up_and_clear_local(acct, root, docs, Some(&lr))
        .await
        .unwrap();
    assert_eq!((clr.uploaded_files, clr.cleared_files), (1, 2));
    assert!(!lr.join("Docs").join("clean.txt").exists());
    let st_after = eng
        .store()
        .lock()
        .get_node(docs as i64)
        .unwrap()
        .unwrap()
        .state;
    assert_eq!(st_after, NodeState::Placeholder);

    // Unbacked local file without data errors on clear; empty display_name falls back to name
    let unbacked = eng
        .create(root, OsStr::new("unbacked.txt"), Kind::File)
        .await
        .unwrap();
    {
        let st = eng.store().lock();
        let mut n = st.get_node(unbacked.id as i64).unwrap().unwrap();
        n.display_name.clear();
        n.state = NodeState::Cached;
        st.upsert_node(&n).unwrap();
    }
    let rel = eng.node_rel_path(root, unbacked.id).unwrap();
    assert_eq!(rel, PathBuf::from("unbacked.txt"));
    let clr_ok = eng
        .sync_up_and_clear_local(acct, root, unbacked.id, Some(&lr))
        .await;
    assert!(clr_ok.is_ok());
    let nf = eng
        .sync_up_and_clear_local(acct, root, 999_999, Some(&lr))
        .await;
    assert!(nf.is_err());

    let clean_id = eng.resolve_remote_path(root, "Docs/clean.txt").unwrap();
    drive.push_error(500);
    let dl_err = eng
        .sync_down_node(acct, root, clean_id, Some(&lr), &cd)
        .await;
    assert!(dl_err.is_err());

    handle.abort();
    let _ = fs::remove_dir_all(&base);
}

#[tokio::test]
async fn stream_cache_disk_hit_cross_chunk_zero_size_and_rev_invalidation() {
    let sd = tmp("stream-ex");
    let (drive, client, handle) = start_drive().await;
    let total_len = (STREAM_CHUNK_BYTES as usize) + 64;
    let payload: Vec<u8> = (0..total_len).map(|i| (i % 199) as u8).collect();
    drive.add_file(ffile("v-cross", "cross.mp4", "video/mp4", "root", &payload));
    drive.add_file(ffile("v-small", "small.mp4", "video/mp4", "root", b"tiny"));
    drive.add_file(ffile("v-zero", "empty.mp4", "video/mp4", "root", b""));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    eng.set_stream_cache_dir(&sd).unwrap();
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client.clone());
    let root = eng.seed_root_account(acct, "Root").unwrap();
    eng.hydrate_folder(acct, root).await.unwrap();

    let cross = eng.resolve_remote_path(root, "cross.mp4").unwrap();
    let small = eng.resolve_remote_path(root, "small.mp4").unwrap();
    let zero = eng.resolve_remote_path(root, "empty.mp4").unwrap();

    eng.prewarm_head_and_tail(small).await.unwrap();
    eng.prewarm_head_and_tail(zero).await.unwrap();
    for (id, off, len) in [(small, 0, 0), (small, 100, 4), (zero, 0, 4)] {
        assert!(eng
            .read_cached_range(id, off, len, 0)
            .await
            .unwrap()
            .is_empty());
    }

    let off = STREAM_CHUNK_BYTES - 16;
    let span = eng.read_cached_range(cross, off, 32, 0).await.unwrap();
    assert_eq!(span.as_ref(), &payload[off as usize..off as usize + 32]);

    // Fresh engine pointing at same stream_dir hits disk cache without network transfers
    let eng2 = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_stream_cache_dir(&sd)
        .unwrap();
    eng2.set_drive_client(acct, client);
    let root2 = eng2.seed_root_account(acct, "Root").unwrap();
    eng2.hydrate_folder(acct, root2).await.unwrap();
    let cross2 = eng2.resolve_remote_path(root2, "cross.mp4").unwrap();
    assert_eq!(cross2, cross);
    let hit = eng2.read_cached_range(cross2, 0, 8, 0).await.unwrap();
    assert_eq!(hit.as_ref(), &payload[0..8]);
    assert_eq!(eng2.stats().await.transfers, 0);

    // Revision bump invalidates disk cache and writes updated .rev
    drive.update_metadata("v-cross", Some("cross.mp4"), None, None);
    eng2.hydrate_folder(acct, root2).await.unwrap();
    let miss = eng2.read_cached_range(cross2, 0, 8, 0).await.unwrap();
    assert_eq!(miss.as_ref(), &payload[0..8]);
    assert_eq!(eng2.stats().await.transfers, 1);

    eng2.evict_node_stream_chunks(cross2);
    handle.abort();
    let _ = fs::remove_dir_all(&sd);
}

#[tokio::test]
async fn sync_up_node_preserves_local_and_clear_rejects_unreadable_dirty_bytes() {
    let base = tmp("sync-up-only");
    let local_root = base.join("mnt");
    fs::create_dir_all(&local_root).unwrap();
    let (drive, client, handle) = start_drive().await;
    drive.add_file(ffile("f-1", "doc.txt", "text/plain", "root", b"orig"));
    let eng = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(base.join("st"))
        .unwrap();
    let acct = Uuid::new_v4();
    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    eng.hydrate_folder(acct, root).await.unwrap();
    let nid = eng.resolve_remote_path(root, "doc.txt").unwrap();

    fs::write(local_root.join("doc.txt"), b"edited-in-place").unwrap();
    let up = eng
        .sync_up_node(acct, root, nid, Some(&local_root))
        .await
        .unwrap();
    assert_eq!(up.uploaded_files, 1);
    assert!(local_root.join("doc.txt").exists());
    assert_eq!(drive.get_file("f-1").unwrap().content, b"edited-in-place");

    // Non-empty Dirty node without readable local/staged bytes must error instead of 0-byte wipe
    {
        let st = eng.store().lock();
        let mut n = st.get_node(nid as i64).unwrap().unwrap();
        n.state = NodeState::Dirty;
        st.upsert_node(&n).unwrap();
    }
    fs::remove_file(local_root.join("doc.txt")).unwrap();
    assert!(eng
        .sync_up_and_clear_local(acct, root, nid, Some(&local_root))
        .await
        .is_err());
    assert_eq!(drive.get_file("f-1").unwrap().content, b"edited-in-place");

    handle.abort();
    let _ = fs::remove_dir_all(&base);
}
