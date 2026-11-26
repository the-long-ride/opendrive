//! Coverage tests for `opendrive-auth` loopback, service account, and vault error branches.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use opendrive_auth::{
    bind_loopback, decrypt_vault, encrypt_vault, wait_for_redirect, AuthError, LoopbackError,
    ServiceAccountKey, VaultPayload,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

const TEST_PKCS8_B64: &str = concat!(
    "MIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQC4b7NqObarTaXeuE995eVK+sWHSZmmeZR/",
    "Wx1KxMCRno8rFVlebtu6+V9tDlyQkxrmOHpqV+hx5hd/0jDVz5EwN4YEowZrHDZcXHEsx99mypGLnpRH7OPg",
    "QXZmz6kjCHuWzacp+I5sfU+SVS/Y4uPyTfFaDpyvBmj8u+3zA7ARGmtlmviaFch2KiPsPZtTzbGgAGlU8Bt1",
    "Ol9sKcWgNZ5LLMbOObDvKDytzzmzHpNL6AWWghL5eBREXVBRIH/uBxc7uIX1gDmQGmFvhaB9XXoQccpipc+B",
    "/OHpk3//fKsLlUwmUetbD7a0EDL9UkO4Y4oztdHrLPTQVyfEv+InE6JBAgMBAAECggEAGz75GLSs7RJ78fJ1",
    "X8ph/JkeLBWt2p90VfoOUjjNn9V2He259Y7X6/+h3rjz6LGqlB78yo/w+T598zaIW1WeA6r4I+A3dwREbZGE",
    "cJKYSJWdOax0CEoRAdR52sdLm67JnyYDT1p2I8XBRwNJnbcq6GUmReEJuXtvcAKNLMy7KlNlnd5auUh1iFSi",
    "cjCefrSVRuxGzFWiezoiT1Dkuf0WKd88IJ0IRBn8GnRE8L9wRf4gQAX3/6W6384FlklpjwzU4GRJQpEERk1M1",
    "EzeJvcsyoNbvPFTLN1/GToe7b1+duvhZYQufN6TwitCX6BXoZrvvEpWI8UFFtpWna9CtUZHSQKBgQD2KjGwVW",
    "47MZW6NJPZGtZsPtuDWVz8kvEE4O6HjdVTs/6Vv8k0DAVHV2oC/mVO44R4CTdzvdq6OXdXSMOHNfukCdsw4H",
    "QzBOYKGgayIhaHxtb51sE0Img5p7GFNLLoh3V/SJM0J7QlLU2ZUatrWvfVTVP00fCKxGCwad56pgcjBQKBgQ",
    "C/ziPDa0S6opBmT5/ncc0ix5suf1ng0IcOOEbjnb6erYN3uiaVytD7bxvkbj7JVZ4aTLokLlxvg0CvgIQ3QX",
    "RVuBSvX3lykJwi76YG8GA+rIwlU0q11kMLXrZOZ++iAfd3r3gWpG+btp669tj8aDGyWwDtsDCw3L0qtjOrNV",
    "hfDQKBgQCra/qqNvPKWN9RiiejOmMzghpPP73XTWR150V0NDroTSHuml281toeMitq5kkaRKoNBS4OdZmJsV",
    "b/+3XeBKO2Of7f7I4+sRyY0EL1bXOPHdK6rOhiK8u0yqh+gJJOB3a1Ko8H9NYoRYISUKM04WZQrcbXaBNALz",
    "y93RvHATNbGQKBgQCdQYGI4xXCfy4s8/OCaodpkIXmpXsnG1Dfk7xrOhXjOxeivfiMr3CP9RTFQK4xnAS4Q1",
    "gJ+gjHVLFgMto0A3JCJV6wOnL0PyQEcL+KyOHgANAWeiey1iy50blep1CNX1dpxW+MAN2QqTWSzHgy2aqFNJ",
    "+aQO5q01A1CJVsNZxysQKBgQDLluomtOX0NlYXU96V0xE0a8e4WaeNoOjLfU/MBIIGzIiGSNjnM1X+CrDvWw",
    "fQhQaYqCiOqI6OvRm2sM0RqWa9579yOrtVMJxh7pVPzSaIZVaUBWDFEECzg+OUaB+UAiMujYedi01ZL9L3ci",
    "NIAT3Bfn/gIPBCddFfVTXu0ZZAwA=="
);

fn sa_json(token_uri: &str) -> String {
    let pem = format!(
        "-----{}-----\n{TEST_PKCS8_B64}\n-----{}-----\n",
        "BEGIN PRIVATE KEY", "END PRIVATE KEY"
    );
    serde_json::json!({
        "type": "service_account",
        "client_email": "bot@proj.iam.gserviceaccount.com",
        "private_key": pem,
        "token_uri": token_uri
    })
    .to_string()
}

