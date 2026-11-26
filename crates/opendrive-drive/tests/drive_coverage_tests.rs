//! Coverage tests for `opendrive-drive` client endpoints and HTTP/JSON error branches.

use opendrive_drive::{DriveClient, DriveError};
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn setup() -> (Arc<FakeDrive>, DriveClient, tokio::task::JoinHandle<()>) {
    let drive = Arc::new(FakeDrive::default());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let client = DriveClient::with_base_urls(
        "tok-cov".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );
    (drive, client, handle)
}

async fn start_one_shot_http(status: &str, body: &str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let handle = tokio::spawn(async move {
        if let Ok((mut s, _)) = listener.accept().await {
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf).await;
            let _ = s.write_all(resp.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn shared_with_me_shared_drives_none_and_download_bytes() {
    let (drive, client, handle) = setup().await;
    drive.add_shared_drive("sd-a", "Team");
    drive.add_file(FakeFile {
        id: "f-swm".into(),
        name: "shared.txt".into(),
        mime_type: "text/plain".into(),
        size: 5,
        parents: vec![],
        trashed: false,
        content: b"hello".to_vec(),
        head_revision_id: Some("r1".into()),
    });

    let drives = client.list_shared_drives(None).await.unwrap();
    assert_eq!(drives.drives.len(), 1);
    let swm = client.list_shared_with_me(None).await.unwrap();
    assert!(swm.files.is_empty());
    let swm2 = client.list_shared_with_me(Some("tok-2")).await.unwrap();
    assert!(swm2.files.is_empty());
    drive.patch_parents("f-swm", Some("p1,p1,p2"), None, None);
    assert_eq!(
        DriveClient::native_doc_link("abc"),
        "https://drive.google.com/open?id=abc"
    );

    let (ok_url, h1) = start_one_shot_http("200 OK", "raw-bytes").await;
    assert_eq!(client.download_bytes(&ok_url).await.unwrap(), b"raw-bytes");
    h1.abort();

    let (nf_url, h2) = start_one_shot_http("404 Not Found", "nope").await;
    assert!(matches!(
        client.download_bytes(&nf_url).await,
        Err(DriveError::NotFound)
    ));
    h2.abort();

    // Unreachable port triggers reqwest send error for download_bytes and download_range.
    let dead_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = dead_listener.local_addr().unwrap();
    drop(dead_listener);
    assert!(matches!(
        client
            .download_bytes(&format!("http://{dead_addr}/x"))
            .await,
        Err(DriveError::Http(_))
    ));
    let dead_client = DriveClient::with_base_urls(
        "t".into(),
        format!("http://{dead_addr}/drive/v3"),
        format!("http://{dead_addr}/upload/drive/v3"),
    );
    assert!(matches!(
        dead_client.download_range("f1", 0, 10).await,
        Err(DriveError::Http(_))
    ));

    // Malformed JSON triggers send_json decode error.
    let (bad_json_url, h3) = start_one_shot_http("200 OK", "{not-json").await;
    let bad_client = DriveClient::with_base_urls("t".into(), bad_json_url.clone(), bad_json_url);
    assert!(matches!(
        bad_client.list_shared_drives(None).await,
        Err(DriveError::Http(_))
    ));
    h3.abort();

    // 404 on download_range propagates NotFound.
    drive.push_error(404);
    assert!(matches!(
        client.download_range("f-swm", 0, 4).await,
        Err(DriveError::NotFound)
    ));

    handle.abort();
}

async fn start_truncated_http(
    declared_len: usize,
    partial: &[u8],
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let head =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {declared_len}\r\nConnection: close\r\n\r\n");
    let body = partial.to_vec();
    let handle = tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf).await;
            let _ = s.write_all(head.as_bytes()).await;
            let _ = s.write_all(&body).await;
            let _ = s.shutdown().await;
        }
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn truncated_body_errors_and_fake_drive_extras_coverage() {
    let (trunc_url, h_trunc) = start_truncated_http(500, b"hi").await;
    let client = DriveClient::with_base_urls("tok".into(), trunc_url.clone(), trunc_url.clone());
    assert!(matches!(
        client.download_bytes(&format!("{trunc_url}/raw")).await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(
        client.download_range("f1", 0, 10).await,
        Err(DriveError::Http(_))
    ));
    h_trunc.abort();

    let drive = FakeDrive::new();
    drive.set_photo_url(Some("https://example.com/a.png".into()));
    assert_eq!(
        drive.photo_url().as_deref(),
        Some("https://example.com/a.png")
    );
    drive.set_static_asset("/a.png", vec![1, 2, 3]);
    assert_eq!(drive.get_static_asset("/a.png"), Some(vec![1, 2, 3]));
    drive.set_root_real_id("real-root");
    assert_eq!(drive.root_real_id().as_deref(), Some("real-root"));

    drive.add_file(FakeFile {
        id: "folder-1".into(),
        name: "Dir".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: None,
    });
    drive.add_file(FakeFile {
        id: "orphan-1".into(),
        name: "SharedNote.txt".into(),
        mime_type: "text/plain".into(),
        size: 3,
        parents: vec![],
        trashed: false,
        content: b"abc".to_vec(),
        head_revision_id: None,
    });
    drive.mark_shared_with_me("orphan-1");
    drive.mark_unowned("orphan-1");
    drive.set_shortcut("sc-1", "orphan-1", "text/plain");
    assert_eq!(drive.list_shared_with_me().len(), 1);
    assert!(drive.list_not_in_root().iter().any(|f| f.id == "orphan-1"));
    assert!(drive.copy_file("folder-1", None, None).is_none());
    let copied = drive
        .copy_file("orphan-1", Some("Copy.txt"), Some(vec!["real-root".into()]))
        .unwrap();
    let patched = drive
        .patch_parents(
            &copied.id,
            Some("folder-1"),
            Some("root"),
            Some("Renamed.txt"),
        )
        .unwrap();
    assert_eq!(patched.name, "Renamed.txt");
    assert!(drive.file_to_json(&patched).get("name").is_some());
    assert!(drive
        .update_metadata("missing", Some("x"), None, None)
        .is_none());
    assert!(!drive.trash("missing"));
}
