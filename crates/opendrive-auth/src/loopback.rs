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
    timeout(Duration::from_secs(timeout_secs), async {
        loop {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(|e| LoopbackError::Io(e.to_string()))?;
            let mut buf = vec![0u8; 4096];
            let n = socket
                .read(&mut buf)
                .await
                .map_err(|e| LoopbackError::Io(e.to_string()))?;
            if n == 0 {
                continue;
            }
            let req = String::from_utf8_lossy(&buf[..n]);
            let target = req.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("");
            let (route, q) = target.split_once('?').unwrap_or((target, ""));
            if route != "/callback" && !q.contains("code=") && !q.contains("error=") {
                let _ = socket
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
                continue;
            }
            let (mut code, mut state, mut oauth_err) = (None, None, None);
            for pair in q.split('&') {
                let mut kv = pair.splitn(2, '=');
                match (kv.next(), kv.next()) {
                    (Some("code"), Some(v)) => code = Some(urlencoding_decode(v)),
                    (Some("state"), Some(v)) => state = Some(urlencoding_decode(v)),
                    (Some("error"), Some(v)) => oauth_err = Some(urlencoding_decode(v)),
                    _ => {}
                }
            }
            let err_detail = if oauth_err.is_some() {
                Some(
                    "Authorization was canceled or denied in Google. Return to OpenDrive to try again.",
                )
            } else if code.is_none() {
                Some(
                    "Missing authorization code. Return to OpenDrive and try connecting your account again.",
                )
            } else if state.as_deref() != Some(expected_state) {
                Some("Security state mismatch. Return to OpenDrive and start sign-in again.")
            } else {
                None
            };
            let body = render_callback_html(err_detail);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(resp.as_bytes()).await;
            let _ = socket.flush().await;
            let _ = socket.shutdown().await;
            if let Some(err) = oauth_err {
                return Err(LoopbackError::Io(format!("Google sign-in denied: {err}")));
            }
            let code = code.ok_or(LoopbackError::BadRequest)?;
            let state = state.ok_or(LoopbackError::BadRequest)?;
            if state != expected_state {
                return Err(LoopbackError::BadRequest);
            }
            return Ok(RedirectParams { code, state });
        }
    })
    .await
    .map_err(|_| LoopbackError::Timeout)?
}

