//! HTTP integration tests for `DriveClient` against `FakeDriveServer`.

use opendrive_drive::{is_native_google_doc, DriveClient, DriveError};
use opendrive_fake_drive::{FakeAbout, FakeDrive, FakeDriveServer, FakeFile};
use serde_json::json;
use std::sync::Arc;

async fn setup() -> (Arc<FakeDrive>, DriveClient, tokio::task::JoinHandle<()>) {
    let drive = Arc::new(FakeDrive::new());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let api = format!("http://{addr}/drive/v3");
    let upload = format!("http://{addr}/upload/drive/v3");
    let client = DriveClient::with_base_urls("tok-123".into(), api, upload);
    (drive, client, handle)
}

#[tokio::test]
async fn about_and_shared_drives() {
    let (drive, client, handle) = setup().await;
    drive.set_about(FakeAbout {
        email: "alice@example.com".into(),
        display_name: "Alice".into(),
        storage_quota: 5000,
        storage_used: 1200,
    });
    drive.add_shared_drive("sd-1", "Engineering");

    let about = client.about().await.unwrap();
    assert_eq!(about.email, "alice@example.com");
    assert_eq!(about.display_name, "Alice");
    assert_eq!(about.storage_quota, 5000);
    assert_eq!(about.storage_used, 1200);

    let drives = client.list_shared_drives(Some("p2")).await.unwrap();
    assert_eq!(drives.drives.len(), 1);
    assert_eq!(drives.drives[0].id, "sd-1");
    assert_eq!(drives.drives[0].name, "Engineering");

    let default_client = DriveClient::new("tok-default".into());
    assert_eq!(default_client.access_token(), "tok-default");
    handle.abort();
}

#[tokio::test]
async fn list_download_update_trash_and_changes() {
    let (drive, client, handle) = setup().await;
    let start_tok = client.get_start_page_token().await.unwrap();
    drive.add_file(FakeFile {
        id: "f1".into(),
        name: "notes.txt".into(),
        mime_type: "text/plain".into(),
        size: 11,
        parents: vec!["root".into()],
        trashed: false,
        content: b"hello world".to_vec(),
        head_revision_id: Some("rev-1".into()),
    });

    let listed = client.list_files("root", Some("p2"), false).await.unwrap();
    assert_eq!(listed.files.len(), 1);
    assert_eq!(listed.files[0].name, "notes.txt");

    let part = client.download_range("f1", 6, 5).await.unwrap();
    assert_eq!(part.as_ref(), b"world");
    assert!(client.download_range("f1", 0, 0).await.unwrap().is_empty());

    let updated = client
        .update_metadata("f1", &json!({ "name": "renamed.txt" }))
        .await
        .unwrap();
    assert_eq!(updated.name, "renamed.txt");
    assert_eq!(updated.head_revision_id.as_deref(), Some("rev-2"));

    let changes = client.changes(&start_tok).await.unwrap();
    assert_eq!(changes.changes.len(), 2);
    let next_tok = changes.new_start_token.unwrap();

    client.trash("f1").await.unwrap();
    assert!(client
        .list_files("root", None, false)
        .await
        .unwrap()
        .files
        .is_empty());

    let after_trash = client.changes(&next_tok).await.unwrap();
    assert_eq!(after_trash.changes.len(), 1);
    assert!(after_trash.changes[0].removed);
    handle.abort();
}

#[tokio::test]
async fn resumable_upload_multi_chunk_and_empty() {
    let (drive, client, handle) = setup().await;

    let uri = client
        .create_resumable(&json!({
            "name": "big.bin",
            "mimeType": "application/octet-stream",
            "parents": ["root"]
        }))
        .await
        .unwrap();
    let first = client.upload_chunk(&uri, 0, 10, b"hello").await.unwrap();
    assert!(first.is_none());
    let second = client.upload_chunk(&uri, 5, 10, b"world").await.unwrap();
    let uploaded = second.expect("final chunk returns file");
    assert_eq!(uploaded.name, "big.bin");
    assert_eq!(uploaded.size, Some(10));
    assert_eq!(drive.get_file(&uploaded.id).unwrap().content, b"helloworld");

    let empty_uri = client
        .create_resumable(&json!({ "name": "empty.txt", "parents": ["root"] }))
        .await
        .unwrap();
    let empty_file = client.upload_chunk(&empty_uri, 0, 0, &[]).await.unwrap();
    assert_eq!(empty_file.unwrap().size, Some(0));
    handle.abort();
}

