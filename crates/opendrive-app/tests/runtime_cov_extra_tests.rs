//! Coverage tests for auth_runner (complete, mod), bridge_vault, and runtime::ui::bridge_from_controller.

mod common_auth_helpers;

use common_auth_helpers::{
    capturing_endpoints, mock_token_server, sample_service_account_json, send_oauth_callback,
    start_fake_studio_drive, FailingSecretStore,
};
use opendrive_app::runtime::auth_runner::{
    start_browser_oauth, start_service_account_auth, AuthEvent, AuthSession, OAuthStartParams,
    ServiceAccountParams,
};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_auth::{
    AuthError, MemorySecretStore, Result as AuthResult, SecretStore, DEFAULT_TOKEN_URI,
};
use opendrive_core::Settings;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

struct FailRefreshTokenStore(MemorySecretStore);

impl SecretStore for FailRefreshTokenStore {
    fn store_refresh_token(&self, _: Uuid, _: &str) -> AuthResult<()> {
        Err(AuthError::Keychain("rt store failed".into()))
    }
    fn load_refresh_token(&self, id: Uuid) -> AuthResult<Option<String>> {
        self.0.load_refresh_token(id)
    }
    fn delete_refresh_token(&self, id: Uuid) -> AuthResult<()> {
        self.0.delete_refresh_token(id)
    }
    fn store_client_secret(&self, id: Uuid, s: &str) -> AuthResult<()> {
        self.0.store_client_secret(id, s)
    }
    fn load_client_secret(&self, id: Uuid) -> AuthResult<Option<String>> {
        self.0.load_client_secret(id)
    }
    fn store_service_account_json(&self, id: Uuid, j: &str) -> AuthResult<()> {
        self.0.store_service_account_json(id, j)
    }
    fn load_service_account_json(&self, id: Uuid) -> AuthResult<Option<String>> {
        self.0.load_service_account_json(id)
    }
    fn delete_service_account_json(&self, id: Uuid) -> AuthResult<()> {
        self.0.delete_service_account_json(id)
    }
}

