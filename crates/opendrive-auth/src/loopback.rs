//! Ephemeral loopback HTTP listener for OAuth redirect.

use std::net::SocketAddr;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

#[derive(Debug, Error)]
pub enum LoopbackError {
    #[error("bind: {0}")]
    Bind(String),
    #[error("timeout")]
    Timeout,
    #[error("bad request")]
    BadRequest,
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug)]
pub struct RedirectParams {
    pub code: String,
    pub state: String,
}

/// Bind 127.0.0.1:0, return (listener, redirect_uri including port).
pub async fn bind_loopback() -> Result<(TcpListener, String, SocketAddr), LoopbackError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| LoopbackError::Bind(e.to_string()))?;
    let addr = listener
        .local_addr()
        .map_err(|e| LoopbackError::Bind(e.to_string()))?;
    let redirect_uri = format!("http://127.0.0.1:{}/callback", addr.port());
    Ok((listener, redirect_uri, addr))
}

/// Wait for one GET with ?code=&state=; respond 200 HTML; return params.
pub async fn wait_for_redirect(
    listener: TcpListener,
    expected_state: &str,
    timeout_secs: u64,
) -> Result<RedirectParams, LoopbackError> {
    let accept = timeout(Duration::from_secs(timeout_secs), listener.accept());
    let (mut socket, _) = accept
        .await
        .map_err(|_| LoopbackError::Timeout)?
        .map_err(|e| LoopbackError::Io(e.to_string()))?;

    let mut buf = vec![0u8; 4096];
    let n = socket
        .read(&mut buf)
        .await
        .map_err(|e| LoopbackError::Io(e.to_string()))?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or("");

    // GET /callback?code=...&state=... HTTP/1.1
    let path = line.split_whitespace().nth(1).unwrap_or("");
    let q = path.split('?').nth(1).unwrap_or("");
    let mut code = None;
    let mut state = None;
    for pair in q.split('&') {
        let mut kv = pair.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("code"), Some(v)) => code = Some(urlencoding_decode(v)),
            (Some("state"), Some(v)) => state = Some(urlencoding_decode(v)),
            _ => {}
        }
    }

    let body = if code.is_some() {
        "<html><body><h1>OpenDrive</h1><p>You can close this window.</p></body></html>"
    } else {
        "<html><body><h1>Error</h1><p>Missing code.</p></body></html>"
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = socket.write_all(resp.as_bytes()).await;

    let code = code.ok_or(LoopbackError::BadRequest)?;
    let state = state.ok_or(LoopbackError::BadRequest)?;
    if state != expected_state {
        return Err(LoopbackError::BadRequest);
    }
    Ok(RedirectParams { code, state })
}

fn urlencoding_decode(s: &str) -> String {
    // minimal: replace %XX and +
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}
