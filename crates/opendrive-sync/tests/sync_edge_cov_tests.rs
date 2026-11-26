//! Edge-case coverage tests for upload worker ops, replica sync ops/upload, eviction, and file copy/ops.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{now_ms, Node, NodeState, Store};
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use tokio::runtime::Runtime;
use uuid::Uuid;

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

fn setup() -> (
    Runtime,
    Arc<FakeDrive>,
    String,
    Uuid,
    Arc<SyncEngine>,
    std::path::PathBuf,
) {
    let rt = Runtime::new().unwrap();
    let drive = Arc::new(FakeDrive::new());
    for (id, name, mime, par, data) in [
        ("empty1", "zero.txt", "text/plain", "root", &b""[..]),
        (
            "gdoc1",
            "Notes",
            "application/vnd.google-apps.document",
            "root",
            &b""[..],
        ),
        ("sub1", "Nested", FOLDER_MIME, "root", &b""[..]),
        (
            "inner1",
            "deep.bin",
            "application/octet-stream",
            "sub1",
            &b"abcdefg"[..],
        ),
    ] {
        drive.add_file(FakeFile {
            id: id.into(),
            name: name.into(),
            mime_type: mime.into(),
            size: data.len() as u64,
            parents: vec![par.into()],
            content: data.to_vec(),
            head_revision_id: Some("r1".into()),
            ..Default::default()
        });
    }
    let (addr, _h) = rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));
    let acct = Uuid::new_v4();
    let store = Store::open_in_memory().unwrap();
    let tmp = std::env::temp_dir().join(format!("od-sync-edge-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let eng = Arc::new(
        SyncEngine::new(store)
            .with_staging_dir(tmp.join("stage"))
            .unwrap(),
    );
    eng.set_stream_cache_dir(tmp.join("stream")).unwrap();
    eng.set_drive_client(
        acct,
        DriveClient::with_base_urls(
            "tok".into(),
            format!("http://{addr}/drive/v3"),
            format!("http://{addr}/upload/drive/v3"),
        ),
    );
    let root = eng.seed_root_account(acct, "Work").unwrap();
    rt.block_on(eng.list_explorer_nodes(acct, root, true))
        .unwrap();
    (rt, drive, addr.to_string(), acct, eng, tmp)
}

