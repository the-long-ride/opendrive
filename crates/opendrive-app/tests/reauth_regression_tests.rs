//! Regression tests for re-authentication panic (nested runtime avatar cache), inline errors, and pending row actions.

mod common_auth_helpers;

use common_auth_helpers::{
    capturing_endpoints, has_copy_text, mock_token_server, poll_until, run_frame,
    sample_service_account_json, send_oauth_callback, start_fake_studio_drive,
};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::formatters::ConnectAuthMode;
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction};
use opendrive_app::runtime::ui::build_bridge_in_dirs_with_secrets;
use opendrive_auth::{MemorySecretStore, SecretStore};
use opendrive_core::{Account, AccountStatus, AppController, CredentialSet, Settings};
use opendrive_fake_drive::{FakeAbout, FakeDrive, FakeDriveServer};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn collect_button_center(out: &egui::FullOutput, label: &str) -> Pos2 {
    fn walk(shape: &Shape, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(ts) if ts.galley.text() == label => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()).center())
            }
            Shape::Vec(v) => v.iter().find_map(|s| walk(s, label)),
            _ => None,
        }
    }
    out.shapes
        .iter()
        .find_map(|cs| walk(&cs.shape, label))
        .unwrap_or_else(|| panic!("missing label {label}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reauth_with_data_dir_and_avatar_photo_url_does_not_panic() {
    let drive = Arc::new(FakeDrive::new());
    let tmp = std::env::temp_dir().join(format!("od-reauth-av-{}", Uuid::new_v4()));
    let (addr, drive_h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    drive.set_static_asset("/avatar.png", vec![0x89, 0x50, 0x4E, 0x47]);
    drive.set_photo_url(Some(format!("http://{addr}/avatar.png")));
    drive.set_about(FakeAbout {
        email: "marshall@example.com".into(),
        display_name: "Marshall".into(),
        storage_used: 1024,
        storage_quota: 4096,
    });
    let (token_url, tok_h) =
        mock_token_server(r#"{"access_token":"at-av","refresh_token":"rt-av","expires_in":3600}"#)
            .await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(
        token_url,
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
        Arc::clone(&opened),
    );

    let mut ctrl = AppController::new(Settings::default());
    let cred_id = Uuid::new_v4();
    let acct_id = Uuid::new_v4();
    ctrl.accounts.add_credential(CredentialSet {
        id: cred_id,
        label: "Desktop".into(),
        client_id: "cid-av".into(),
    });
    ctrl.accounts.add_account(Account {
        id: acct_id,
        nickname: "Marshall".into(),
        email: "marshall@example.com".into(),
        credential_set_id: cred_id,
        folder_name: "Marshall".into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::NeedsReauth,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    });
    let secrets = Arc::new(MemorySecretStore::new());
    secrets.store_client_secret(cred_id, "csec-av").unwrap();
    let bridge =
        build_bridge_in_dirs_with_secrets(ctrl, Some(tmp.clone()), Some(&tmp), secrets.clone())
            .unwrap();

    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: true,
        copy_link: false,
    });
    poll_until(&mut app, |a| {
        a.auth_session
            .as_ref()
            .and_then(|s| s.auth_url.as_ref())
            .is_some()
    });
    let url = app.auth_session.as_ref().unwrap().auth_url.clone().unwrap();
    send_oauth_callback(&url);
    poll_until(&mut app, |a| a.auth_session.is_none());

    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Ok));
    assert_eq!(app.state.accounts[0].status, AccountStatus::Connected);
    assert_eq!(
        secrets.load_refresh_token(acct_id).unwrap().as_deref(),
        Some("rt-av")
    );

    tok_h.abort();
    drive_h.abort();
    let _ = std::fs::remove_dir_all(tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inline_reauth_error_surfaces_toast_and_pending_row_sign_in_keeps_session() {
    let drive = Arc::new(FakeDrive::new());
    let (addr, drive_h) = FakeDriveServer::start(drive).await;
    let (token_url, tok_h) =
        mock_token_server(r#"{"error":"invalid_client","error_description":"Unauthorized"}"#).await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(
        token_url,
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
        Arc::clone(&opened),
    );

    let (bridge, _rx) = opendrive_app::runtime::bridge::Bridge::new(Settings::default());
    let cred_id = Uuid::new_v4();
    let acct_id = Uuid::new_v4();
    {
        let h = bridge.controller_handle();
        let mut g = h.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cred_id,
            label: "Desktop".into(),
            client_id: "cid-bad".into(),
        });
        g.accounts.add_account(Account {
            id: acct_id,
            nickname: "Marshall".into(),
            email: "marshall@example.com".into(),
            credential_set_id: cred_id,
            folder_name: "Marshall".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::NeedsReauth,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
    }
    bridge
        .secrets()
        .store_client_secret(cred_id, "bad-secret")
        .unwrap();

    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: true,
        copy_link: false,
    });
    poll_until(&mut app, |a| {
        a.auth_session
            .as_ref()
            .and_then(|s| s.auth_url.as_ref())
            .is_some()
    });
    let first_url = app.auth_session.as_ref().unwrap().auth_url.clone().unwrap();

    let ctx = egui::Context::default();
    let _ = run_frame(&ctx, &mut app, Vec::new());
    let out = run_frame(&ctx, &mut app, Vec::new());
    let copy_pos = collect_button_center(&out, "Copy Link");
    let click_in = |pos: Pos2, pressed: bool| RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 760.0))),
        events: vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    ctx.run_ui(click_in(copy_pos, true), |ui| app.render_root(ui))
        .textures_delta
        .clear();
    let copy_out = ctx.run_ui(click_in(copy_pos, false), |ui| app.render_root(ui));
    assert_eq!(
        app.auth_session
            .as_ref()
            .and_then(|s| s.auth_url.as_deref()),
        Some(first_url.as_str())
    );
    let after_copy = run_frame(&ctx, &mut app, Vec::new());
    assert!(has_copy_text(&copy_out, &first_url) || has_copy_text(&after_copy, &first_url));

    send_oauth_callback(&first_url);
    poll_until(&mut app, |a| a.auth_session.is_none());
    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Error));
    assert!(app
        .notice
        .as_ref()
        .unwrap()
        .message
        .contains("invalid_client"));

    tok_h.abort();
    drive_h.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_account_reauth_opens_sa_drawer_without_oauth_leak_and_updates_account() {
    let (api_base, upload_base, drive_h) = start_fake_studio_drive().await;
    let (token_url, tok_h) =
        mock_token_server(r#"{"access_token":"at-sa","expires_in":3600}"#).await;
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let endpoints = capturing_endpoints(
        token_url.clone(),
        api_base,
        upload_base,
        Arc::clone(&opened),
    );
    let (bridge, _rx) = opendrive_app::runtime::bridge::Bridge::new(Settings::default());
    let (cred_id, acct_id) = (Uuid::new_v4(), Uuid::new_v4());
    {
        let h = bridge.controller_handle();
        let mut g = h.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cred_id,
            label: "Desktop".into(),
            client_id: "cid-oauth".into(),
        });
        g.accounts.add_account(Account {
            id: acct_id,
            nickname: "BotOld".into(),
            email: "bot@example.com".into(),
            credential_set_id: Uuid::nil(),
            folder_name: "BotFolder".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::NeedsReauth,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
    }
    bridge
        .secrets()
        .store_client_secret(cred_id, "csec-oauth")
        .unwrap();

    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: true,
        copy_link: false,
    });
    assert!(app.drawer_open);
    assert_eq!(app.draft.auth_mode, ConnectAuthMode::ServiceAccount);
    assert!(app.draft.client_id.is_empty() && opened.lock().unwrap().is_empty());

    app.draft.service_account_input = sample_service_account_json(&token_url);
    app.draft.nickname = "BotUpdated".into();
    app.draft.read_only = true;
    app.apply_action(UiAction::StartServiceAccount);
    poll_until(&mut app, |a| a.auth_session.is_none());

    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Ok));
    let updated = &app.state.accounts[0];
    assert_eq!(updated.status, AccountStatus::Connected);
    assert_eq!(updated.nickname, "BotUpdated");
    assert!(updated.read_only);

    // Canceling or failing in-place reauth must clear reauth_account_id before + Connect.
    app.apply_action(UiAction::ReauthAccount {
        id: acct_id.to_string(),
        open_browser: true,
        copy_link: false,
    });
    assert_eq!(app.reauth_account_id, Some(acct_id));
    app.apply_action(UiAction::CancelAuth);
    assert_eq!(app.reauth_account_id, None);
    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    assert_eq!(app.reauth_account_id, None);

    tok_h.abort();
    drive_h.abort();
}