#[tokio::test]
async fn loopback_missing_state_returns_bad_request() {
    let (listener, _, addr) = bind_loopback().await.unwrap();
    let waiter = tokio::spawn(async move { wait_for_redirect(listener, "expected", 5).await });
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"GET /callback?code=onlycode HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .unwrap();
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf).await;
    assert!(matches!(
        waiter.await.unwrap(),
        Err(LoopbackError::BadRequest)
    ));
}

#[tokio::test]
async fn service_account_json_or_path_and_missing_access_token() {
    let empty_uri = ServiceAccountKey::from_json_str(&sa_json("")).unwrap();
    assert_eq!(empty_uri.token_uri, opendrive_auth::DEFAULT_TOKEN_URI);

    let (from_inline, raw) =
        ServiceAccountKey::from_json_or_path(&sa_json("http://127.0.0.1:1")).unwrap();
    assert!(raw.starts_with('{'));
    assert_eq!(from_inline.client_email, "bot@proj.iam.gserviceaccount.com");

    let dir = std::env::temp_dir().join(format!("od-auth-cov-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("sa.json");
    std::fs::write(&p, sa_json("http://127.0.0.1:1")).unwrap();
    let (from_file, _) =
        ServiceAccountKey::from_json_or_path(&format!("\"{}\"", p.display())).unwrap();
    assert_eq!(from_file.client_email, "bot@proj.iam.gserviceaccount.com");

    let bad_pem = serde_json::json!({
        "client_email": "a@b.com",
        "private_key": "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----",
    })
    .to_string();
    assert!(ServiceAccountKey::from_json_str(&bad_pem).is_err());
    let non_pkcs8_pem = serde_json::json!({
        "client_email": "a@b.com",
        "private_key": "-----BEGIN PRIVATE KEY-----\nQUJDRA==\n-----END PRIVATE KEY-----",
    })
    .to_string();
    assert!(ServiceAccountKey::from_json_str(&non_pkcs8_pem).is_err());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let h = tokio::spawn(async move {
        if let Ok((mut s, _)) = listener.accept().await {
            let mut b = [0u8; 2048];
            let _ = s.read(&mut b).await;
            let body = "{}";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes()).await;
        }
    });
    let key = ServiceAccountKey::from_json_str(&sa_json(&format!("http://{addr}/token"))).unwrap();
    assert!(matches!(
        key.fetch_access_token(false).await,
        Err(AuthError::OAuth(_))
    ));
    h.abort();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn vault_corruption_branches() {
    let payload = VaultPayload {
        exported_at: "2026-01-01T00:00:00Z".into(),
        credentials: vec![],
        accounts: vec![],
    };
    let bytes = encrypt_vault(&payload, "GoodPass1!").unwrap();
    assert!(decrypt_vault(b"{not-json", "GoodPass1!").is_err());

    let mut env: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (field, val) in [
        ("salt_b64", "!!!"),
        ("nonce_b64", "!!!"),
        ("nonce_b64", &STANDARD.encode([1u8; 4])),
        ("ciphertext_b64", "!!!"),
    ] {
        let mut cloned = env.clone();
        cloned[field] = serde_json::json!(val);
        let bad = serde_json::to_vec(&cloned).unwrap();
        assert!(matches!(
            decrypt_vault(&bad, "GoodPass1!"),
            Err(AuthError::Vault(_))
        ));
    }
    env["iterations"] = serde_json::json!(10);
    assert!(decrypt_vault(&serde_json::to_vec(&env).unwrap(), "GoodPass1!").is_err());
    env["iterations"] = serde_json::json!(0);
    assert!(decrypt_vault(&serde_json::to_vec(&env).unwrap(), "GoodPass1!").is_err());

    let bad_c = opendrive_auth::AuthClient::with_endpoints(
        "c".into(),
        "s".into(),
        "http://127.0.0.1:1/a",
        "http://127.0.0.1:1/t",
    );
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt.block_on(bad_c.refresh("r")).is_err());
    let l = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let a = l.local_addr().unwrap();
    let h = rt.spawn(async move {
        for body in ["not-json", r#"{"error":"access_denied"}"#, r#"{"error":"access_denied","error_description":"x"}"#, r#"{"access_token":"t"}"#] {
            if let Ok((mut s, _)) = l.accept().await {
                let mut b = [0u8; 1024];
                let _ = s.read(&mut b).await;
                let r = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                let _ = s.write_all(r.as_bytes()).await;
            }
        }
    });
    let mc = opendrive_auth::AuthClient::with_endpoints(
        "c".into(),
        "s".into(),
        "http://127.0.0.1:1/a",
        format!("http://{a}/t"),
    );
    for _ in 0..4 {
        assert!(rt
            .block_on(mc.exchange_code("c", "v", "http://127.0.0.1"))
            .is_err());
    }
    h.abort();
}