#[test]
fn worker_ops_rename_trash_unknown_and_retry_failure() {
    let (rt, drive, _addr, acct, eng, tmp) = setup();
    assert!(!rt.block_on(eng.process_one_upload()).unwrap());
    let _ = eng
        .store()
        .lock()
        .enqueue_upload(99999, "create", None)
        .unwrap();
    assert!(rt.block_on(eng.process_one_upload()).is_err());
    let _ = eng.store().lock().mark_node_uploads_done(99999);

    // Upload empty local file and non-empty local file, then exercise rename/trash/unknown ops.
    let local_zero = tmp.join("up_empty.txt");
    std::fs::write(&local_zero, b"").unwrap();
    let sum = rt
        .block_on(eng.sync_up_local_path(acct, 1, &local_zero, None))
        .unwrap();
    assert_eq!(sum.uploaded_files, 1);

    // Non-existent source error
    assert!(rt
        .block_on(eng.sync_up_local_path(acct, 1, &tmp.join("missing.txt"), None))
        .is_err());

    let children = eng.list_cached_children(1).unwrap();
    let up_node = children.iter().find(|n| n.name == "up_empty.txt").unwrap();
    {
        let st = eng.store().lock();
        let mut renamed = up_node.clone();
        renamed.name = "renamed_up.txt".into();
        renamed.display_name = "renamed_up.txt".into();
        st.upsert_node(&renamed).unwrap();
        st.enqueue_upload(renamed.id, "rename", None).unwrap();
    }
    assert!(rt.block_on(eng.process_one_upload()).unwrap());
    assert!(drive
        .get_file(&up_node.drive_id)
        .is_some_and(|f| f.name == "renamed_up.txt"));

    eng.store()
        .lock()
        .enqueue_upload(up_node.id, "unknown_noop", None)
        .unwrap();
    assert!(rt.block_on(eng.process_one_upload()).unwrap());

    eng.store()
        .lock()
        .enqueue_upload(up_node.id, "trash", None)
        .unwrap();
    assert!(rt.block_on(eng.process_one_upload()).unwrap());
    assert!(drive.get_file(&up_node.drive_id).is_some_and(|f| f.trashed));
    assert_eq!(rt.block_on(eng.flush_account_uploads(acct)).unwrap(), 0);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn replica_ops_zero_native_doc_recursive_and_copy_branches() {
    let (rt, _drive, _addr, acct, eng, tmp) = setup();
    let children = eng.list_cached_children(1).unwrap();
    let zero = children.iter().find(|n| n.name == "zero.txt").unwrap();
    let gdoc = children.iter().find(|n| n.name == "Notes").unwrap();
    let nested = children.iter().find(|n| n.name == "Nested").unwrap();

    // Missing node -> Err
    assert!(rt
        .block_on(eng.sync_down_node(acct, 1, 98765, None, &tmp))
        .is_err());
    // Native doc -> no-op
    let (gsum, gpath) = rt
        .block_on(eng.sync_down_node(acct, 1, gdoc.id as u64, None, &tmp))
        .unwrap();
    assert_eq!(gsum.downloaded_files, 0);
    assert!(gpath.is_none());

    // Zero-byte file download
    let acct_root = tmp.join("mount");
    let (zsum, zpath) = rt
        .block_on(eng.sync_down_node(acct, 1, zero.id as u64, Some(&acct_root), &tmp))
        .unwrap();
    assert_eq!(zsum.downloaded_files, 1);
    assert!(zpath.unwrap().exists());

    // Folder recursive sync down
    let (nsum, npath) = rt
        .block_on(eng.sync_down_node(acct, 1, nested.id as u64, Some(&acct_root), &tmp))
        .unwrap();
    assert_eq!(nsum.downloaded_files, 1);
    assert!(npath.unwrap().join("deep.bin").exists());

    // Recursive upload creating new subfolder + reusing existing subfolder + updating existing file
    let up_dir = tmp.join("upload_tree");
    std::fs::create_dir_all(up_dir.join("Sub")).unwrap();
    std::fs::write(up_dir.join("Sub").join("a.txt"), b"hello").unwrap();
    let u1 = rt
        .block_on(eng.sync_up_local_path(acct, 1, &up_dir, Some(&acct_root)))
        .unwrap();
    assert_eq!(u1.uploaded_files, 1);
    std::fs::write(up_dir.join("Sub").join("a.txt"), b"hello2").unwrap();
    let u2 = rt
        .block_on(eng.sync_up_local_path(acct, 1, &up_dir, Some(&acct_root)))
        .unwrap();
    assert_eq!(u2.uploaded_files, 1);

    // Offline cache fallback when Shared with me / root is listed after removing Drive client
    let swm = eng
        .list_cached_children(1)
        .unwrap()
        .into_iter()
        .find(|n| n.name == "zero.txt")
        .unwrap();
    let _ = eng.store().lock().upsert_node(&Node {
        state: NodeState::Cached,
        last_access_ms: now_ms(),
        ..swm
    });

    // Copy / move / rename / delete file & folder ops
    let copied = rt
        .block_on(eng.copy_node_everywhere(acct, 1, nested.id as u64, 1, Some(&acct_root), &tmp))
        .unwrap();
    assert_eq!(copied.name, "Nested (copy)");
    let renamed = rt
        .block_on(eng.rename_node_everywhere(
            acct,
            1,
            copied.id as u64,
            "Nested2",
            Some(&acct_root),
        ))
        .unwrap();
    assert_eq!(renamed.name, "Nested2");
    rt.block_on(eng.delete_node_everywhere(acct, 1, renamed.id as u64, Some(&acct_root)))
        .unwrap();

    // Copy single cached file, move cached file (relocating local/cache), & mismatched root sync_down fallback
    let zero_node = eng
        .list_cached_children(1)
        .unwrap()
        .into_iter()
        .find(|n| n.name == "zero.txt")
        .unwrap();
    let f_copied = rt
        .block_on(eng.copy_node_everywhere(acct, 1, zero_node.id as u64, 1, Some(&acct_root), &tmp))
        .unwrap();
    rt.block_on(eng.move_node_everywhere(
        acct,
        1,
        f_copied.id as u64,
        nested.id as u64,
        Some(&acct_root),
    ))
    .unwrap();
    let _ =
        rt.block_on(eng.sync_down_node(acct, 99999, zero_node.id as u64, Some(&acct_root), &tmp));

    // Missing node / invalid copy destination / blocker / worker fail branches
    let blocker = tmp.join("file_blocker");
    std::fs::write(&blocker, b"x").unwrap();
    assert!(SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&blocker)
        .is_err());
    let deep_id = eng.list_cached_children(nested.id as u64).unwrap()[0].id as u64;
    assert!(rt
        .block_on(eng.sync_down_node(acct, 1, deep_id, Some(&blocker), &tmp))
        .is_err());
    assert!(rt
        .block_on(eng.sync_up_local_path(acct, 1, std::path::Path::new("/"), None))
        .is_err());
    assert!(rt
        .block_on(eng.sync_down_node(acct, 1, deep_id, Some(&acct_root), &tmp))
        .is_ok());
    let _ =
        rt.block_on(eng.rename_node_everywhere(acct, 1, deep_id, "deep2.bin", Some(&acct_root)));
    let _ = rt.block_on(eng.move_node_everywhere(acct, 1, deep_id, 1, Some(&acct_root)));

    let dead_acct = Uuid::new_v4();
    let dead_eng = SyncEngine::new(Store::open_in_memory().unwrap());
    dead_eng.set_drive_client(
        dead_acct,
        DriveClient::with_base_urls(
            "t".into(),
            "http://127.0.0.1:1/drive/v3",
            "http://127.0.0.1:1/upload/drive/v3",
        ),
    );
    for (nid, sz, uri) in [(101, 4i64, None), (102, 0i64, Some("http://127.0.0.1:1/u"))] {
        let st = dead_eng.store().lock();
        let name = format!("n{nid}.bin");
        st.upsert_node(&Node {
            id: nid,
            account_id: dead_acct,
            drive_id: String::new(),
            parent_id: None,
            name: name.clone(),
            display_name: name,
            kind: "file".into(),
            mime: None,
            size: sz,
            md5: None,
            modified_ms: 0,
            head_revision: None,
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Dirty,
            pinned: false,
            last_access_ms: 0,
        })
        .unwrap();
        let _op = st.enqueue_upload(nid, "create", uri).unwrap();
        drop(st);
        assert!(rt.block_on(dead_eng.process_one_upload()).is_err());
        let _ = dead_eng.store().lock().mark_node_uploads_done(nid);
    }

    assert!(rt
        .block_on(eng.copy_node_everywhere(acct, 1, 99999, 1, None, &tmp))
        .is_err());
    assert!(rt
        .block_on(eng.move_node_everywhere(acct, 1, nested.id as u64, nested.id as u64, None))
        .is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}