fn render_callback_html(err_detail: Option<&str>) -> String {
    let ok = err_detail.is_none();
    let (title, heading, detail, tone_fg, tone_bg, tone_border) = match err_detail {
        None => (
            "OpenDrive — Connected",
            "Google Drive connected",
            "You can close this window. Return to OpenDrive Control Center.",
            "#0f766e",
            "#dcfcf3",
            "#0f766e",
        ),
        Some(detail) => (
            "OpenDrive — Authorization Error",
            "Sign-in could not complete",
            detail,
            "#d92d20",
            "#fee4e2",
            "#b42318",
        ),
    };
    let badge_svg = if ok {
        r#"<svg width="42" height="42" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4.5 12.5L9.5 17.5L19.5 6.5"/></svg>"#
    } else {
        r#"<svg width="42" height="42" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4L21 19.5H3L12 4Z"/><path d="M12 10V14"/><circle cx="12" cy="16.8" r="1" fill="currentColor" stroke="none"/></svg>"#
    };
    let brand_svg = r##"<svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true"><polygon points="12,2 21,7 12,12 3,7" fill="#f8fafc"/><polygon points="3,7 12,12 12,22 3,17" fill="#ff3d7f"/><polygon points="12,12 21,7 21,17 12,22" fill="#00d4ff"/><polygon points="12,5 15.5,7 12,9 8.5,7" fill="#4f46e5"/><path d="M12 2L21 7V17L12 22L3 17V7Z M3 7L12 12L21 7 M12 12V22" stroke="#1c1e24" stroke-width="1.6" stroke-linejoin="round"/></svg>"##;
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title><style>
:root{{color-scheme:light;--bg:#d6d4f0;--dot:#b6b3dc;--card:#f8fafc;--ink:#1c1e24;--text:#181a20;--muted:#4a5162;--pink:#ff3d7f;--cyan:#00d4ff;--tone-fg:{tone_fg};--tone-bg:{tone_bg};--tone-border:{tone_border}}}
*{{box-sizing:border-box}}::selection{{background:#4f46e5;color:#fff}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;background:var(--bg) radial-gradient(circle,var(--dot) 1.5px,transparent 1.6px) 0 0/24px 24px;color:var(--text);font:400 14px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif}}
.card{{width:100%;max-width:420px;background:var(--card);border:1.5px solid var(--ink);border-radius:12px;padding:32px 28px;box-shadow:-3px -3px 0 var(--pink),3px 3px 0 var(--cyan);display:flex;flex-direction:column;align-items:center;text-align:center}}
.brand{{display:inline-flex;align-items:center;justify-content:center;gap:8px;font-weight:700;font-size:14px;color:var(--text);margin-bottom:24px}}
.icon-box{{width:76px;height:76px;margin:0 auto 22px;border-radius:12px;display:grid;place-items:center;color:var(--tone-fg);background:var(--tone-bg);border:2px solid var(--tone-border);box-shadow:3px 3px 0 var(--ink)}}
h1{{margin:0 0 8px;font-size:18px;font-weight:700;line-height:1.3;letter-spacing:-.015em}}
p{{margin:0;max-width:34ch;color:var(--muted);font-size:14px;line-height:1.5}}
@media(max-width:480px){{.card{{padding:26px 20px}}}}
</style></head><body><main class="card"><div class="brand">{brand_svg}<span>OpenDrive</span></div><div class="icon-box">{badge_svg}</div><h1>{heading}</h1><p>{detail}</p></main></body></html>"##
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
        assert!(!resp.contains("Authorization Complete"));
        let params = waiter.await.unwrap().unwrap();
        assert_eq!(params.code, "abc/123");
        assert_eq!(params.state, "st 1");
    }

    #[test]
    fn callback_html_uses_light_theme_centered_large_check_without_redundant_pill() {
        let ok_html = render_callback_html(None);
        assert!(ok_html.contains("color-scheme:light"));
        assert!(ok_html.contains("#d6d4f0"));
        assert!(ok_html.contains("#f8fafc"));
        assert!(ok_html.contains("#0f766e"));
        assert!(ok_html.contains("#dcfcf3"));
        assert!(ok_html.contains("align-items:center"));
        assert!(ok_html.contains("text-align:center"));
        assert!(ok_html.contains("width:76px;height:76px"));
        assert!(ok_html.contains(r#"width="42" height="42""#));
        assert!(ok_html.contains("Google Drive connected"));
        assert!(ok_html.contains("You can close this window. Return to OpenDrive Control Center."));
        assert!(!ok_html.contains("Authorization Complete"));
        assert!(!ok_html.contains(r#"class="pill""#));
        assert!(!ok_html.contains("#131519"));

        let err_html = render_callback_html(Some("Missing authorization code."));
        assert!(err_html.contains("#fee4e2"));
        assert!(err_html.contains("#d92d20"));
        assert!(err_html.contains("Sign-in could not complete"));
        assert!(err_html.contains("Missing authorization code."));
        assert!(!err_html.contains("Authorization Failed"));
        assert!(!err_html.contains(r#"class="pill""#));
    }

    #[tokio::test]
    async fn redirect_rejects_state_mismatch_missing_code_denied_and_timeout() {
        let (listener, _, addr) = bind_loopback().await.unwrap();
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "good", 5).await });
        let resp = send_raw_get(addr, "/callback?code=x&state=wrong").await;
        assert!(!resp.contains("Google Drive connected"));
        assert!(resp.contains("Security state mismatch"));
        assert!(matches!(
            waiter.await.unwrap(),
            Err(LoopbackError::BadRequest)
        ));

        let (listener, _, addr) = bind_loopback().await.unwrap();
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "good", 5).await });
        let resp = send_raw_get(addr, "/callback?state=good").await;
        assert!(resp.contains("Missing authorization code"));
        assert!(matches!(
            waiter.await.unwrap(),
            Err(LoopbackError::BadRequest)
        ));

        let (listener, _, addr) = bind_loopback().await.unwrap();
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "good", 5).await });
        let resp = send_raw_get(addr, "/callback?error=access_denied&state=good").await;
        assert!(resp.contains("canceled or denied"));
        assert!(matches!(waiter.await.unwrap(), Err(LoopbackError::Io(_))));

        let (listener, _, _) = bind_loopback().await.unwrap();
        assert!(matches!(
            wait_for_redirect(listener, "good", 0).await,
            Err(LoopbackError::Timeout)
        ));
    }

    #[tokio::test]
    async fn redirect_survives_empty_preconnect_and_favicon_before_callback() {
        let (listener, _, addr) = bind_loopback().await.unwrap();
        let waiter = tokio::spawn(async move { wait_for_redirect(listener, "st2", 5).await });
        drop(TcpStream::connect(addr).await.unwrap());
        let fav = send_raw_get(addr, "/favicon.ico").await;
        assert!(fav.contains("204 No Content"));
        let ok = send_raw_get(addr, "/callback?code=c9&state=st2").await;
        assert!(ok.contains("Google Drive connected"));
        assert_eq!(waiter.await.unwrap().unwrap().code, "c9");
    }

    #[test]
    fn urlencoding_decode_handles_percent_and_invalid() {
        assert_eq!(urlencoding_decode("a+b%20c%2Fd%ZZ"), "a b c/d%ZZ");
    }
}
