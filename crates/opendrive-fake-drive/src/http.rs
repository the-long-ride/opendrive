//! Minimal HTTP/1.1 request parser and response writer for `FakeDriveServer`.

use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub(crate) struct HttpReq {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub(crate) async fn read_request(stream: &mut TcpStream) -> Option<HttpReq> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let req_line = lines.next()?;
    let mut parts = req_line.split_whitespace();
    let method = parts.next()?.to_string();
    let raw_target = parts.next()?;
    let (path, raw_query) = raw_target.split_once('?').unwrap_or((raw_target, ""));
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let content_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buf.len() < body_start + content_len {
        let n = stream.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let end = (body_start + content_len).min(buf.len());
    Some(HttpReq {
        method,
        path: path.to_string(),
        query: parse_query(raw_query),
        headers,
        body: buf[body_start..end].to_vec(),
    })
}

pub(crate) async fn write_resp(
    stream: &mut TcpStream,
    status: u16,
    ctype: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        308 => "Resume Incomplete",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await
}

fn parse_query(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in raw.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(pct_decode(k), pct_decode(v));
    }
    map
}

fn pct_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
            out.push(b[i]);
            i += 1;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn extract_parent_from_q(q: &str) -> Option<&str> {
    let start = q.find('\'')? + 1;
    let rest = &q[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

pub(crate) fn parse_range(v: &str) -> Option<(u64, u64)> {
    let spec = v.strip_prefix("bytes=")?;
    let (s, e) = spec.split_once('-')?;
    Some((s.parse().ok()?, e.parse().ok()?))
}

pub(crate) fn parse_content_range(v: &str) -> (u64, Option<u64>) {
    let rest = v.strip_prefix("bytes ").unwrap_or(v);
    let (range, total) = rest.split_once('/').unwrap_or((rest, "*"));
    let total_num = total.parse::<u64>().ok();
    let offset = range
        .split_once('-')
        .and_then(|(s, _)| s.parse::<u64>().ok())
        .unwrap_or(0);
    (offset, total_num)
}