#[tokio::test]
async fn error_status_mapping_and_backoff_recovery() {
    let (drive, client, handle) = setup().await;

    drive.push_error(429);
    let about = client.about().await.unwrap();
    assert_eq!(about.email, "user@example.com");

    drive.set_forced_error(Some(401));
    assert!(matches!(client.about().await, Err(DriveError::Auth)));

    drive.set_forced_error(Some(404));
    assert!(matches!(
        client.download_range("missing", 0, 4).await,
        Err(DriveError::NotFound)
    ));

    drive.set_forced_error(Some(500));
    assert!(matches!(
        client.get_start_page_token().await,
        Err(DriveError::Http(_))
    ));

    drive.set_forced_error(None);
    drive.push_error(308);
    assert!(client.download_range("f1", 0, 4).await.unwrap().is_empty());

    drive.push_error(500);
    assert!(matches!(
        client
            .create_resumable(&json!({ "name": "fail.txt" }))
            .await,
        Err(DriveError::Http(_))
    ));

    let uri = client
        .create_resumable(&json!({ "name": "ok.txt" }))
        .await
        .unwrap();
    drive.push_error(500);
    assert!(matches!(
        client.upload_chunk(&uri, 0, 1, b"x").await,
        Err(DriveError::Http(_))
    ));

    let dead = DriveClient::with_base_urls("t".into(), "http://127.0.0.1:1", "http://127.0.0.1:1");
    assert!(matches!(dead.about().await, Err(DriveError::Http(_))));
    assert!(matches!(
        dead.download_range("nope", 0, 10).await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(
        dead.create_resumable(&json!({ "name": "x" })).await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(
        dead.upload_chunk("http://127.0.0.1:1/u", 0, 1, b"x").await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(dead.trash("x").await, Err(DriveError::Http(_))));
    assert!(matches!(
        dead.get_start_page_token().await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(dead.changes("1").await, Err(DriveError::Http(_))));
    assert!(matches!(
        dead.update_metadata("x", &json!({ "name": "y" })).await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(
        client
            .update_metadata("f1", &json!({ "parents": ["root"] }))
            .await,
        Err(DriveError::NotFound)
    ));
    assert!(matches!(
        client.trash("missing").await,
        Err(DriveError::NotFound)
    ));
    let bad_session = format!("{uri}-missing");
    assert!(matches!(
        client.upload_chunk(&bad_session, 0, 1, b"x").await,
        Err(DriveError::NotFound)
    ));

    // Tiny raw TCP server to cover missing Location, invalid JSON in upload_chunk, and file-only change
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    let mock_handle = tokio::spawn(async move {
        for body in [
            "{}",
            "not-json",
            r#"{"changes":[{"file":{"id":"c1","name":"n","mimeType":"text/plain"}}]}"#,
        ] {
            if let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = s.write_all(resp.as_bytes()).await;
            }
        }
    });
    let mock = DriveClient::with_base_urls(
        "t".into(),
        format!("http://{mock_addr}"),
        format!("http://{mock_addr}"),
    );
    assert!(matches!(
        mock.create_resumable(&json!({})).await,
        Err(DriveError::Other(_))
    ));
    assert!(matches!(
        mock.upload_chunk(&format!("http://{mock_addr}/u"), 0, 1, b"a")
            .await,
        Err(DriveError::Http(_))
    ));
    let ch = mock.changes("1").await.unwrap();
    assert_eq!(ch.changes[0].file_id, "c1");
    mock_handle.abort();
    drive.push_error(204);
    assert!(matches!(
        client.upload_chunk(&uri, 0, 1, b"x").await,
        Err(DriveError::Http(_))
    ));
    assert!(matches!(
        client.download_range("nope", 0, 10).await,
        Err(DriveError::NotFound)
    ));
    assert_eq!(
        DriveClient::native_doc_link("abc"),
        "https://drive.google.com/open?id=abc"
    );
    assert!(is_native_google_doc("application/vnd.google-apps.document"));
    assert!(!is_native_google_doc("text/plain"));
    handle.abort();
}
