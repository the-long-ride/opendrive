//! Targeted UI click, drawer, titlebar, zoom, modal, and CLI dispatch coverage tests.

mod common_auth_helpers;
mod support;

use eframe::egui::{
    self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2, ViewportId,
};
use opendrive_app::design::theme::ThemeMode;
use opendrive_app::desktop::{
    ControlCenterApp, ExplorerSortColumn, NoticeTone, UiAction, UiNotice,
};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::{dispatch, dispatch_bridge};
use opendrive_core::Settings;
use opendrive_fake_drive::FakeDriveServer;
use std::sync::{Arc, Mutex};
use support::{add_acct, collect, has, seed_drive, text_pos};

fn hit(ctx: &egui::Context, app: &mut ControlCenterApp, maximized: bool, pos: Pos2) {
    for pressed in [true, false] {
        let ev = Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let _ = run_vp(ctx, app, maximized, vec![Event::PointerMoved(pos), ev]);
    }
}

fn hit_txt(ctx: &egui::Context, app: &mut ControlCenterApp, needle: &str) -> bool {
    let shapes = run_vp(ctx, app, false, Vec::new());
    let texts = collect(&shapes);
    if has(&texts, needle) {
        hit(ctx, app, false, text_pos(&shapes, needle));
        true
    } else {
        false
    }
}

fn test_endpoints(
    token_url: String,
    save_path: std::path::PathBuf,
    opened: Arc<Mutex<Vec<String>>>,
) -> AuthEndpoints {
    let urls = Arc::clone(&opened);
    let urls_b = Arc::clone(&opened);
    AuthEndpoints {
        auth_url: "http://fail.local/auth".into(),
        token_url,
        browser_opener: Arc::new(move |url| {
            urls_b.lock().unwrap().push(url.to_string());
            if url.contains("fail.local") {
                Err("simulated browser error".into())
            } else {
                Ok(())
            }
        }),
        folder_opener: Arc::new(move |p| {
            urls.lock().unwrap().push(p.to_string());
            Ok(())
        }),
        vault_save_picker: Arc::new(move || Some(save_path.clone())),
        vault_open_picker: Arc::new(|| None),
        upload_files_picker: Arc::new(Vec::new),
        upload_folder_picker: Arc::new(|| None),
        app_exe_picker: Arc::new(|| None),
        item_launcher: Arc::new(|_, _| Ok(())),
        ..AuthEndpoints::default()
    }
}

fn collect_titlebar_pills(shape: &Shape, out: &mut Vec<Rect>) {
    match shape {
        Shape::Rect(rs)
            if rs.stroke.width > 0.0
                && (rs.rect.height() - 28.0).abs() < 0.5
                && rs.rect.bottom() < 56.0 =>
        {
            out.push(rs.rect);
        }
        Shape::Vec(c) => c.iter().for_each(|s| collect_titlebar_pills(s, out)),
        _ => {}
    }
}

fn run_vp(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    maximized: bool,
    events: Vec<Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1080.0, 900.0))),
        events,
        ..Default::default()
    };
    raw.viewports.entry(ViewportId::ROOT).or_default().maximized = Some(maximized);
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out.shapes
}