fn wait_auth_terminal(sess: &mut AuthSession, send_cb: bool) -> AuthEvent {
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut sent = false;
    while Instant::now() < deadline {
        if let Some(ev) = sess.try_recv() {
            match &ev {
                AuthEvent::BrowserReady { auth_url, .. } if send_cb && !sent => {
                    sent = true;
                    send_oauth_callback(auth_url);
                }
                AuthEvent::Completed { .. } | AuthEvent::Failed(_) => return ev,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for auth event");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_runner_timeout_http_and_store_error_paths() {
    let (api_ok, up_ok, drive_h) = start_fake_studio_drive().await;
    let (tok_ok, tok_ok_h) =
        mock_token_server(r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#).await;
    let listener_500 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr_500 = listener_500.local_addr().unwrap();
    let h_500 = tokio::spawn(async move {
        while let Ok((mut s, _)) = listener_500.accept().await {
            let mut b = [0u8; 1024];
            let _ = s.read(&mut b).await;
            let _ = s
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\n\r\n{}")
                .await;
        }
    });
    let bad_url = format!("http://{addr_500}");
    let opened = Arc::new(Mutex::new(Vec::new()));

    let params = OAuthStartParams {
        target_account_id: None,
        client_id: "cid".into(),
        client_secret: "sec".into(),
        nickname: "N".into(),
        folder_name: "N".into(),
        read_only: false,
        open_browser: false,
        copy_link: false,
    };

    // 1. Timeout waiting for redirect.
    let (br, _) = Bridge::new(Settings::default());
    let mut ep_to = capturing_endpoints(
        tok_ok.clone(),
        api_ok.clone(),
        up_ok.clone(),
        Arc::clone(&opened),
    );
    ep_to.redirect_timeout_secs = 0;
    let mut s1 = start_browser_oauth(br.clone(), ep_to, params.clone(), None);
    assert!(matches!(
        wait_auth_terminal(&mut s1, false),
        AuthEvent::Failed(_)
    ));
    std::thread::sleep(Duration::from_millis(20));
    assert!(s1.try_recv().is_none());

    // 2. Token exchange 500 error.
    let ep_tok500 = capturing_endpoints(
        format!("{bad_url}/tok"),
        api_ok.clone(),
        up_ok.clone(),
        Arc::clone(&opened),
    );
    let mut s2 = start_browser_oauth(br.clone(), ep_tok500, params.clone(), None);
    assert!(matches!(
        wait_auth_terminal(&mut s2, true),
        AuthEvent::Failed(_)
    ));

    // 3. drive.about() 500 error for OAuth and Service Account with custom token_uri.
    let ep_drv500 = capturing_endpoints(
        tok_ok.clone(),
        format!("{bad_url}/drv"),
        up_ok.clone(),
        Arc::clone(&opened),
    );
    let mut s3 = start_browser_oauth(br.clone(), ep_drv500.clone(), params.clone(), None);
    assert!(matches!(
        wait_auth_terminal(&mut s3, true),
        AuthEvent::Failed(_)
    ));

    let custom_sa = sample_service_account_json(&tok_ok);
    let mut s4 = start_service_account_auth(
        br,
        ep_drv500,
        ServiceAccountParams {
            target_account_id: None,
            json_or_path: custom_sa.clone(),
            nickname: "S".into(),
            folder_name: "S".into(),
            read_only: false,
        },
        None,
    );
    assert!(matches!(
        wait_auth_terminal(&mut s4, false),
        AuthEvent::Failed(_)
    ));

    // 4. FailingSecretStore in complete_oauth_account & complete_service_account, plus refresh_token store fail.
    let (fail_br, _) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        None,
        Arc::new(FailingSecretStore),
    );
    let ep_ok = capturing_endpoints(
        tok_ok.clone(),
        api_ok.clone(),
        up_ok.clone(),
        Arc::clone(&opened),
    );
    let mut s5 = start_browser_oauth(fail_br.clone(), ep_ok.clone(), params.clone(), None);
    assert!(matches!(
        wait_auth_terminal(&mut s5, true),
        AuthEvent::Failed(_)
    ));
    let mut s6 = start_service_account_auth(
        fail_br,
        ep_ok.clone(),
        ServiceAccountParams {
            target_account_id: None,
            json_or_path: custom_sa.clone(),
            nickname: "S".into(),
            folder_name: "S".into(),
            read_only: false,
        },
        None,
    );
    assert!(matches!(
        wait_auth_terminal(&mut s6, false),
        AuthEvent::Failed(_)
    ));

    let (rt_fail_br, _) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        None,
        Arc::new(FailRefreshTokenStore(MemorySecretStore::new())),
    );
    let mut s7 = start_browser_oauth(rt_fail_br, ep_ok.clone(), params, None);
    assert!(matches!(
        wait_auth_terminal(&mut s7, true),
        AuthEvent::Failed(_)
    ));

    // 5. target_account_id pointing to unknown id falls back to adding new account.
    let (ok_br, _) = Bridge::new(Settings::default());
    let mut s8 = start_service_account_auth(
        ok_br,
        ep_ok.clone(),
        ServiceAccountParams {
            target_account_id: Some(Uuid::new_v4()),
            json_or_path: sample_service_account_json(DEFAULT_TOKEN_URI),
            nickname: "Fallback".into(),
            folder_name: "Fallback".into(),
            read_only: true,
        },
        None,
    );
    assert!(matches!(
        wait_auth_terminal(&mut s8, false),
        AuthEvent::Completed { .. }
    ));

    let fstore = FailRefreshTokenStore(MemorySecretStore::new());
    let uid = Uuid::new_v4();
    fstore.store_client_secret(uid, "sec").unwrap();
    assert_eq!(
        fstore.load_client_secret(uid).unwrap().as_deref(),
        Some("sec")
    );
    fstore.store_service_account_json(uid, "{}").unwrap();
    assert_eq!(
        fstore.load_service_account_json(uid).unwrap().as_deref(),
        Some("{}")
    );
    fstore.delete_service_account_json(uid).unwrap();
    fstore.delete_refresh_token(uid).unwrap();

    // Unwritable config_dir fails save_to inside complete::persist_account (complete.rs L201)
    let blocker = std::env::temp_dir().join(format!("od-auth-blk-{}", Uuid::new_v4()));
    std::fs::write(&blocker, b"x").unwrap();
    let (blk_br, _) = Bridge::new_with_engine(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        Some(blocker.clone()),
    );
    let mut s9 = start_service_account_auth(
        blk_br,
        ep_ok,
        ServiceAccountParams {
            target_account_id: None,
            json_or_path: sample_service_account_json(DEFAULT_TOKEN_URI),
            nickname: "Blk".into(),
            folder_name: "Blk".into(),
            read_only: false,
        },
        None,
    );
    assert!(matches!(
        wait_auth_terminal(&mut s9, false),
        AuthEvent::Failed(_)
    ));
    let _ = std::fs::remove_file(&blocker);

    h_500.abort();
    tok_ok_h.abort();
    drive_h.abort();
}
