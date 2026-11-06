//! Integration tests for AuthClient, MemorySecretStore, and ServiceAccountKey JWT signing.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use opendrive_auth::{
    parse_pkcs8_pem, AuthClient, AuthError, MemorySecretStore, SecretStore, ServiceAccountKey,
    AUTH_URL, DEFAULT_TOKEN_URI,
};
use ring::signature::{UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
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

const TEST_PUB_PKCS1_B64: &str = concat!(
    "MIIBCgKCAQEAuG+zajm2q02l3rhPfeXlSvrFh0mZpnmUf1sdSsTAkZ6PKxVZXm7buvlfbQ5ckJMa5jh6alfo",
    "ceYXf9Iw1c+RMDeGBKMGaxw2XFxxLMffZsqRi56UR+zj4EF2Zs+pIwh7ls2nKfiObH1PklUv2OLj8k3xWg6c",
    "rwZo/Lvt8wOwERprZZr4mhXIdioj7D2bU82xoABpVPAbdTpfbCnFoDWeSyzGzjmw7yg8rc85sx6TS+gFloIS",
    "+XgURF1QUSB/7gcXO7iF9YA5kBphb4WgfV16EHHKYqXPgfzh6ZN//3yrC5VMJlHrWw+2tBAy/VJDuGOKM7XR",
    "6yz00FcnxL/iJxOiQQIDAQAB"
);

fn sample_pem() -> String {
    format!(
        "-----{}-----\n{TEST_PKCS8_B64}\n-----{}-----\n",
        "BEGIN PRIVATE KEY", "END PRIVATE KEY"
    )
}

async fn mock_token_server(json_body: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        if let Ok((mut s, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            let _ = s.write_all(resp.as_bytes()).await;
        }
    });
    (format!("http://{addr}/token"), handle)
}

#[test]
fn authorization_url_includes_scope_and_pkce() {
    let client = AuthClient::new("cid".into(), "sec".into());
    let ro = client.authorization_url("http://127.0.0.1:9/cb", "chal", "st", true);
    assert!(ro.contains("drive.readonly"));
    assert!(ro.contains("code_challenge=chal"));
    assert!(ro.contains("state=st"));
    let rw = client.authorization_url("http://127.0.0.1:9/cb", "chal", "st", false);
    assert!(rw.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive&"));
}

#[tokio::test]
async fn exchange_code_and_refresh_paths() {
    let (url, h) =
        mock_token_server(r#"{"access_token":"at1","refresh_token":"rt1","expires_in":3600}"#)
            .await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    let (at, rt) = client
        .exchange_code("c", "v", "http://127.0.0.1:1/cb")
        .await
        .unwrap();
    assert_eq!((at.as_str(), rt.as_str()), ("at1", "rt1"));
    h.abort();

    let (url, h) = mock_token_server(r#"{"access_token":"at2"}"#).await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert_eq!(client.refresh("rt1").await.unwrap(), "at2");
    h.abort();

    let (url, h) = mock_token_server(r#"{"error":"invalid_grant"}"#).await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert!(matches!(
        client.refresh("bad").await,
        Err(AuthError::InvalidGrant)
    ));
    h.abort();

    let (url, h) = mock_token_server(r#"{"access_token":"at_only"}"#).await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert!(matches!(
        client
            .exchange_code("c", "v", "http://127.0.0.1:1/cb")
            .await,
        Err(AuthError::OAuth(_))
    ));
    h.abort();

    let (url, h) = mock_token_server(r#"{"error":"access_denied"}"#).await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert!(matches!(
        client.refresh("rt").await,
        Err(AuthError::OAuth(ref msg)) if msg == "access_denied"
    ));
    h.abort();

    let (url, h) = mock_token_server(r#"{}"#).await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert!(matches!(
        client.refresh("rt").await,
        Err(AuthError::OAuth(_))
    ));
    h.abort();

    let (url, h) = mock_token_server(r#"{"refresh_token":"rt_only"}"#).await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert!(matches!(
        client
            .exchange_code("c", "v", "http://127.0.0.1:1/cb")
            .await,
        Err(AuthError::OAuth(_))
    ));
    h.abort();

    let (url, h) = mock_token_server("not-json").await;
    let client = AuthClient::with_endpoints("cid".into(), "sec".into(), AUTH_URL, url);
    assert!(matches!(
        client.refresh("rt").await,
        Err(AuthError::OAuth(_))
    ));
    h.abort();

    let unreachable = AuthClient::with_endpoints(
        "cid".into(),
        "sec".into(),
        AUTH_URL,
        "http://127.0.0.1:1/token",
    );
    assert!(matches!(
        unreachable.refresh("rt").await,
        Err(AuthError::OAuth(_))
    ));
}

