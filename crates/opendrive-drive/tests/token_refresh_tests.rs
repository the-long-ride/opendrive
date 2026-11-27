//! Auto-refresh, 403 pass-through, proactive refresh, and coalescing tests for `DriveClient`.

use opendrive_drive::{DriveClient, DriveError, TokenFuture, TokenRefresher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct TestRefresher {
    calls: AtomicUsize,
    delay: Duration,
    outcome: std::sync::Mutex<Result<String, String>>,
}

impl TokenRefresher for TestRefresher {
    fn refresh_token(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            self.outcome.lock().unwrap().clone()
        })
    }
}

async fn start_auth_stub(
    valid_tokens: &'static [&'static str],
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let h = tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let auth = req
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("authorization").then(|| {
                            v.trim()
                                .strip_prefix("Bearer ")
                                .unwrap_or("")
                                .trim()
                                .to_string()
                        })
                    })
                    .unwrap_or_default();
                let resp = if valid_tokens.contains(&auth.as_str()) {
                    let body = r#"{"user":{"emailAddress":"u@ex.com","displayName":"User"},"storageQuota":{"limit":"100","usage":"10"}}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if auth == "forbidden-tok" {
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .into()
                } else {
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .into()
                };
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), h)
}

#[tokio::test]
async fn refreshes_on_401_coalesces_concurrent_requests_and_skips_403() {
    let (base, h) = start_auth_stub(&["fresh-tok", "proactive-tok"]).await;
    let r1 = Arc::new(TestRefresher {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(30),
        outcome: std::sync::Mutex::new(Ok("fresh-tok".into())),
    });
    let client = DriveClient::with_base_urls("expired-tok".into(), &base, &base)
        .with_token_refresher(Arc::clone(&r1) as Arc<dyn TokenRefresher>);
    let tasks: Vec<_> = (0..6)
        .map(|_| {
            let c = client.clone();
            tokio::spawn(async move { c.about().await })
        })
        .collect();
    for j in tasks {
        assert_eq!(j.await.unwrap().unwrap().email, "u@ex.com");
    }
    assert_eq!(r1.calls.load(Ordering::SeqCst), 1);
    assert_eq!(client.access_token(), "fresh-tok");

    let r403 = Arc::new(TestRefresher {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
        outcome: std::sync::Mutex::new(Ok("fresh-tok".into())),
    });
    let c403 = DriveClient::with_base_urls("forbidden-tok".into(), &base, &base)
        .with_token_refresher(Arc::clone(&r403) as Arc<dyn TokenRefresher>);
    assert!(matches!(c403.about().await, Err(DriveError::Auth)));
    assert_eq!(r403.calls.load(Ordering::SeqCst), 0);

    let r_pro = Arc::new(TestRefresher {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
        outcome: std::sync::Mutex::new(Ok("proactive-tok".into())),
    });
    let c_pro = DriveClient::with_base_urls("expired-tok".into(), &base, &base)
        .with_token_refresher(Arc::clone(&r_pro) as Arc<dyn TokenRefresher>)
        .with_proactive_refresh_age(Duration::ZERO);
    assert_eq!(c_pro.about().await.unwrap().display_name, "User");
    assert_eq!(r_pro.calls.load(Ordering::SeqCst), 1);
    assert_eq!(c_pro.access_token(), "proactive-tok");

    for outcome in [Err("revoked".into()), Ok("   ".into())] {
        let r_bad = Arc::new(TestRefresher {
            calls: AtomicUsize::new(0),
            delay: Duration::ZERO,
            outcome: std::sync::Mutex::new(outcome),
        });
        let c_bad = DriveClient::with_base_urls("expired-tok".into(), &base, &base)
            .with_token_refresher(r_bad as Arc<dyn TokenRefresher>);
        assert!(matches!(c_bad.about().await, Err(DriveError::Auth)));
    }

    // Proactive refresh failure falls back to existing valid token cleanly.
    let r_fail = Arc::new(TestRefresher {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
        outcome: std::sync::Mutex::new(Err("offline".into())),
    });
    let c_fb = DriveClient::with_base_urls("fresh-tok".into(), &base, &base)
        .with_token_refresher(r_fail as Arc<dyn TokenRefresher>)
        .with_proactive_refresh_age(Duration::ZERO);
    assert_eq!(c_fb.about().await.unwrap().email, "u@ex.com");
    h.abort();

    // Retry transport error after 401 refresh returns DriveError::Http.
    let one_shot = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let one_addr = one_shot.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((mut s, _)) = one_shot.accept().await {
            let mut b = [0u8; 1024];
            let _ = s.read(&mut b).await;
            let _ = s
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
        }
    });
    let r_drop = Arc::new(TestRefresher {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(40),
        outcome: std::sync::Mutex::new(Ok("fresh-tok".into())),
    });
    let c_drop = DriveClient::with_base_urls(
        "expired-tok".into(),
        format!("http://{one_addr}"),
        format!("http://{one_addr}"),
    )
    .with_token_refresher(r_drop as Arc<dyn TokenRefresher>);
    assert!(matches!(c_drop.about().await, Err(DriveError::Http(_))));
}
