//! Unit tests for ControlCenterApp actions, virtual drive toggle, and mount folder opening.

use eframe::egui::{self, Pos2, RawInput, Rect, Vec2};
use opendrive_app::design::formatters::{ConnectAuthMode, BANDWIDTH_20M, BANDWIDTH_5M, GIB};
use opendrive_app::design::theme::ThemeMode;
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::{DocsMode, Settings};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[test]
fn control_center_actions_theme_toggle_and_errors_work() {
    let tmp_root = std::env::temp_dir().join(format!("od-ui-root-{}", Uuid::new_v4()));
    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, None);
    let bridge = bridge.with_mount_manager(Arc::new(
        MountManager::new_null().with_mount_root(tmp_root.clone()),
    ));

    let opened_folders = Arc::new(Mutex::new(Vec::<String>::new()));
    let folder_ref = Arc::clone(&opened_folders);
    let endpoints = AuthEndpoints {
        browser_opener: Arc::new(|_| Ok(())),
        folder_opener: Arc::new(move |p| {
            folder_ref.lock().unwrap().push(p.to_string());
            Ok(())
        }),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);

    assert_eq!(app.theme_mode, ThemeMode::DarkStudio);
    app.apply_action(UiAction::ToggleTheme);
    assert_eq!(app.theme_mode, ThemeMode::ColorPop);
    app.apply_action(UiAction::ToggleTheme);
    assert_eq!(app.theme_mode, ThemeMode::DarkStudio);

    assert!(!app.settings_open);
    app.apply_action(UiAction::ToggleSettings(None));
    assert!(app.settings_open);
    app.apply_action(UiAction::ToggleSettings(Some(false)));
    assert!(!app.settings_open);

    app.apply_action(UiAction::ToggleDrawer(None));
    assert!(app.drawer_open);

    assert!(!app.draft.show_secret);
    app.apply_action(UiAction::ToggleSecretVisibility);
    assert!(app.draft.show_secret);
    app.apply_action(UiAction::ToggleSecretVisibility);
    assert!(!app.draft.show_secret);

    app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));
    assert_eq!(app.draft.auth_mode, ConnectAuthMode::ServiceAccount);
    app.apply_action(UiAction::StartServiceAccount);
    assert!(app.form_error.is_some());
    assert!(app.auth_session.is_none());

    app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::BrowserOAuth));
    assert!(app.form_error.is_none());
    app.draft.client_id.clear();
    app.draft.client_secret.clear();
    let start_oauth = UiAction::StartBrowserOAuth {
        open_browser: false,
        copy_link: false,
    };
    app.apply_action(start_oauth.clone());
    assert!(app.form_error.is_some());
    app.draft.client_id = "cid-test".into();
    app.apply_action(start_oauth.clone());
    assert!(app.form_error.is_some());

    app.apply_action(UiAction::CancelAuth);
    assert!(app.auth_session.is_none());
    assert!(app.notice.is_none());

    app.draft.client_secret = "csec-test".into();
    app.apply_action(start_oauth);
    assert!(app.form_error.is_none());
    assert!(app.auth_session.is_some());
    app.apply_action(UiAction::CancelAuth);
    assert!(app.auth_session.is_none());
    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Ok));

    bridge
        .add_account(
            "Work".into(),
            "work@example.com".into(),
            "work".into(),
            false,
        )
        .unwrap();
    app.apply_action(UiAction::Refresh);
    assert_eq!(app.state.accounts.len(), 1);

    let id = app.state.accounts[0].id.to_string();
    for action in [
        "pause",
        "resume",
        "toggle_readonly",
        "pause_all",
        "resume_all",
        "other",
        "remove",
    ] {
        let target = (!(action.ends_with("all") || action == "other")).then(|| id.clone());
        app.apply_action(UiAction::AccountAction { action, id: target });
    }
    assert!(app.state.accounts.is_empty());

    app.apply_action(UiAction::SetBandwidth(Some(BANDWIDTH_5M)));
    assert_eq!(app.state.settings.bandwidth_limit, Some(BANDWIDTH_5M));
    app.apply_action(UiAction::SetBandwidth(None));
    assert_eq!(app.state.settings.bandwidth_limit, None);
    app.apply_action(UiAction::SetCacheMax(25 * GIB));
    assert_eq!(app.state.settings.cache_max_size, 25 * GIB);
    app.apply_action(UiAction::SetDocsMode(DocsMode::ExportOffice));
    assert_eq!(app.state.settings.docs_mode, DocsMode::ExportOffice);
    app.apply_action(UiAction::SetDocsMode(DocsMode::Link));
    app.apply_action(UiAction::ToggleAutostart);
    assert!(app.state.settings.autostart);
    app.apply_action(UiAction::ToggleAutostart);
    assert!(!app.state.settings.autostart);

    app.apply_action(UiAction::OpenMountRoot);
    assert!(tmp_root.is_dir());
    app.apply_action(UiAction::ToggleVirtualDrive);
    assert!(app.state.settings.virtual_drive_enabled);
    app.apply_action(UiAction::OpenMountRoot);
    let root_s = tmp_root.display().to_string();
    assert_eq!(
        opened_folders.lock().unwrap().as_slice(),
        [root_s.clone(), root_s]
    );
    app.apply_action(UiAction::ToggleVirtualDrive);
    assert!(!app.state.settings.virtual_drive_enabled);

    app.apply_action(UiAction::PurgeCache);
    app.apply_action(UiAction::DismissNotice);
    assert!(app.notice.is_none());
    app.apply_action(UiAction::Refresh);

    let blocker = std::env::temp_dir().join(format!("od-ui-err-{}", Uuid::new_v4()));
    std::fs::write(&blocker, b"x").unwrap();
    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (err_bridge, _) =
        Bridge::new_with_engine(Settings::default(), engine, Some(blocker.clone()));
    assert!(err_bridge
        .add_account("Fail".into(), "f@x.com".into(), "fail".into(), false)
        .is_err());
    let err_endpoints = AuthEndpoints {
        folder_opener: Arc::new(|_| Err("open failed".into())),
        ..Default::default()
    };
    let mut err_app = ControlCenterApp::with_auth_endpoints(err_bridge, err_endpoints);
    for act in [
        UiAction::AccountAction {
            action: "pause_all",
            id: None,
        },
        UiAction::SetBandwidth(Some(BANDWIDTH_20M)),
        UiAction::OpenMountRoot,
    ] {
        err_app.apply_action(act);
        assert_eq!(
            err_app.notice.as_ref().map(|n| n.tone),
            Some(NoticeTone::Error)
        );
    }

    let mm = MountManager::new_null().with_mount_root(blocker.clone());
    let vd_b = Bridge::new(Settings::default())
        .0
        .with_mount_manager(Arc::new(mm));
    let mut vd_app = ControlCenterApp::new(vd_b.clone());
    vd_b.update_virtual_drive(true).unwrap();
    let ctx = egui::Context::default();
    vd_b.spawn_mount_sync(Some(ctx.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        let mut out = ctx.run_ui(RawInput::default(), |ui| vd_app.render_root(ui));
        out.textures_delta.clear();
        if vd_app.notice.as_ref().map(|n| n.tone) == Some(NoticeTone::Error) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        vd_app.notice.as_ref().map(|n| n.tone),
        Some(NoticeTone::Error)
    );

    // Inline alias editing & encrypted vault backup actions.
    bridge
        .add_account(
            "Work".into(),
            "work@example.com".into(),
            "work".into(),
            false,
        )
        .unwrap();
    app.apply_action(UiAction::Refresh);
    let acct_id = app.state.accounts[0].id.to_string();
    app.apply_action(UiAction::StartEditAlias(acct_id));
    assert!(app.alias_edit.is_some());
    app.alias_edit.as_mut().unwrap().nickname = "Custom Alias".into();
    app.alias_edit.as_mut().unwrap().rename_folder = true;
    app.apply_action(UiAction::SaveAlias);
    assert!(app.alias_edit.is_none());
    assert_eq!(app.state.accounts[0].nickname, "Custom Alias");
    assert_eq!(app.state.accounts[0].folder_name, "Custom Alias");

    let vault_file = tmp_root.join("backup.odcred");
    let vf_save = vault_file.clone();
    let vf_open = vault_file.clone();
    let vault_endpoints = AuthEndpoints {
        vault_save_picker: Arc::new(move || Some(vf_save.clone())),
        vault_open_picker: Arc::new(move || Some(vf_open.clone())),
        ..Default::default()
    };
    let mut vault_app = ControlCenterApp::with_auth_endpoints(bridge.clone(), vault_endpoints);
    vault_app.apply_action(UiAction::ToggleVaultSecret);
    assert!(vault_app.vault_draft.show_password);
    vault_app.apply_action(UiAction::OpenVaultBackup);
    assert!(vault_app.settings_open && !vault_app.drawer_open);
    vault_app.vault_draft.password = "StrongVault9!".into();
    vault_app.vault_draft.confirm_password = "Mismatch!".into();
    vault_app.apply_action(UiAction::ExportVault);
    assert!(vault_app.vault_draft.feedback_is_err);
    vault_app.vault_draft.confirm_password = "StrongVault9!".into();
    vault_app.apply_action(UiAction::ExportVault);
    assert!(vault_file.exists());
    vault_app.vault_draft.password = "StrongVault9!".into();
    vault_app.apply_action(UiAction::ImportVault);
    assert!(!vault_app.vault_draft.feedback_is_err);
    app.apply_action(UiAction::WindowMinimize);
    app.apply_action(UiAction::WindowToggleMaximize);
    app.apply_action(UiAction::WindowRequestClose);
    app.apply_action(UiAction::WindowShowFromTray);

    let _ = std::fs::remove_dir_all(tmp_root);
    let _ = std::fs::remove_file(blocker);
}

