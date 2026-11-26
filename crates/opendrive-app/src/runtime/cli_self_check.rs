//! Self-contained `--self-check` exercising SyncEngine, StreamCache, MediaStreamServer, ExplorerQuota, OpenWithRule, and Bridge.

use crate::runtime::bridge::Bridge;
use opendrive_core::{
    resolve_open_app, supports_http_stream_arg, AppCommand, AppController, AppEvent, OpenWithRule,
    Settings,
};
use opendrive_drive::DriveClient;
use opendrive_platform::compute_explorer_quota;
use opendrive_store::Store;
use opendrive_sync::{SyncEngine, VfsEngine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

pub(super) async fn run_self_check() -> anyhow::Result<()> {
    let tmp = std::env::temp_dir().join(format!("opendrive-selfcheck-{}", Uuid::new_v4()));
    let stream_dir = tmp.join("stream_cache");
    let up_file = tmp.join("note.txt");
    std::fs::create_dir_all(&stream_dir)?;
    std::fs::write(&up_file, b"hello")?;

    let video_bytes: Vec<u8> = (0..(256 * 1024usize)).map(|i| (i % 251) as u8).collect();
    let (addr, server_h) = start_mock_drive(Arc::new(video_bytes.clone())).await?;

    let store = Store::open_in_memory().map_err(|e| anyhow::anyhow!(e))?;
    let engine = Arc::new(
        SyncEngine::new(store)
            .with_stream_cache_dir(&stream_dir)
            .map_err(|e| anyhow::anyhow!(e))?,
    );
    let mut ctrl = AppController::new(Settings::default());
    let ev = ctrl.handle(AppCommand::AddAccount {
        nickname: "Work".into(),
    });
    assert!(matches!(ev[0], AppEvent::AccountUpdated(_)));

    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&engine), None);
    let bridge = bridge.with_data_dir(tmp.clone());
    let snap = bridge
        .add_account(
            "Work".into(),
            "work@example.com".into(),
            "work".into(),
            false,
        )
        .map_err(anyhow::Error::msg)?;
    assert_eq!(snap.accounts.len(), 1);
    let acct_id = snap.accounts[0].id;
    engine.set_drive_client(
        acct_id,
        DriveClient::with_base_urls(
            "self-check-tok".into(),
            format!("http://{addr}/drive/v3"),
            format!("http://{addr}/upload/drive/v3"),
        ),
    );

    let (_root, entries) = bridge
        .explorer_list(acct_id, None, true)
        .map_err(anyhow::Error::msg)?;
    let vid = entries
        .iter()
        .find(|e| e.name == "demo.mp4")
        .ok_or_else(|| anyhow::anyhow!("missing demo.mp4 in explorer_list"))?;
    let url = bridge
        .explorer_stream_url(acct_id, vid.id)
        .map_err(anyhow::Error::msg)?;
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Range", "bytes=0-65535")
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 206);
    let body = resp.bytes().await?;
    assert_eq!(body.as_ref(), &video_bytes[..65536]);

    bridge
        .update_open_with_rules(vec![OpenWithRule {
            pattern: "@video".into(),
            app_command: "vlc".into(),
        }])
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        resolve_open_app(
            &bridge.state_snapshot().settings.open_with_rules,
            "demo.mp4",
            None
        ),
        Some("vlc")
    );
    assert!(supports_http_stream_arg(Some("vlc")));
    let opened = bridge
        .explorer_open_item(acct_id, vid.id, true)
        .map_err(anyhow::Error::msg)?;
    assert!(opened.is_stream_url);
    assert_eq!(opened.app_command.as_deref(), Some("vlc"));

    let (down_sum, _) = bridge
        .explorer_sync_down(acct_id, vid.id)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(down_sum.downloaded_files, 1);
    let clr_sum = bridge
        .explorer_sync_up_and_clear(acct_id, vid.id)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(clr_sum.cleared_files, 1);
    let root_id = bridge
        .explorer_root_node(acct_id)
        .map_err(anyhow::Error::msg)?;
    let up_sum = bridge
        .explorer_sync_up(acct_id, root_id, &[up_file])
        .map_err(anyhow::Error::msg)?;
    assert_eq!(up_sum.uploaded_files, 1);
    let root_items = bridge
        .explorer_list(acct_id, Some(root_id), false)
        .map_err(anyhow::Error::msg)?
        .1;
    let note_id = root_items
        .iter()
        .find(|e| e.name == "note.txt")
        .map(|e| e.id)
        .ok_or_else(|| anyhow::anyhow!("missing note.txt"))?;
    let arch_id = root_items
        .iter()
        .find(|e| e.name == "Archive")
        .map(|e| e.id)
        .ok_or_else(|| anyhow::anyhow!("missing Archive"))?;
    let renamed = bridge
        .explorer_rename_item(acct_id, note_id, "note-ren.txt")
        .map_err(anyhow::Error::msg)?;
    assert_eq!(renamed.name, "note-ren.txt");
    let copied = bridge
        .explorer_copy_item(acct_id, note_id, root_id)
        .map_err(anyhow::Error::msg)?;
    bridge
        .explorer_move_item(acct_id, copied.id, arch_id)
        .map_err(anyhow::Error::msg)?;
    bridge
        .explorer_delete_item(acct_id, copied.id)
        .map_err(anyhow::Error::msg)?;
    assert!(compute_explorer_quota(&[(4 * 1024, 16 * 1024)]).is_some());
    assert!(compute_explorer_quota(&[(100, 0)]).is_none());

    let page = engine.list(0, None).await.map_err(|e| anyhow::anyhow!(e))?;
    let state = bridge.state_snapshot();
    server_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
    println!(
        "self-check ok: entries={} accounts={} platform={} stream=206",
        page.entries.len(),
        state.accounts.len(),
        state.platform
    );
    Ok(())
}

