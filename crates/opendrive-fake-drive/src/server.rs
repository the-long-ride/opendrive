//! In-process HTTP/1.1 Drive v3 server backed by `FakeDrive`.

use crate::http::{
    extract_parent_from_q, parse_content_range, parse_range, read_request, write_resp, HttpReq,
};
use crate::FakeDrive;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

pub struct FakeDriveServer;

impl FakeDriveServer {
    pub async fn start(drive: Arc<FakeDrive>) -> (SocketAddr, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake drive");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let d = Arc::clone(&drive);
                tokio::spawn(async move {
                    let _ = handle_conn(stream, d, addr.port()).await;
                });
            }
        });
        (addr, handle)
    }
}

async fn handle_conn(
    mut stream: TcpStream,
    drive: Arc<FakeDrive>,
    port: u16,
) -> std::io::Result<()> {
    let Some(req) = read_request(&mut stream).await else {
        return Ok(());
    };
    if let Some(status) = drive.take_error() {
        return write_resp(&mut stream, status, "application/json", &[], b"{}").await;
    }
    let (status, ctype, extra_headers, body) = dispatch(&req, &drive, port);
    write_resp(&mut stream, status, ctype, &extra_headers, &body).await
}

fn dispatch(
    req: &HttpReq,
    drive: &FakeDrive,
    port: u16,
) -> (u16, &'static str, Vec<(String, String)>, Vec<u8>) {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/drive/v3/about") => {
            let a = drive.about();
            let mut user = json!({ "emailAddress": a.email, "displayName": a.display_name });
            if let Some(url) = drive.photo_url() {
                user["photoLink"] = Value::String(url);
            }
            json_ok(json!({
                "user": user,
                "storageQuota": {
                    "limit": a.storage_quota.to_string(),
                    "usage": a.storage_used.to_string(),
                }
            }))
        }
        ("GET", "/drive/v3/drives") => {
            let drives: Vec<Value> = drive
                .list_shared_drives()
                .into_iter()
                .map(|d| json!({ "id": d.id, "name": d.name }))
                .collect();
            json_ok(json!({ "drives": drives, "nextPageToken": Value::Null }))
        }
        ("GET", "/drive/v3/files") => {
            let q = req.query.get("q").map(|s| s.as_str()).unwrap_or("");
            let items = if q.contains("sharedWithMe = true") {
                drive.list_shared_with_me()
            } else {
                let parent = extract_parent_from_q(q).unwrap_or("root");
                drive.list_children(parent)
            };
            let files: Vec<Value> = items.iter().map(|f| drive.file_to_json(f)).collect();
            json_ok(json!({ "files": files, "nextPageToken": Value::Null }))
        }
        ("GET", p) if p.starts_with("/assets/") => match drive.get_static_asset(p) {
            Some(bytes) => (200, "application/octet-stream", vec![], bytes),
            None => (404, "application/json", vec![], b"{}".to_vec()),
        },
        ("GET", "/drive/v3/changes/startPageToken") => {
            let tok = drive.current_change_token().to_string();
            json_ok(json!({ "startPageToken": tok }))
        }
        ("GET", "/drive/v3/changes") => {
            let tok = req
                .query
                .get("pageToken")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            let (changes, new_tok) = drive.changes_since(tok);
            let items: Vec<Value> = changes
                .into_iter()
                .map(|c| {
                    json!({
                        "fileId": c.file_id,
                        "removed": c.removed,
                        "file": c.file.map(|f| f.to_json()),
                    })
                })
                .collect();
            json_ok(json!({
                "changes": items,
                "newStartPageToken": new_tok.to_string(),
            }))
        }
        ("POST", "/upload/drive/v3/files") => {
            let v: Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let name = v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("untitled")
                .to_string();
            let mime = v
                .get("mimeType")
                .and_then(|x| x.as_str())
                .unwrap_or("application/octet-stream")
                .to_string();
            let parents = v
                .get("parents")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let file_id = v.get("id").and_then(|x| x.as_str()).map(str::to_string);
            let sid = drive.create_upload_session(name, mime, parents, file_id);
            let loc = format!("http://127.0.0.1:{port}/upload/session/{sid}");
            (
                200,
                "application/json",
                vec![("Location".into(), loc)],
                b"{}".to_vec(),
            )
        }
        ("GET", p) if p.starts_with("/drive/v3/files/") => {
            let id = &p["/drive/v3/files/".len()..];
            let Some(f) = drive.get_file(id) else {
                return (404, "application/json", vec![], b"{}".to_vec());
            };
            if req.query.get("alt").map(String::as_str) == Some("media") {
                if let Some((start, end)) = req.headers.get("range").and_then(|r| parse_range(r)) {
                    let s = (start as usize).min(f.content.len());
                    let e = (end as usize + 1).min(f.content.len());
                    return (
                        206,
                        "application/octet-stream",
                        vec![],
                        f.content[s..e].to_vec(),
                    );
                }
                return (200, "application/octet-stream", vec![], f.content);
            }
            json_ok(f.to_json())
        }
        ("PATCH", p) if p.starts_with("/drive/v3/files/") => {
            let id = &p["/drive/v3/files/".len()..];
            let v: Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let name = v.get("name").and_then(|x| x.as_str());
            let trashed = v.get("trashed").and_then(|x| x.as_bool());
            let parents = v.get("parents").and_then(|x| x.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect()
            });
            if trashed == Some(true) && name.is_none() && parents.is_none() {
                if drive.trash(id) {
                    if let Some(f) = drive.get_file(id) {
                        return json_ok(f.to_json());
                    }
                }
                return (404, "application/json", vec![], b"{}".to_vec());
            }
            match drive.update_metadata(id, name, trashed, parents) {
                Some(f) => json_ok(f.to_json()),
                None => (404, "application/json", vec![], b"{}".to_vec()),
            }
        }
        ("PUT", p) if p.starts_with("/upload/session/") => {
            let sid = &p["/upload/session/".len()..];
            let (offset, total) = req
                .headers
                .get("content-range")
                .map(|v| parse_content_range(v))
                .unwrap_or((0, None));
            match drive.put_upload_chunk(sid, offset, total, &req.body) {
                Ok(None) => (308, "text/plain", vec![], Vec::new()),
                Ok(Some(f)) => json_ok(f.to_json()),
                Err(_) => (404, "application/json", vec![], b"{}".to_vec()),
            }
        }
        _ => (404, "application/json", vec![], b"{}".to_vec()),
    }
}

fn json_ok(v: Value) -> (u16, &'static str, Vec<(String, String)>, Vec<u8>) {
    (
        200,
        "application/json",
        vec![],
        serde_json::to_vec(&v).unwrap(),
    )
}
