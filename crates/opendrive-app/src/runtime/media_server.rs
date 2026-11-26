//! Localhost HTTP/1.1 range-streaming server backed by `SyncEngine::read_cached_range`.

use crate::runtime::explorer_helpers::{write_http_head, write_http_simple};
use opendrive_sync::{NodeId, SyncEngine};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use uuid::Uuid;

const STREAM_PIECE_BYTES: usize = 512 * 1024;
const READ_AHEAD_CHUNKS: usize = 3;

pub struct MediaStreamServer {
    port: u16,
    token: String,
    task: JoinHandle<()>,
}

impl Drop for MediaStreamServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MediaStreamServer {
    pub fn start(engine: Arc<SyncEngine>, rt: &tokio::runtime::Handle) -> Result<Self, String> {
        let std_l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        std_l.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = std_l.local_addr().map_err(|e| e.to_string())?.port();
        let _guard = rt.enter();
        let listener = TcpListener::from_std(std_l).map_err(|e| e.to_string())?;
        let token = Uuid::new_v4().simple().to_string();
        let tok_clone = token.clone();
        let task = rt.spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (eng, tok) = (Arc::clone(&engine), tok_clone.clone());
                tokio::spawn(async move {
                    let _ = handle_connection(stream, eng, &tok).await;
                });
            }
        });
        Ok(Self { port, token, task })
    }

    pub fn stream_url(&self, node_id: NodeId, filename: &str) -> String {
        let enc = pct_encode(filename);
        format!(
            "http://127.0.0.1:{}/stream/{}/{node_id}/{enc}",
            self.port, self.token
        )
    }
}

fn pct_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

async fn handle_connection(
    stream: TcpStream,
    engine: Arc<SyncEngine>,
    expected_token: &str,
) -> std::io::Result<()> {
    let (read_half, mut w) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut req_line = String::new();
    if reader.read_line(&mut req_line).await? == 0 {
        return Ok(());
    }
    let mut parts = req_line.split_whitespace();
    let (method, raw_target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut range_hdr = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((_, v)) = line
            .split_once(':')
            .filter(|(k, _)| k.trim().eq_ignore_ascii_case("range"))
        {
            range_hdr = Some(v.trim().to_string());
        }
    }
    let is_head = match method {
        "GET" => false,
        "HEAD" => true,
        _ => return write_http_simple(&mut w, "403 Forbidden", None).await,
    };
    let mut segs = raw_target
        .split('?')
        .next()
        .unwrap_or("")
        .strip_prefix("/stream/")
        .unwrap_or("")
        .splitn(3, '/');
    if !raw_target.starts_with("/stream/") || segs.next().unwrap_or("") != expected_token {
        return write_http_simple(&mut w, "403 Forbidden", None).await;
    }
    let Some(node) = segs
        .next()
        .unwrap_or("")
        .parse::<NodeId>()
        .ok()
        .and_then(|id| engine.get_node(id).ok().flatten())
        .filter(|n| !n.trashed && !n.is_folder())
    else {
        return write_http_simple(&mut w, "404 Not Found", None).await;
    };
    let (node_id, total) = (node.id as NodeId, node.size.max(0) as u64);
    let ct = content_type_for(node.effective_name(), node.mime.as_deref());
    let (status, start, end, cr) = match range_hdr.as_deref() {
        Some(raw) => match parse_http_range(raw, total) {
            Some((s, e)) => (
                "206 Partial Content",
                s,
                e,
                Some(format!("bytes {s}-{e}/{total}")),
            ),
            None => {
                return write_http_simple(
                    &mut w,
                    "416 Range Not Satisfiable",
                    Some(&format!("bytes */{total}")),
                )
                .await
            }
        },
        None => ("200 OK", 0, total.saturating_sub(1), None),
    };
    let slice_len = if total == 0 { 0 } else { end - start + 1 };
    write_http_head(&mut w, status, Some(ct), slice_len, cr.as_deref()).await?;
    if is_head || slice_len == 0 {
        return w.flush().await;
    }
    let mut offset = start;
    while offset <= end {
        let want = ((end - offset + 1).min(STREAM_PIECE_BYTES as u64)) as u32;
        let Ok(chunk) = engine
            .read_cached_range(node_id, offset, want, READ_AHEAD_CHUNKS)
            .await
        else {
            break;
        };
        if chunk.is_empty() || w.write_all(&chunk).await.is_err() {
            break;
        }
        offset += chunk.len() as u64;
    }
    let _ = w.flush().await;
    Ok(())
}

