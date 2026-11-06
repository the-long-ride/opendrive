//! Integration tests for per-account NeedsReauth Sign In / Copy Link flows and token persistence.

mod common_auth_helpers;

use common_auth_helpers::{
    capturing_endpoints, has_copy_text, mock_token_server, poll_until, run_frame,
    send_oauth_callback, start_fake_studio_drive, FailingSecretStore,
};
use eframe::egui;
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_auth::{MemorySecretStore, SecretStore};
use opendrive_core::{Account, AccountStatus, CredentialSet, Settings};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn seed_needs_reauth_account(bridge: &Bridge, cred_id: Uuid, nickname: &str, folder: &str) -> Uuid {
    let acct_id = Uuid::new_v4();
    bridge.with_controller(|_| {});
    {
        let handle = bridge.controller_handle();
        let mut g = handle.lock().unwrap();
        g.accounts.add_account(Account {
            id: acct_id,
            nickname: nickname.into(),
            email: "old@example.com".into(),
            credential_set_id: cred_id,
            folder_name: folder.into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::NeedsReauth,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
    }
    acct_id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reauth_with_saved_credentials_supports_copy_link_and_open_browser_in_place() {
    let (api_base, upload_base, drive_h) = start_fake_studio_drive().await;
    let (token_url, tok_h) = mock_token_server(
        r#"{"access_token":"at-reauth","refresh_token":"rt-reauth","expires_in":3600}"#,
    )
    .await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(token_url, api_base, upload_base, Arc::clone(&opened));

    let (bridge, _rx) = Bridge::new(Settings::default());
    let cred_id = Uuid::new_v4();
    {
        let handle = bridge.controller_handle();
        handle
            .lock()
            .unwrap()
            .accounts
            .add_credential(CredentialSet {
                id: cred_id,
                label: "Desktop OAuth".into(),
                client_id: "cid-reauth".into(),
            });
    }
    bridge
        .secrets()
        .store_client_secret(cred_id, "csec-reauth")
        .unwrap();
    let acct_id = seed_needs_reauth_account(&bridge, cred_id, "Marshall", "Marshall");

    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: false,
        copy_link: true,
    });
    poll_until(&mut app, |a| {
        a.auth_session
            .as_ref()
            .and_then(|s| s.auth_url.as_ref())
            .is_some()
    });
    assert!(!app.drawer_open);
    assert_eq!(
        app.auth_session.as_ref().and_then(|s| s.target_account),
        Some(acct_id)
    );
    let copy_url = app
        .auth_session
        .as_ref()
        .and_then(|s| s.auth_url.clone())
        .unwrap();

    let ctx = egui::Context::default();
    let out = run_frame(&ctx, &mut app, Vec::new());
    assert!(opened.lock().unwrap().is_empty());
    assert!(has_copy_text(&out, &copy_url));

    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: true,
        copy_link: false,
    });
    poll_until(&mut app, |a| {
        a.auth_session
            .as_ref()
            .and_then(|s| s.auth_url.as_ref())
            .is_some_and(|u| u != &copy_url)
    });
    let live_url = app
        .auth_session
        .as_ref()
        .and_then(|s| s.auth_url.clone())
        .unwrap();
    assert_eq!(
        opened.lock().unwrap().as_slice(),
        std::slice::from_ref(&live_url)
    );

    send_oauth_callback(&live_url);
    poll_until(&mut app, |a| a.auth_session.is_none());

    assert_eq!(app.state.accounts.len(), 1);
    let updated = &app.state.accounts[0];
    assert_eq!(updated.id, acct_id);
    assert_eq!(updated.nickname, "Marshall");
    assert_eq!(updated.folder_name, "Marshall");
    assert_eq!(updated.email, "studio@example.com");
    assert_eq!(updated.status, AccountStatus::Connected);
    assert!(updated.storage_quota > 0);
    assert_eq!(
        bridge
            .secrets()
            .load_refresh_token(acct_id)
            .unwrap()
            .as_deref(),
        Some("rt-reauth")
    );
    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Ok));

    tok_h.abort();
    drive_h.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reauth_missing_credentials_opens_prefilled_drawer_and_updates_in_place() {
    let (api_base, upload_base, drive_h) = start_fake_studio_drive().await;
    let (token_url, tok_h) = mock_token_server(
        r#"{"access_token":"at-reauth2","refresh_token":"rt-reauth2","expires_in":3600}"#,
    )
    .await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(token_url, api_base, upload_base, Arc::clone(&opened));

    let (bridge, _rx) = Bridge::new(Settings::default());
    let acct_id = seed_needs_reauth_account(&bridge, Uuid::nil(), "Marshall", "Marshall");

    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: true,
        copy_link: false,
    });
    assert!(app.drawer_open);
    assert_eq!(app.reauth_account_id, Some(acct_id));
    assert!(app.form_error.is_some());
    assert_eq!(app.draft.nickname, "Marshall");
    assert_eq!(app.draft.folder_name, "Marshall");
    assert!(run_frame(&egui::Context::default(), &mut app, Vec::new())
        .platform_output
        .commands
        .is_empty());

    app.draft.client_id = "cid-fresh".into();
    app.draft.client_secret = "csec-fresh".into();
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: true,
        copy_link: false,
    });
    poll_until(&mut app, |a| {
        a.auth_session
            .as_ref()
            .and_then(|s| s.auth_url.as_ref())
            .is_some()
    });
    let live_url = app
        .auth_session
        .as_ref()
        .and_then(|s| s.auth_url.clone())
        .unwrap();
    send_oauth_callback(&live_url);
    poll_until(&mut app, |a| a.auth_session.is_none());

    assert!(!app.drawer_open);
    assert_eq!(app.reauth_account_id, None);
    assert_eq!(app.state.accounts.len(), 1);
    assert_eq!(app.state.accounts[0].id, acct_id);
    assert_eq!(app.state.accounts[0].status, AccountStatus::Connected);
    assert_eq!(
        bridge
            .secrets()
            .load_refresh_token(acct_id)
            .unwrap()
            .as_deref(),
        Some("rt-reauth2")
    );
    tok_h.abort();
    drive_h.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_secret_persists_on_oauth_start_and_keyring_errors_surface() {
    let dir = std::env::temp_dir().join(format!("od-sec-{}", Uuid::new_v4()));
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
    let cred_id = Uuid::new_v4();
    let (bridge, _rx) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        Some(dir.clone()),
        secrets.clone(),
    );
    {
        let handle = bridge.controller_handle();
        handle
            .lock()
            .unwrap()
            .accounts
            .add_credential(CredentialSet {
                id: cred_id,
                label: "Desktop OAuth".into(),
                client_id: "cid-keep".into(),
            });
    }
    let acct_id = seed_needs_reauth_account(&bridge, cred_id, "Marshall", "Marshall");

    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), AuthEndpoints::default());
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: false,
        copy_link: true,
    });
    assert!(app.drawer_open);
    assert_eq!(app.draft.client_id, "cid-keep");
    assert!(app
        .form_error
        .as_deref()
        .is_some_and(|e| e.contains("OAuth Client Secret")));

    app.draft.client_secret = "GOCSPX-saved-early".into();
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: false,
        copy_link: true,
    });
    assert!(app.form_error.is_none());
    app.apply_action(UiAction::CancelAuth);

    let creds = opendrive_core::load_credentials(&dir).unwrap();
    assert_eq!(creds.len(), 1);
    assert_eq!(
        secrets.load_client_secret(cred_id).unwrap().as_deref(),
        Some("GOCSPX-saved-early")
    );
    let reopened = ControlCenterApp::new(bridge);
    assert_eq!(reopened.draft.client_id, "cid-keep");
    assert_eq!(reopened.draft.client_secret, "GOCSPX-saved-early");

    let (fail_bridge, _rx2) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        None,
        Arc::new(FailingSecretStore),
    );
    let mut fail_app = ControlCenterApp::new(fail_bridge);
    fail_app.apply_action(UiAction::ToggleDrawer(Some(true)));
    fail_app.draft.client_id = "cid-fail".into();
    fail_app.draft.client_secret = "sec-fail".into();
    fail_app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: false,
        copy_link: true,
    });
    assert!(fail_app
        .form_error
        .as_deref()
        .is_some_and(|e| e.contains("keyring locked")));
    let _ = std::fs::remove_dir_all(&dir);
}