async fn start_mock_drive(
    video: Arc<Vec<u8>>,
) -> anyhow::Result<(std::net::SocketAddr, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let has_note = Arc::new(AtomicBool::new(false));
    let handle = tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let (data, has_note) = (Arc::clone(&video), Arc::clone(&has_note));
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let first = req.lines().next().unwrap_or("");
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap_or("");
                let target = parts.next().unwrap_or("");

                if method == "GET"
                    && target.starts_with("/drive/v3/files/vid-1")
                    && target.contains("alt=media")
                {
                    let (s, e) = parse_req_range(&req, data.len());
                    let slice = &data[s..=e];
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {s}-{e}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        data.len(),
                        slice.len()
                    );
                    let _ = sock.write_all(hdr.as_bytes()).await;
                    let _ = sock.write_all(slice).await;
                } else if method == "GET" && target.starts_with("/drive/v3/files/root") {
                    let _ = write_json(&mut sock, r#"{"id":"root"}"#).await;
                } else if method == "GET"
                    && target.starts_with("/drive/v3/files")
                    && (target.contains("not+") || target.contains("not%20"))
                {
                    let _ = write_json(&mut sock, r#"{"files":[]}"#).await;
                } else if method == "GET" && target.starts_with("/drive/v3/files") {
                    let note_json = if has_note.load(Ordering::Relaxed) {
                        r#",{"id":"up-1","name":"note.txt","mimeType":"text/plain","size":"5","modifiedTime":"2026-01-01T00:00:00Z","headRevisionId":"rev-u1","trashed":false}"#
                    } else {
                        ""
                    };
                    let body = format!(
                        r#"{{"files":[{{"id":"vid-1","name":"demo.mp4","mimeType":"video/mp4","size":"{}","modifiedTime":"2026-01-01T00:00:00Z","headRevisionId":"rev-1","trashed":false}},{{"id":"dir-1","name":"Archive","mimeType":"application/vnd.google-apps.folder","trashed":false}}{note_json}]}}"#,
                        data.len()
                    );
                    let _ = write_json(&mut sock, &body).await;
                } else if method == "POST" && target.contains("/copy") {
                    let _ = write_json(&mut sock, r#"{"id":"cp-1","name":"note-ren (copy).txt","mimeType":"text/plain","size":"5","headRevisionId":"rev-c1","trashed":false}"#).await;
                } else if method == "PATCH" && target.starts_with("/drive/v3/files/") {
                    let _ = write_json(&mut sock, r#"{"id":"up-1","name":"note-ren.txt","mimeType":"text/plain","size":"5","headRevisionId":"rev-m1","trashed":false}"#).await;
                } else if method == "POST" && target.starts_with("/upload/drive/v3/files") {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nLocation: http://{addr}/upload/session/1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    let _ = sock.write_all(hdr.as_bytes()).await;
                } else if method == "PUT" && target.starts_with("/upload/session/1") {
                    has_note.store(true, Ordering::Relaxed);
                    let body = r#"{"id":"up-1","name":"note.txt","mimeType":"text/plain","size":"5","modifiedTime":"2026-01-01T00:00:00Z","headRevisionId":"rev-u1","trashed":false}"#;
                    let _ = write_json(&mut sock, body).await;
                } else {
                    let _ = write_json(&mut sock, "{}").await;
                }
            });
        }
    });
    Ok((addr, handle))
}

fn parse_req_range(req: &str, total: usize) -> (usize, usize) {
    for line in req.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("range") {
                if let Some((a, b)) = v
                    .trim()
                    .strip_prefix("bytes=")
                    .and_then(|r| r.split_once('-'))
                {
                    let s = a
                        .trim()
                        .parse::<usize>()
                        .unwrap_or(0)
                        .min(total.saturating_sub(1));
                    let e = b
                        .trim()
                        .parse::<usize>()
                        .unwrap_or(total - 1)
                        .min(total - 1);
                    return (s, e.max(s));
                }
            }
        }
    }
    (0, total.saturating_sub(1))
}

async fn write_json(sock: &mut tokio::net::TcpStream, body: &str) -> std::io::Result<()> {
    let hdr = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(hdr.as_bytes()).await
}
