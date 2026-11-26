//! Targeted coverage tests for `replica_upload`, `worker`, `file_copy`, `replica_ops`, and `staging` error paths.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{Node, NodeState, Store};
use opendrive_sync::{StagingDir, SyncEngine};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

async fn start_incomplete_upload_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let put_seq = Arc::new(AtomicUsize::new(0));
    let h = tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            let seq_ref = Arc::clone(&put_seq);
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let resp = if req.starts_with("POST ") {
                    format!(
                        "HTTP/1.1 200 OK\r\nLocation: http://{addr}/up/1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                    )
                } else {
                    let step = seq_ref.fetch_add(1, Ordering::SeqCst);
                    if step < 2 {
                        "HTTP/1.1 308 Resume Incomplete\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
                    } else {
                        let json = r#"{"id":"","name":"x.txt","mimeType":"text/plain"}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                            json.len()
                        )
                    }
                };
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), h)
}

#[tokio::test]
async fn upload_incomplete_worker_chunk_errors_copy_and_staging_io() {
    let tmp = std::env::temp_dir().join(format!("od-sync-final-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();

    // 1. replica_upload incomplete metadata & empty-id errors + worker chunk upload errors.
    let (up_base, up_h) = start_incomplete_upload_server().await;
    let store = Store::open_in_memory().unwrap();
    let eng = Arc::new(
        SyncEngine::new(store)
            .with_staging_dir(tmp.join("stage"))
            .unwrap(),
    );
    let acct1 = Uuid::new_v4();
    eng.set_drive_client(
        acct1,
        DriveClient::with_base_urls("t".into(), up_base.clone(), up_base),
    );
    let root1 = eng.seed_root_account(acct1, "UpTest").unwrap();

    let up_dir = tmp.join("NewFolder");
    let up_file = tmp.join("x.txt");
    std::fs::create_dir_all(&up_dir).unwrap();
    std::fs::write(&up_file, b"hello").unwrap();

    for (p, exp) in [
        (&up_dir, "folder creation returned no metadata"),
        (&up_file, "upload completed without file metadata"),
        (&up_file, "empty file id"),
    ] {
        let err = eng
            .sync_up_local_path(acct1, root1, p, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains(exp), "{err}");
    }
    up_h.abort();

    // Worker chunk upload failures with preset resumable_uri (empty & non-empty staged payload).
    {
        let st = eng.store().lock();
        for (nid, nm, sz) in [(50i64, "zero.bin", 0i64), (51i64, "part.bin", 5i64)] {
            st.upsert_node(&Node {
                id: nid,
                account_id: acct1,
                drive_id: String::new(),
                parent_id: Some(root1 as i64),
                name: nm.into(),
                display_name: nm.into(),
                kind: "file".into(),
                mime: Some("application/octet-stream".into()),
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
        }
    }
    let stage = StagingDir::new(tmp.join("stage")).unwrap();
    stage.write_at(51, 0, b"hello").unwrap();
    let op1 = eng.enqueue_upload(50, "create", None).unwrap();
    eng.store()
        .lock()
        .update_upload(op1, "pending", 0, 0, Some("http://127.0.0.1:1/dead"))
        .unwrap();
    assert!(eng.process_one_upload().await.is_err());
    eng.store()
        .lock()
        .update_upload(op1, "done", 1, 0, None)
        .unwrap();
    let op2 = eng.enqueue_upload(51, "create", None).unwrap();
    eng.store()
        .lock()
        .update_upload(op2, "pending", 0, 0, Some("http://127.0.0.1:1/dead"))
        .unwrap();
    assert!(eng.process_one_upload().await.is_err());

    // 2. file_copy, replica_ops, and StagingDir filesystem error branches.
    let drive = Arc::new(FakeDrive::new());
    drive.add_file(FakeFile {
        id: "f-copy".into(),
        name: "doc.txt".into(),
        mime_type: "text/plain".into(),
        size: 5,
        parents: vec!["root".into()],
        trashed: false,
        content: b"hello".to_vec(),
        head_revision_id: Some("r1".into()),
    });
    let (faddr, fh) = FakeDriveServer::start(drive).await;
    let acct2 = Uuid::new_v4();
    eng.set_drive_client(
        acct2,
        DriveClient::with_base_urls(
            "t".into(),
            format!("http://{faddr}/drive/v3"),
            format!("http://{faddr}/upload/drive/v3"),
        ),
    );
    let root2 = eng.seed_root_account(acct2, "CopyTest").unwrap();
    eng.list_explorer_nodes(acct2, root2, true).await.unwrap();
    let f_nid = eng.resolve_remote_path(root2, "doc.txt").unwrap();
    let ok_cache = tmp.join("ok_cache");
    eng.sync_down_node(acct2, root2, f_nid, None, &ok_cache)
        .await
        .unwrap();

    let bad_cache = tmp.join("bad_cache");
    std::fs::create_dir_all(&bad_cache).unwrap();
    std::fs::write(bad_cache.join("files"), b"blocker").unwrap();
    assert!(eng
        .copy_node_everywhere(acct2, root2, f_nid, root2, None, &bad_cache)
        .await
        .is_err());

    let acct_root = tmp.join("acct_root");
    std::fs::create_dir_all(acct_root.join("doc (copy).txt")).unwrap();
    std::fs::create_dir_all(acct_root.join("doc.txt")).unwrap();
    assert!(eng
        .copy_node_everywhere(acct2, root2, f_nid, root2, Some(&acct_root), &ok_cache)
        .await
        .is_err());
    assert!(eng
        .sync_down_node(acct2, root2, f_nid, Some(&acct_root), &ok_cache)
        .await
        .is_err());

    let dir_stage = StagingDir::new(tmp.join("dir_stage")).unwrap();
    std::fs::create_dir_all(dir_stage.path_for(99)).unwrap();
    assert!(dir_stage.read_all(99).is_err());
    assert!(dir_stage.write_at(99, 0, b"a").is_err());
    assert!(dir_stage.remove(99).is_err());

    // 3. Extra replica_ops, replica_upload, and file_ops branches.
    let drive3 = Arc::new(FakeDrive::new());
    for (id, nm, mm, par, data) in [
        ("f-zero", "zero.txt", "text/plain", "root", &b""[..]),
        ("f-short", "short.txt", "text/plain", "root", &b"hi"[..]),
        (
            "d-ex",
            "ExistingDir",
            "application/vnd.google-apps.folder",
            "root",
            &b""[..],
        ),
        (
            "d-sub",
            "ChildDir",
            "application/vnd.google-apps.folder",
            "d-ex",
            &b""[..],
        ),
    ] {
        drive3.add_file(FakeFile {
            id: id.into(),
            name: nm.into(),
            mime_type: mm.into(),
            size: data.len() as u64,
            parents: vec![par.into()],
            content: data.to_vec(),
            head_revision_id: Some("r1".into()),
            ..Default::default()
        });
    }
    let (a3, h3) = FakeDriveServer::start(drive3).await;
    let acct3 = Uuid::new_v4();
    eng.set_drive_client(
        acct3,
        DriveClient::with_base_urls(
            "t".into(),
            format!("http://{a3}/drive/v3"),
            format!("http://{a3}/upload/drive/v3"),
        ),
    );
    let root3 = eng.seed_root_account(acct3, "OpsTest").unwrap();
    eng.list_explorer_nodes(acct3, root3, true).await.unwrap();
    let z_nid = eng.resolve_remote_path(root3, "zero.txt").unwrap();
    let s_nid = eng.resolve_remote_path(root3, "short.txt").unwrap();
    let d_ex = eng.resolve_remote_path(root3, "ExistingDir").unwrap();
    eng.list_explorer_nodes(acct3, d_ex, true).await.unwrap();
    let d_sub = eng
        .resolve_remote_path(root3, "ExistingDir/ChildDir")
        .unwrap();

    // zero.txt size=0, short.txt size>content EOF break, and blocker file for create_dir_all err.
    eng.sync_down_node(acct3, root3, z_nid, None, &ok_cache)
        .await
        .unwrap();
    {
        let st = eng.store().lock();
        let mut sn = st.get_node(s_nid as i64).unwrap().unwrap();
        sn.size = 100;
        st.upsert_node(&sn).unwrap();
    }
    eng.sync_down_node(acct3, root3, s_nid, None, &ok_cache)
        .await
        .unwrap();
    let file_blocker = tmp.join("root_is_file");
    std::fs::write(&file_blocker, b"x").unwrap();
    assert!(eng
        .sync_down_node(acct3, root3, s_nid, Some(&file_blocker), &ok_cache)
        .await
        .is_err());

    // Upload matching ExistingDir within local_account_root (hits ensure_remote_folder & root cache).
    let ops_root = tmp.join("ops_root");
    let up_ex = ops_root.join("ExistingDir");
    std::fs::create_dir_all(&up_ex).unwrap();
    std::fs::write(up_ex.join("fresh.txt"), b"new").unwrap();
    let sum = eng
        .sync_up_local_path(acct3, root3, &up_ex, Some(&ops_root))
        .await
        .unwrap();
    assert_eq!(sum.uploaded_files, 1);

    // file_ops errors: wrong account, sibling conflict, move folder into child, and local folder/file deletes.
    assert!(eng
        .delete_node_everywhere(acct1, root3, z_nid, None)
        .await
        .is_err());
    assert!(eng
        .rename_node_everywhere(acct3, root3, z_nid, "short.txt", None)
        .await
        .is_err());
    assert!(eng
        .move_node_everywhere(acct3, root3, d_ex, d_sub, None)
        .await
        .is_err());
    assert!(eng
        .move_node_everywhere(acct3, root3, s_nid, root3, None)
        .await
        .is_ok());
    let local_short = ops_root.join("short.txt");
    std::fs::write(&local_short, b"hi").unwrap();
    eng.delete_node_everywhere(acct3, root3, s_nid, Some(&ops_root))
        .await
        .unwrap();
    eng.delete_node_everywhere(acct3, root3, d_ex, Some(&ops_root))
        .await
        .unwrap();

    h3.abort();
    fh.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
