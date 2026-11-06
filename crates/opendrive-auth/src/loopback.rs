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

    let body = render_callback_html(code.is_some());
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

fn render_callback_html(ok: bool) -> String {
    let (title, pill, heading, detail, tone_var) = if ok {
        (
            "OpenDrive — Connected",
            "Authorization Complete",
            "Google Drive connected",
            "You can close this window. Return to OpenDrive Control Center.",
            "--ok",
        )
    } else {
        (
            "OpenDrive — Authorization Error",
            "Authorization Failed",
            "Sign-in could not complete",
            "Missing code. Return to OpenDrive and try connecting your account again.",
            "--err",
        )
    };
    let badge_svg = if ok {
        r#"<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10.5L8.5 15L16.5 5.5"/></svg>"#
    } else {
        r#"<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M10 3.5L17 16H3L10 3.5Z"/><path d="M10 8.5V12"/><circle cx="10" cy="14" r=".8" fill="currentColor"/></svg>"#
    };
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title><style>
:root{{--bg:#131519;--card:#1b1e24;--border:#2e333e;--ink:#090a0d;--text:#f5f7fa;--muted:#949ba7;--blue:#2e7cf6;--ok:#34d399;--err:#ff5238;--tint:rgba(52,211,153,.14)}}
@media(prefers-color-scheme:light){{:root{{--bg:#f2f4f8;--card:#ffffff;--border:#dce1eb;--ink:#14171f;--text:#14171f;--muted:#555e6e;--blue:#2563eb}}}}
*{{box-sizing:border-box}}body{{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;background:var(--bg);color:var(--text);font:400 15px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif}}
.card{{width:100%;max-width:440px;background:var(--card);border:1.5px solid var(--border);border-radius:16px;padding:28px;box-shadow:0 6px 0 var(--ink)}}
.brand{{display:flex;align-items:center;gap:10px;font-weight:700;font-size:17px;margin-bottom:20px}}
.brand svg{{color:var(--blue)}}
.pill{{display:inline-flex;align-items:center;gap:8px;padding:5px 12px;border-radius:999px;font-size:12px;font-weight:600;border:1px solid var({tone_var});color:var({tone_var});background:color-mix(in srgb,var({tone_var}) 14%,transparent);margin-bottom:14px}}
.icon-box{{width:44px;height:44px;border-radius:12px;display:grid;place-items:center;color:var({tone_var});background:color-mix(in srgb,var({tone_var}) 14%,transparent);border:1px solid var({tone_var});margin-bottom:14px}}
h1{{margin:0 0 8px;font-size:21px;line-height:1.25;letter-spacing:-.01em}}
p{{margin:0;color:var(--muted);font-size:14px}}
</style></head><body><main class="card"><div class="brand"><svg width="22" height="22" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><polygon points="10,2.5 17,6.5 17,13.5 10,17.5 3,13.5 3,6.5"/><path d="M10 10V17.5M10 10L3 6.5M10 10L17 6.5"/></svg><span>OpenDrive</span></div><div class="icon-box">{badge_svg}</div><div class="pill">{pill}</div><h1>{heading}</h1><p>{detail}</p></main></body></html>"#
    )
}

fn urlencoding_decode(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    async fn send_raw_get(addr: SocketAddr, target: &str) -> String {
        let mut s = TcpStream::connect(addr).await.unwrap();
        let req = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        let _ = s.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    }

    #[tokio::test]
    async fn redirect_success_decodes_params() {
        let (listener, uri, addr) = bind_loopback().await.unwrap();
        assert!(uri.starts_with("http://127.0.0.1:"));
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "st 1", 5).await });
        let resp = send_raw_get(addr, "/callback?extra=1&code=abc%2F123&state=st+1").await;
        assert!(resp.contains("You can close this window."));
        let params = waiter.await.unwrap().unwrap();
        assert_eq!(params.code, "abc/123");
        assert_eq!(params.state, "st 1");
    }

    #[tokio::test]
    async fn redirect_rejects_state_mismatch_missing_code_and_timeout() {
        let (listener, _, addr) = bind_loopback().await.unwrap();
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "good", 5).await });
        let _ = send_raw_get(addr, "/callback?code=x&state=wrong").await;
        assert!(matches!(
            waiter.await.unwrap(),
            Err(LoopbackError::BadRequest)
        ));

        let (listener, _, addr) = bind_loopback().await.unwrap();
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "good", 5).await });
        let resp = send_raw_get(addr, "/callback?state=good").await;
        assert!(resp.contains("Missing code."));
        assert!(matches!(
            waiter.await.unwrap(),
            Err(LoopbackError::BadRequest)
        ));

        let (listener, _, _) = bind_loopback().await.unwrap();
        assert!(matches!(
            wait_for_redirect(listener, "good", 0).await,
            Err(LoopbackError::Timeout)
        ));
    }

    #[test]
    fn urlencoding_decode_handles_percent_and_invalid() {
        assert_eq!(urlencoding_decode("a+b%20c%2Fd%ZZ"), "a b c/d%ZZ");
    }
}