#[test]
fn memory_secret_store_roundtrip_and_delete() {
    let store = MemorySecretStore::new();
    let acct = Uuid::new_v4();
    let cred = Uuid::new_v4();

    assert_eq!(store.load_refresh_token(acct).unwrap(), None);
    store.store_refresh_token(acct, "rt-1").unwrap();
    assert_eq!(
        store.load_refresh_token(acct).unwrap().as_deref(),
        Some("rt-1")
    );

    store.store_client_secret(cred, "sec-1").unwrap();
    assert_eq!(
        store.load_client_secret(cred).unwrap().as_deref(),
        Some("sec-1")
    );

    store
        .store_service_account_json(acct, "{\"ok\":1}")
        .unwrap();
    assert_eq!(
        store.load_service_account_json(acct).unwrap().as_deref(),
        Some("{\"ok\":1}")
    );

    store.delete_account_secrets(acct);
    assert_eq!(store.load_refresh_token(acct).unwrap(), None);
    assert_eq!(store.load_service_account_json(acct).unwrap(), None);
}

#[tokio::test]
async fn service_account_parsing_signing_and_token_exchange() {
    assert!(parse_pkcs8_pem("").is_err());
    assert!(parse_pkcs8_pem("%%%").is_err());
    assert!(parse_pkcs8_pem("AQID").is_err());
    assert!(!parse_pkcs8_pem(&sample_pem()).unwrap().is_empty());

    assert!(ServiceAccountKey::from_json_or_path("  ").is_err());
    assert!(ServiceAccountKey::from_json_or_path("/nonexistent/sa.json").is_err());
    assert!(ServiceAccountKey::from_json_str("{").is_err());
    assert!(
        ServiceAccountKey::from_json_str(r#"{"client_email":"bad","private_key":"x"}"#).is_err()
    );

    let raw_json = serde_json::json!({
        "type": "service_account",
        "client_email": "bot@proj.iam.gserviceaccount.com",
        "private_key": sample_pem(),
        "token_uri": "",
        "project_id": "proj"
    })
    .to_string();

    let (parsed, kept) = ServiceAccountKey::from_json_or_path(&raw_json).unwrap();
    assert_eq!(parsed.token_uri, DEFAULT_TOKEN_URI);
    assert_eq!(kept, raw_json);

    let tmp = std::env::temp_dir().join(format!("od-sa-{}.json", Uuid::new_v4()));
    std::fs::write(&tmp, &raw_json).unwrap();
    let quoted_path = format!("\"{}\"", tmp.display());
    let (from_file, _) = ServiceAccountKey::from_json_or_path(&quoted_path).unwrap();
    let _ = std::fs::remove_file(&tmp);

    let jwt = from_file.sign_jwt(true, 1_700_000_000).unwrap();
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3);
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(
        claims["scope"],
        "https://www.googleapis.com/auth/drive.readonly"
    );
    let pub_der = STANDARD.decode(TEST_PUB_PKCS1_B64).unwrap();
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &pub_der)
        .verify(signing_input.as_bytes(), &sig_bytes)
        .unwrap();

    let (url, h) = mock_token_server(r#"{"access_token":"sa-at","expires_in":3600}"#).await;
    let mut sa = from_file.clone();
    sa.token_uri = url;
    assert_eq!(sa.fetch_access_token(false).await.unwrap(), "sa-at");
    h.abort();

    let (url, h) = mock_token_server(r#"{"error":"invalid_grant"}"#).await;
    sa.token_uri = url;
    assert!(matches!(
        sa.fetch_access_token(false).await,
        Err(AuthError::InvalidGrant)
    ));
    h.abort();
}
