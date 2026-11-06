//! Unit tests for `FakeDrive` state transitions and resumable uploads.

use opendrive_fake_drive::{FakeAbout, FakeDrive, FakeDriveServer, FakeFile};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[test]
fn fake_drive_crud_changes_and_errors() {
    let drive = FakeDrive::new();
    assert!(drive.get_file("root").is_some());
    assert_eq!(drive.about().email, "user@example.com");

    drive.set_about(FakeAbout {
        email: "bob@example.com".into(),
        display_name: "Bob".into(),
        storage_quota: 100,
        storage_used: 10,
    });
    assert_eq!(drive.about().display_name, "Bob");

    drive.add_shared_drive("d1", "Team");
    assert_eq!(drive.list_shared_drives().len(), 1);

    let t0 = drive.current_change_token();
    drive.add_file(FakeFile {
        id: "f1".into(),
        name: "a.txt".into(),
        mime_type: "text/plain".into(),
        size: 3,
        parents: vec!["root".into()],
        trashed: false,
        content: b"abc".to_vec(),
        head_revision_id: None,
    });
    assert_eq!(drive.list_children("root").len(), 1);
    let updated = drive
        .update_metadata("f1", Some("b.txt"), None, None)
        .unwrap();
    assert_eq!(updated.name, "b.txt");
    assert_eq!(updated.head_revision_id.as_deref(), Some("rev-2"));

    assert!(drive.trash("f1"));
    assert!(drive.list_children("root").is_empty());

    let (changes, t1) = drive.changes_since(t0);
    assert_eq!(changes.len(), 3);
    assert!(t1 > t0);

    drive.push_error(429);
    assert_eq!(drive.take_error(), Some(429));
    assert_eq!(drive.take_error(), None);
}

#[test]
fn fake_drive_resumable_upload_session() {
    let drive = FakeDrive::new();
    let sid = drive.create_upload_session(
        "chunked.bin".into(),
        "application/octet-stream".into(),
        vec!["root".into()],
        None,
    );
    assert!(drive
        .put_upload_chunk(&sid, 0, Some(6), b"abc")
        .unwrap()
        .is_none());
    let done = drive
        .put_upload_chunk(&sid, 3, Some(6), b"def")
        .unwrap()
        .unwrap();
    assert_eq!(done.content, b"abcdef");
    assert_eq!(done.size, 6);
    assert!(drive.put_upload_chunk(&sid, 0, Some(1), b"x").is_err());
}

async fn raw_http(port: u16, method: &str, path: &str, body: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let req = if body.is_empty() {
        format!("{method} {path} HTTP/1.1\r\nHost: x\r\n\r\n")
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    };
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf).await;
    String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test]
async fn fake_drive_server_edge_routes_and_malformed_requests() {
    let drive = Arc::new(FakeDrive::new());
    drive.add_file(FakeFile {
        id: "m1".into(),
        name: "m.txt".into(),
        mime_type: "text/plain".into(),
        size: 5,
        parents: vec!["root".into()],
        trashed: false,
        content: b"hello".to_vec(),
        head_revision_id: None,
    });
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let p = addr.port();

    // Premature EOF socket
    let s = TcpStream::connect(addr).await.unwrap();
    drop(s);

    // Full media without Range + metadata GET
    let full = raw_http(p, "GET", "/drive/v3/files/m1?alt=media", "").await;
    assert!(full.ends_with("hello"));
    let meta = raw_http(p, "GET", "/drive/v3/files/m1", "").await;
    assert!(meta.contains("200 OK"));

    // Invalid percent encoding in query + unknown route + missing PATCH/PUT
    let bad_pct = raw_http(p, "GET", "/drive/v3/files?q=%ZZ", "").await;
    assert!(bad_pct.contains("200 OK"));
    let unk = raw_http(p, "GET", "/unknown", "").await;
    assert!(unk.contains("404 Not Found"));
    let patch_trash = raw_http(p, "PATCH", "/drive/v3/files/missing", "{\"trashed\":true}").await;
    assert!(patch_trash.contains("404 Not Found"));
    let patch_name = raw_http(p, "PATCH", "/drive/v3/files/missing", "{\"name\":\"x\"}").await;
    assert!(patch_name.contains("404 Not Found"));
    let put_miss = raw_http(p, "PUT", "/upload/session/missing", "a").await;
    assert!(put_miss.contains("404 Not Found"));

    drive.push_error(418);
    let custom = raw_http(p, "GET", "/drive/v3/about", "").await;
    assert!(custom.contains("418 Status"));
    handle.abort();
}