#[test]
fn cloud_console_guide_modal_toggles_renders_and_opens_console() {
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let opened_ref = Arc::clone(&opened);
    let (bridge, _rx) = Bridge::new(Settings::default());
    let ok_ep = AuthEndpoints {
        browser_opener: Arc::new(move |url| {
            opened_ref.lock().unwrap().push(url.to_string());
            Ok(())
        }),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ok_ep);
    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.apply_action(UiAction::ToggleGuide(Some(true)));
    assert!(app.guide_open);
    let ctx = egui::Context::default();
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    assert!(!out.shapes.is_empty());
    app.apply_action(UiAction::OpenCloudConsole);
    assert_eq!(
        opened.lock().unwrap().as_slice(),
        ["https://console.cloud.google.com/"]
    );
    app.apply_action(UiAction::ToggleGuide(Some(false)));
    assert!(!app.guide_open);

    let (bridge2, _rx2) = Bridge::new(Settings::default());
    let err_ep = AuthEndpoints {
        browser_opener: Arc::new(|_| Err("no browser".into())),
        ..Default::default()
    };
    let mut err_app = ControlCenterApp::with_auth_endpoints(bridge2, err_ep);
    err_app.apply_action(UiAction::OpenCloudConsole);
    assert_eq!(
        err_app.notice.as_ref().map(|n| n.tone),
        Some(NoticeTone::Error)
    );
}
