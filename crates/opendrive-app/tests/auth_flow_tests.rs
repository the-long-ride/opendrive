//! End-to-end integration tests for Browser OAuth PKCE, Copy Link / Cancel / error fallbacks, and Service Account JSON auth.

mod common_auth_helpers;

use common_auth_helpers::{
    capturing_endpoints, hanging_token_server, has_copy_text, mock_token_server, poll_until,
    run_frame, sample_service_account_json, send_oauth_callback, start_fake_studio_drive,
    MockDroppedFile,
};
use eframe::egui;
use opendrive_app::design::formatters::ConnectAuthMode;
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_auth::DEFAULT_TOKEN_URI;
use opendrive_core::{AccountStatus, Settings};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn auth_url_of(app: &ControlCenterApp) -> Option<String> {
    app.auth_session.as_ref().and_then(|s| s.auth_url.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_oauth_pkce_end_to_end_connects_persists_and_cleans_up() {
    let (api_base, upload_base, drive_h) = start_fake_studio_drive().await;
    let (token_url, tok_h) =
        mock_token_server(r#"{"access_token":"at-ok","refresh_token":"rt-ok","expires_in":3600}"#)
            .await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(token_url, api_base, upload_base, Arc::clone(&opened));

    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.draft.client_id = "cid-e2e".into();
    app.draft.client_secret = "csec-e2e".into();
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: true,
        copy_link: false,
    });

    poll_until(&mut app, |a| auth_url_of(a).is_some());
    let auth_url = auth_url_of(&app).unwrap();
    assert_eq!(
        opened.lock().unwrap().as_slice(),
        std::slice::from_ref(&auth_url)
    );

    send_oauth_callback(&auth_url);
    poll_until(&mut app, |a| a.auth_session.is_none());

    assert_eq!(app.state.accounts.len(), 1);
    let acct = &app.state.accounts[0];
    let acct_id = acct.id;
    assert_eq!(acct.nickname, "studio");
    assert_eq!(acct.email, "studio@example.com");
    assert_eq!(acct.status, AccountStatus::Connected);
    assert_eq!(acct.folder_name, "studio");
    assert!(!app.drawer_open);
    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Ok));

    let creds = bridge.with_controller(|c| c.accounts.list_credentials().to_vec());
    assert_eq!(creds.len(), 1);
    assert_eq!(creds[0].client_id, "cid-e2e");
    assert_eq!(
        bridge
            .secrets()
            .load_client_secret(creds[0].id)
            .unwrap()
            .as_deref(),
        Some("csec-e2e")
    );
    assert_eq!(
        bridge
            .secrets()
            .load_refresh_token(acct.id)
            .unwrap()
            .as_deref(),
        Some("rt-ok")
    );

    let rehydrated = ControlCenterApp::new(bridge.clone());
    assert_eq!(rehydrated.draft.client_id, "cid-e2e");
    assert_eq!(rehydrated.draft.client_secret, "csec-e2e");

    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.draft.nickname = "Alt Nick".into();
    app.draft.folder_name = "alt-folder".into();
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: true,
        copy_link: false,
    });
    poll_until(&mut app, |a| auth_url_of(a).is_some());
    send_oauth_callback(&auth_url_of(&app).unwrap());
    poll_until(&mut app, |a| a.auth_session.is_none());
    assert_eq!(app.state.accounts.len(), 2);
    assert_eq!(app.state.accounts[1].nickname, "Alt Nick");
    assert_eq!(app.state.accounts[1].folder_name, "alt-folder");
    assert_eq!(
        bridge.with_controller(|c| c.accounts.list_credentials().len()),
        1
    );

    bridge
        .account_action("remove", Some(&acct_id.to_string()))
        .unwrap();
    assert_eq!(bridge.secrets().load_refresh_token(acct_id).unwrap(), None);

    tok_h.abort();
    drive_h.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_oauth_copy_link_open_browser_cancel_and_error_fallbacks() {
    let (token_url, tok_h) = mock_token_server(r#"{"access_token":"unused"}"#).await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(
        token_url.clone(),
        String::new(),
        String::new(),
        Arc::clone(&opened),
    );

    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.draft.client_id = "cid-copy".into();
    app.draft.client_secret = "csec-copy".into();
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: false,
        copy_link: true,
    });

    poll_until(&mut app, |a| auth_url_of(a).is_some());
    let expected_url = auth_url_of(&app).unwrap();
    let ctx = egui::Context::default();
    let out1 = run_frame(&ctx, &mut app, Vec::new());
    assert!(opened.lock().unwrap().is_empty());
    assert!(app
        .notice
        .as_ref()
        .is_some_and(|n| n.tone == NoticeTone::Ok && n.message.contains("Copied")));
    assert!(has_copy_text(&out1, &expected_url));

    app.apply_action(UiAction::CopyAuthLink);
    let out2 = run_frame(&ctx, &mut app, Vec::new());
    assert!(has_copy_text(&out2, &expected_url));

    app.apply_action(UiAction::OpenAuthBrowser);
    assert_eq!(opened.lock().unwrap().as_slice(), &[expected_url]);
    app.apply_action(UiAction::CancelAuth);
    assert!(app.auth_session.is_none());

    let err_endpoints = AuthEndpoints {
        token_url,
        browser_opener: Arc::new(|_| Err("no browser".into())),
        ..Default::default()
    };
    let (err_bridge, _rx2) = Bridge::new(Settings::default());
    let mut err_app = ControlCenterApp::with_auth_endpoints(err_bridge, err_endpoints);
    err_app.apply_action(UiAction::ToggleDrawer(Some(true)));
    err_app.draft.client_id = "cid-err".into();
    err_app.draft.client_secret = "csec-err".into();
    err_app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: true,
        copy_link: false,
    });

    poll_until(&mut err_app, |a| {
        auth_url_of(a).is_some() && a.notice.as_ref().map(|n| n.tone) == Some(NoticeTone::Error)
    });
    let err_url = auth_url_of(&err_app).unwrap();
    let out_warn = run_frame(&ctx, &mut err_app, Vec::new());
    assert!(err_app.auth_session.is_some());
    assert_eq!(
        err_app.notice.as_ref().map(|n| n.tone),
        Some(NoticeTone::Error)
    );
    assert!(has_copy_text(&out_warn, &err_url));

    err_app.dismiss_notice();
    err_app.apply_action(UiAction::OpenAuthBrowser);
    let out_open_err = run_frame(&ctx, &mut err_app, Vec::new());
    assert_eq!(
        err_app.notice.as_ref().map(|n| n.tone),
        Some(NoticeTone::Error)
    );
    assert!(has_copy_text(&out_open_err, &err_url));
    err_app.apply_action(UiAction::CancelAuth);
    tok_h.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_account_drop_connect_and_failure_paths() {
    let (api_base, upload_base, drive_h) = start_fake_studio_drive().await;
    let (token_url, tok_h) =
        mock_token_server(r#"{"access_token":"sa-at","expires_in":3600}"#).await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(token_url, api_base, upload_base, Arc::clone(&opened));

    let cfg = std::env::temp_dir().join(format!("od-sa-cfg-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cfg).unwrap();
    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(cfg.clone()));

    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));

    let ctx = egui::Context::default();
    let path_drop: egui::DroppedFileHandle = Arc::new(MockDroppedFile {
        path: PathBuf::from("/tmp/service-account.json"),
        bytes: Err("unused".into()),
    });
    run_frame(&ctx, &mut app, vec![path_drop]);
    assert_eq!(app.draft.service_account_input, "/tmp/service-account.json");

    let raw_json = sample_service_account_json(DEFAULT_TOKEN_URI);
    let bytes_drop: egui::DroppedFileHandle = Arc::new(MockDroppedFile {
        path: PathBuf::new(),
        bytes: Ok(raw_json.clone().into_bytes()),
    });
    run_frame(&ctx, &mut app, vec![bytes_drop]);
    assert_eq!(app.draft.service_account_input, raw_json);

    app.apply_action(UiAction::StartServiceAccount);
    poll_until(&mut app, |a| a.auth_session.is_none());

    assert!(opened.lock().unwrap().is_empty());
    assert_eq!(app.state.accounts.len(), 1);
    let acct = &app.state.accounts[0];
    assert_eq!(acct.email, "studio@example.com");
    assert_eq!(acct.status, AccountStatus::Connected);
    assert_eq!(
        bridge
            .secrets()
            .load_service_account_json(acct.id)
            .unwrap()
            .as_deref(),
        Some(raw_json.as_str())
    );
    assert!(cfg.join("accounts.json").exists());

    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));
    app.draft.service_account_input = "not-valid-json".into();
    app.apply_action(UiAction::StartServiceAccount);
    poll_until(&mut app, |a| a.auth_session.is_none());
    assert!(app.form_error.is_some());

    let (hang_url, hang_h) = hanging_token_server().await;
    let mut hang_app = ControlCenterApp::with_auth_endpoints(
        bridge.clone(),
        capturing_endpoints(hang_url, String::new(), String::new(), Arc::clone(&opened)),
    );
    hang_app.apply_action(UiAction::ToggleDrawer(Some(true)));
    hang_app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));
    hang_app.draft.service_account_input = raw_json.clone();
    hang_app.apply_action(UiAction::StartServiceAccount);
    assert!(hang_app.auth_session.is_some());
    let _ = run_frame(&ctx, &mut hang_app, Vec::new());
    hang_app.apply_action(UiAction::CancelAuth);
    assert!(hang_app.auth_session.is_none());

    let (fail_url, fail_h) = mock_token_server(r#"{"error":"invalid_grant"}"#).await;
    let mut fail_app = ControlCenterApp::with_auth_endpoints(
        bridge,
        capturing_endpoints(fail_url, String::new(), String::new(), opened),
    );
    fail_app.apply_action(UiAction::ToggleDrawer(Some(true)));
    fail_app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));
    fail_app.draft.service_account_input = raw_json;
    fail_app.apply_action(UiAction::StartServiceAccount);
    poll_until(&mut fail_app, |a| a.auth_session.is_none());
    assert!(fail_app.form_error.is_some());

    let _ = std::fs::remove_dir_all(cfg);
    hang_h.abort();
    fail_h.abort();
    tok_h.abort();
    drive_h.abort();
}