pub fn parse_http_range(header_val: &str, total: u64) -> Option<(u64, u64)> {
    let raw = header_val.trim().strip_prefix("bytes=")?.trim();
    if total == 0 || raw.is_empty() || raw.contains(',') {
        return None;
    }
    let (start_s, end_s) = raw.split_once('-')?;
    let (start_s, end_s) = (start_s.trim(), end_s.trim());
    if start_s.is_empty() {
        let suffix: u64 = end_s.parse().ok()?;
        return (suffix > 0).then(|| (total - suffix.min(total), total - 1));
    }
    let start: u64 = start_s.parse().ok()?;
    if start >= total {
        return None;
    }
    if end_s.is_empty() {
        return Some((start, total - 1));
    }
    let end: u64 = end_s.parse().ok()?;
    (end >= start).then(|| (start, end.min(total - 1)))
}

pub fn content_type_for(filename: &str, mime: Option<&str>) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .filter(|_| filename.contains('.'))
        .map(str::to_ascii_lowercase);
    let by_ext = match ext.as_deref() {
        Some("mp4" | "m4v") => Some("video/mp4"),
        Some("mkv") => Some("video/x-matroska"),
        Some("webm") => Some("video/webm"),
        Some("mov") => Some("video/quicktime"),
        Some("pdf") => Some("application/pdf"),
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("svg") => Some("image/svg+xml"),
        Some("mp3") => Some("audio/mpeg"),
        Some("flac") => Some("audio/flac"),
        Some("wav") => Some("audio/wav"),
        Some("ogg" | "opus") => Some("audio/ogg"),
        _ => None,
    };
    if let Some(ct) = by_ext {
        return ct;
    }
    let m = mime.map(str::trim).unwrap_or("");
    [
        "video/mp4",
        "video/x-matroska",
        "video/webm",
        "video/quicktime",
        "application/pdf",
        "image/png",
        "image/jpeg",
        "image/webp",
        "image/gif",
    ]
    .into_iter()
    .find(|k| m.eq_ignore_ascii_case(k))
    .unwrap_or("application/octet-stream")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ranges_and_content_types() {
        assert_eq!(parse_http_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_http_range("bytes=50-", 100), Some((50, 99)));
        assert_eq!(parse_http_range("bytes=-16", 100), Some((84, 99)));
        assert_eq!(parse_http_range("bytes=-500", 100), Some((0, 99)));
        assert_eq!(parse_http_range("bytes=90-500", 100), Some((90, 99)));
        assert_eq!(parse_http_range("bytes=100-105", 100), None);
        assert_eq!(parse_http_range("bytes=10-5", 100), None);
        assert_eq!(parse_http_range("bytes=-0", 100), None);
        assert_eq!(parse_http_range("bytes=0-1", 0), None);
        for (file, ct) in [
            ("clip.mp4", "video/mp4"),
            ("movie.MKV", "video/x-matroska"),
            ("c.webm", "video/webm"),
            ("c.mov", "video/quicktime"),
            ("spec.pdf", "application/pdf"),
            ("pic.png", "image/png"),
            ("p.jpg", "image/jpeg"),
            ("p.gif", "image/gif"),
            ("p.webp", "image/webp"),
            ("p.svg", "image/svg+xml"),
            ("a.mp3", "audio/mpeg"),
            ("a.flac", "audio/flac"),
            ("a.wav", "audio/wav"),
            ("a.ogg", "audio/ogg"),
            ("b.bin", "application/octet-stream"),
        ] {
            assert_eq!(content_type_for(file, None), ct);
        }
        assert_eq!(content_type_for("raw", Some("video/webm")), "video/webm");
        assert_eq!(pct_encode("My Video (1).mp4"), "My%20Video%20%281%29.mp4");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn handles_post_folder_empty_and_full_get() {
        let eng = std::sync::Arc::new(SyncEngine::new(
            opendrive_store::Store::open_in_memory().unwrap(),
        ));
        let root = eng.seed_root_account(Uuid::new_v4(), "w").unwrap();
        let mut n = eng.store().lock().get_node(root as i64).unwrap().unwrap();
        (n.id, n.parent_id, n.drive_id, n.kind, n.size) =
            (0, Some(root as i64), "e1".into(), "file".into(), 0);
        (n.name, n.display_name, n.mime) =
            ("e.mp4".into(), "e.mp4".into(), Some("video/mp4".into()));
        let empty = eng.store().lock().upsert_node(&n).unwrap() as u64;
        let srv = super::MediaStreamServer::start(eng, &tokio::runtime::Handle::current()).unwrap();
        let c = reqwest::Client::new();
        let r1 = c.post(srv.stream_url(empty, "e.mp4")).send().await.unwrap();
        assert_eq!(r1.status(), 403);
        let r2 = c.get(srv.stream_url(root, "w")).send().await.unwrap();
        assert_eq!(r2.status(), 404);
        let ok = c.get(srv.stream_url(empty, "e.mp4")).send().await.unwrap();
        assert_eq!(ok.status(), 200);
        assert!(ok.bytes().await.unwrap().is_empty());
    }
}