#[test]
fn drawer_oauth_service_account_and_modals_cover_branches() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = std::env::temp_dir().join(format!("od-ui-cov-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let opened = Arc::new(Mutex::new(Vec::new()));
    let (tok_url, tok_srv) = rt.block_on(common_auth_helpers::mock_token_server(
        r#"{"error":"denied"}"#,
    ));
    let (bridge, _ctrl) = Bridge::new(Settings::default());
    let ep = test_endpoints(tok_url, tmp.join("vault.odcred"), Arc::clone(&opened));
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ep);
    let ctx = egui::Context::default();

    assert!(hit_txt(&ctx, &mut app, "Add First Account"));
    assert!(app.drawer_open);

    app.draft.client_id.clear();
    assert!(hit_txt(&ctx, &mut app, "Sign in with Google"));
    assert!(app.form_error.is_some() || app.notice.is_some());

    app.draft.client_id = "cid".into();
    app.draft.client_secret = "sec".into();
    assert!(hit_txt(&ctx, &mut app, "Show"));
    assert!(hit_txt(&ctx, &mut app, "Sign in with Google"));
    for lbl in ["Copy Link", "Open Browser", "Cancel Sign-In"] {
        let _ = hit_txt(&ctx, &mut app, lbl);
    }

    assert!(hit_txt(&ctx, &mut app, "Service Account JSON (No Login)"));
    app.draft.service_account_input = "{bad".into();
    assert!(hit_txt(&ctx, &mut app, "Connect Service Account"));
    assert!(hit_txt(&ctx, &mut app, "Cancel"));

    assert!(hit_txt(&ctx, &mut app, "Open Settings"));
    for lbl in ["Show", "Export (.odcred)", "Import (.odcred)"] {
        let _ = hit_txt(&ctx, &mut app, lbl);
    }
    let wheel = Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: Vec2::new(0.0, -900.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
        source: egui::MouseWheelSource::Unknown,
    };
    for _ in 0..2 {
        let _ = run_vp(
            &ctx,
            &mut app,
            false,
            vec![Event::PointerMoved(Pos2::new(540.0, 450.0)), wheel.clone()],
        );
    }
    let shapes = run_vp(&ctx, &mut app, false, Vec::new());
    let texts = collect(&shapes);
    assert!(has(&texts, "Star on GitHub"));
    hit(&ctx, &mut app, false, text_pos(&shapes, "Star on GitHub"));
    if has(&texts, "Donate") {
        hit(&ctx, &mut app, false, text_pos(&shapes, "Donate"));
    }
    assert!(opened
        .lock()
        .unwrap()
        .iter()
        .any(|u| u.contains("github.com/the-long-ride/opendrive")));
    tok_srv.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn bento_titlebar_zoom_notice_and_cli_bridge_branches() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = std::env::temp_dir().join(format!("od-ui-bento-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let opened = Arc::new(Mutex::new(Vec::new()));
    let (tok_url, tok_srv) = rt.block_on(common_auth_helpers::mock_token_server("{}"));
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(seed_drive(&[], 10, 1000)));
    let (bridge, _ctrl) = Bridge::new(Settings::default());
    let (a1, a2) = (
        add_acct(&bridge, bridge.engine(), "A1", addr, 10, 1000),
        add_acct(&bridge, bridge.engine(), "A2", addr, 10, 1000),
    );
    let ep = test_endpoints(tok_url, tmp.join("v.odcred"), Arc::clone(&opened));
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep);
    app.theme_mode = ThemeMode::DarkStudio;
    app.notice = Some(UiNotice {
        tone: NoticeTone::Error,
        message: "sync error".into(),
    });
    let ctx = egui::Context::default();
    let _ = run_vp(&ctx, &mut app, false, vec![]);
    for lbl in ["Retry", "Open Folder", "Explore"] {
        let _ = hit_txt(&ctx, &mut app, lbl);
    }
    app.apply_action(UiAction::OpenMountRoot);
    app.apply_action(UiAction::OpenAccountFolder(tmp.to_string_lossy().into()));
    app.apply_action(UiAction::ReauthAccount {
        id: "bad".into(),
        open_browser: false,
        copy_link: true,
    });
    app.apply_action(UiAction::ReauthAccount {
        id: a2.to_string(),
        open_browser: false,
        copy_link: true,
    });
    app.apply_action(UiAction::CancelAuth);
    app.apply_action(UiAction::StartEditAlias("bad".into()));
    app.apply_action(UiAction::StartEditAlias(a1.to_string()));
    if let Some(d) = &mut app.alias_edit {
        d.nickname = "Alias1".into();
    }
    app.apply_action(UiAction::SaveAlias);
    app.apply_action(UiAction::StartEditAlias(a1.to_string()));
    app.apply_action(UiAction::CancelAlias);
    for (act, id) in [
        ("toggle_sync", Some(a1.to_string())),
        ("move_down", Some(a1.to_string())),
        ("move_up", Some(a1.to_string())),
        ("remove", Some(a2.to_string())),
    ] {
        app.apply_action(UiAction::AccountAction { action: act, id });
    }
    app.apply_action(UiAction::OpenExplorer(a1.to_string()));
    for a in [
        UiAction::ExplorerSelectAll,
        UiAction::ExplorerToggleSort(ExplorerSortColumn::Size),
        UiAction::ExplorerRefresh,
        UiAction::ExplorerNavigateBreadcrumb(99),
        UiAction::ExplorerSyncUpFiles,
        UiAction::ExplorerSyncUpFolder,
        UiAction::CloseExplorer,
    ] {
        app.apply_action(a);
    }

    let key = |k: Key| Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    };
    for ev in [
        key(Key::Plus),
        key(Key::Minus),
        key(Key::Num0),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 12.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::CTRL,
            source: egui::MouseWheelSource::Unknown,
        },
    ] {
        let _ = run_vp(&ctx, &mut app, true, vec![ev]);
    }

    let shapes = run_vp(&ctx, &mut app, true, vec![]);
    let mut pills = Vec::new();
    shapes
        .iter()
        .for_each(|s| collect_titlebar_pills(&s.shape, &mut pills));
    pills.sort_by(|a, b| a.left().total_cmp(&b.left()));
    if pills.len() >= 3 {
        let n = pills.len();
        app.tray_active = true;
        hit(&ctx, &mut app, true, pills[n - 3].center());
        hit(&ctx, &mut app, true, pills[n - 2].center());
        hit(&ctx, &mut app, true, pills[n - 1].center());
        assert!(app.hidden_to_tray);
        app.tray_active = false;
        hit(&ctx, &mut app, true, pills[n - 1].center());
        assert!(app.quit_requested);
    }

    dispatch("--help", std::iter::empty()).unwrap();
    fn mk<'a>(s: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        s.iter().map(|x| (*x).to_string())
    }
    assert!(dispatch_bridge("cp", mk(&["A1", "a.txt"]), &bridge).is_err());
    assert!(dispatch_bridge("rename", mk(&["A1", "a.txt", "b.txt"]), &bridge).is_err());
    assert!(dispatch_bridge("mv", mk(&["A1", "a.txt", "b.txt"]), &bridge).is_err());
    assert!(dispatch_bridge("rm", mk(&["A1", "a.txt"]), &bridge).is_err());
    assert!(dispatch_bridge("nope", std::iter::empty(), &bridge).is_ok());

    tok_srv.abort();
    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
