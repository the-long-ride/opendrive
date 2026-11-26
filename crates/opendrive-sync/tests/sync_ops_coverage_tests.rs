//! Coverage tests for `opendrive-sync` replica ops, upload, and local clear branches.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
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
async fn replica_ops_upload_and_clear_branches() {
    let base = std::env::temp_dir().join(format!("od-sync-ops1-{}", Uuid::new_v4()));
    let (mnt, cache, stage) = (base.join("mnt"), base.join("cache"), base.join("stage"));
    std::fs::create_dir_all(&mnt).unwrap();

    let (drive, client, handle) = start_drive().await;
    let fld = "application/vnd.google-apps.folder";
    drive.add_file(ffile("d-sub", "Sub", fld, "root", &[]));
    drive.add_file(ffile("f-empty", "empty.txt", "text/plain", "root", &[]));
    drive.add_file(ffile(
        "doc-1",
        "NotesDoc",
        "application/vnd.google-apps.document",
        "root",
        &[],
    ));
    drive.add_file(ffile("f-a", "a.txt", "text/plain", "d-sub", b"alpha"));

    let eng = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&stage)
        .unwrap();
    let acct = Uuid::new_v4();
    assert!(eng
        .sync_up_local_path(acct, 1, &base.join("missing"), None)
        .await
        .is_err());
    assert!(eng.sync_up_local_path(acct, 1, &mnt, None).await.is_err());

    eng.set_drive_client(acct, client);
    let root = eng.seed_root_account(acct, "Root").unwrap();
    assert!(eng.list_explorer_nodes(acct, 99999, false).await.is_err());
    assert!(eng
        .sync_down_node(acct, root, 99999, None, &cache)
        .await
        .is_err());

    let top = eng.list_explorer_nodes(acct, root, true).await.unwrap();
    let id_of = |name: &str| top.iter().find(|n| n.name == name).unwrap().id as u64;
    let (sub_id, empty_id, doc_id) = (id_of("Sub"), id_of("empty.txt"), id_of("NotesDoc"));

    // Cached fast-path on subfolder and fallback to cached children when root hydrate errors.
    let sub_kids = eng.list_explorer_nodes(acct, sub_id, false).await.unwrap();
    assert_eq!(sub_kids.len(), 1);
    let a_id = sub_kids[0].id as u64;
    assert_eq!(
        eng.list_explorer_nodes(acct, sub_id, false)
            .await
            .unwrap()
            .len(),
        1
    );
    drive.push_error(500);
    assert!(!eng
        .list_explorer_nodes(acct, root, false)
        .await
        .unwrap()
        .is_empty());

    // Native doc sync_down is a no-op; zero-byte & folder sync_down to cache_dir (None local root).
    let (doc_sum, doc_p) = eng
        .sync_down_node(acct, root, doc_id, None, &cache)
        .await
        .unwrap();
    assert_eq!((doc_sum.downloaded_files, doc_p), (0, None));
    let (e_sum, e_p) = eng
        .sync_down_node(acct, root, empty_id, None, &cache)
        .await
        .unwrap();
    assert_eq!(e_sum.downloaded_files, 1);
    assert!(e_p.unwrap().exists());

    // sync_down download error cleans up tmp file; folder sync_down succeeds.
    drive.push_error(500);
    assert!(eng
        .sync_down_node(acct, root, a_id, None, &cache)
        .await
        .is_err());
    let (dir_sum, dir_p) = eng
        .sync_down_node(acct, root, sub_id, None, &cache)
        .await
        .unwrap();
    assert_eq!(dir_sum.downloaded_files, 1);
    assert!(dir_p.is_none());

    // sync_up_local_path: existing remote folder + nested subdir + 0-byte + multi-chunk + existing file update.
    let up_sub = mnt.join("Sub");
    std::fs::create_dir_all(up_sub.join("Nested")).unwrap();
    std::fs::write(up_sub.join("a.txt"), b"updated-alpha").unwrap();
    std::fs::write(up_sub.join("Nested").join("zero.txt"), b"").unwrap();
    std::fs::write(up_sub.join("big.bin"), vec![0x7Cu8; 300 * 1024]).unwrap();
    let up = eng
        .sync_up_local_path(acct, root, &up_sub, Some(&mnt))
        .await
        .unwrap();
    assert_eq!(up.uploaded_files, 3);

    // sync_up_node & sync_up_and_clear_local on folder + clean cached file + empty-drive 0-byte vs dirty error.
    let su = eng
        .sync_up_node(acct, root, sub_id, Some(&mnt))
        .await
        .unwrap();
    assert_eq!(su.uploaded_files, 0);
    let clr = eng
        .sync_up_and_clear_local(acct, root, sub_id, Some(&mnt))
        .await
        .unwrap();
    assert!(clr.cleared_files >= 2);
    assert_eq!(
        eng.get_node(sub_id).unwrap().unwrap().state,
        NodeState::Placeholder
    );

    let local_zero = eng
        .create(root, OsStr::new("lzero.txt"), Kind::File)
        .await
        .unwrap();
    let _ = eng
        .store()
        .lock()
        .mark_node_uploads_done(local_zero.id as i64);
    assert_eq!(
        eng.sync_up_and_clear_local(acct, root, local_zero.id, Some(&mnt))
            .await
            .unwrap()
            .uploaded_files,
        1
    );

    let local_bad = eng
        .create(root, OsStr::new("lbad.txt"), Kind::File)
        .await
        .unwrap();
    {
        let st = eng.store().lock();
        let _ = st.mark_node_uploads_done(local_bad.id as i64);
        let mut n = st.get_node(local_bad.id as i64).unwrap().unwrap();
        n.size = 10;
        st.upsert_node(&n).unwrap();
    }
    assert!(eng
        .sync_up_and_clear_local(acct, root, local_bad.id, Some(&mnt))
        .await
        .is_err());

    handle.abort();
    let _ = std::fs::remove_dir_all(&base);
}
