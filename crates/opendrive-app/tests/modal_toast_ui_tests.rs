//! Headless egui tests for titlebar non-selectability, modal overlays, right-aligned status badge, and 5s toast timeout.

use eframe::egui::{self, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::modals::open_rules_ui::render_open_rules_section;
use opendrive_app::desktop::titlebar::render_custom_titlebar;
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{Account, AccountStatus, Settings};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn push_shape_texts(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Text(ts) => out.push(ts.galley.text().to_owned()),
        Shape::Vec(v) => v.iter().for_each(|s| push_shape_texts(s, out)),
        _ => {}
    }
}

fn collect_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|cs| push_shape_texts(&cs.shape, &mut out));
    out
}

fn run_shapes(
    ctx: &egui::Context,
    size: Vec2,
    mut draw: impl FnMut(&mut egui::Ui),
) -> Vec<egui::epaint::ClippedShape> {
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| draw(ui));
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    shapes
}

fn run_size(ctx: &egui::Context, size: Vec2, app: &mut ControlCenterApp) -> Vec<String> {
    collect_texts(&run_shapes(ctx, size, |ui| app.render_root(ui)))
}

#[test]
fn modals_layout_subtitle_removal_and_titlebar_bottom_work() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let ep = opendrive_app::runtime::auth_runner::AuthEndpoints {
        folder_opener: std::sync::Arc::new(|_| Ok(())),
        browser_opener: std::sync::Arc::new(|_| Ok(())),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep);
    app.state.accounts = vec![Account {
        id: Uuid::new_v4(),
        nickname: "Work".into(),
        email: "work@example.com".into(),
        credential_set_id: Uuid::nil(),
        folder_name: "work".into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 1_073_741_824,
        storage_quota: 16_106_127_360,
        avatar_url: None,
    }];
    let ctx = egui::Context::default();

    let mut top_y = 0.0;
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
        ..Default::default()
    };
    let mut actions = Vec::new();
    let mut out = ctx.run_ui(raw, |ui| {
        let pal = palette(ThemeMode::DarkStudio);
        top_y = render_custom_titlebar(
            ui,
            &pal,
            ThemeMode::DarkStudio,
            &app.state,
            false,
            false,
            &mut actions,
        );
    });
    out.textures_delta.clear();
    assert!(top_y > 20.0);

    let base_texts = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    assert!(!base_texts
        .iter()
        .any(|t| t.contains("Manage Drive accounts, aliases, avatars, and write protection.")));
    assert!(base_texts.iter().any(|t| t == "Explore"));
    assert!(!base_texts.iter().any(|t| matches!(
        t.as_str(),
        "Connected" | "This PC Active" | "Sync Active" | "Links (.gdoc)"
    )));
    assert!(base_texts.iter().any(|t| t == "Open"));
    let acct_dir = std::env::temp_dir().join("od-acct-open");
    app.apply_action(UiAction::OpenAccountFolder(acct_dir.display().to_string()));

    let wheel_raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
        events: vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(0.0, 2.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::COMMAND,
        }],
        ..Default::default()
    };
    let mut wheel_out = ctx.run_ui(wheel_raw, |ui| app.render_root(ui));
    wheel_out.textures_delta.clear();
    let _ = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    assert!(ctx.zoom_factor() > 1.05, "zoom={}", ctx.zoom_factor());

    let narrow_texts = run_size(&ctx, Vec2::new(460.0, 640.0), &mut app);
    assert!(narrow_texts.iter().any(|t| t == "Explore"));

    app.settings_open = true;
    for size in [Vec2::new(960.0, 720.0), Vec2::new(610.0, 480.0)] {
        let texts = run_size(&ctx, size, &mut app);
        assert!(texts.iter().any(|t| t == "OpenDrive Settings"));
        if size.y >= 700.0 {
            assert!(texts.iter().any(|t| t == "Bandwidth"));
            assert!(texts.iter().any(|t| t == "Cache Limit"));
        }
        assert!(!texts
            .iter()
            .any(|t| matches!(t.as_str(), "Off" | "Enabled" | "Link (.gdoc)")));
    }
    app.settings_open = false;

    app.guide_open = true;
    let guide_texts = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    for needle in [
        "Google Cloud Console Setup Guide",
        "Option A — Browser Sign-In (OAuth 2.0 Desktop App)",
        "Create or select a Google Cloud project",
        "Open Google Cloud Console",
    ] {
        assert!(guide_texts.iter().any(|t| t == needle));
    }
}

#[test]
fn toast_notice_auto_dismisses_after_five_seconds() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::new(bridge.clone());
    let ctx = egui::Context::default();

    app.apply_action(UiAction::AccountAction {
        action: "pause_all",
        id: None,
    });
    assert!(app.notice.is_some());
    assert!(app.notice_expires_at.is_some());

    let texts = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    assert!(app.notice.is_some());
    assert!(texts.iter().any(|t| t.contains("Paused")));

    app.notice_expires_at = Some(Instant::now() - Duration::from_millis(50));
    let _ = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    assert!(app.notice.is_none());
    assert!(app.notice_expires_at.is_none());

    app.apply_action(UiAction::AccountAction {
        action: "resume_all",
        id: None,
    });
    assert!(app.notice.is_some());
    app.apply_action(UiAction::DismissNotice);
    assert!(app.notice.is_none());
    assert!(app.notice_expires_at.is_none());
}

#[test]
fn settings_modal_width_is_768_and_open_rules_card_supports_search_validation_and_edit() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::new(bridge.clone());
    app.settings_open = true;
    let ctx = egui::Context::default();

    let frame_shapes = run_shapes(&ctx, Vec2::new(960.0, 720.0), |ui| app.render_root(ui));
    let find_text_rect = |shapes: &[egui::epaint::ClippedShape], exact: &str| -> Option<Rect> {
        fn walk(s: &Shape, exact: &str) -> Option<Rect> {
            match s {
                Shape::Text(ts) if ts.galley.text() == exact => {
                    Some(ts.galley.rect.translate(ts.pos.to_vec2()))
                }
                Shape::Vec(v) => v.iter().find_map(|x| walk(x, exact)),
                _ => None,
            }
        }
        shapes.iter().find_map(|cs| walk(&cs.shape, exact))
    };
    let has_modal_w = |cs: &egui::epaint::ClippedShape| -> bool {
        let ok = |w: f32| (740.0..=776.0).contains(&w);
        match &cs.shape {
            Shape::Rect(r) => ok(r.rect.width()),
            Shape::Vec(v) => v
                .iter()
                .any(|s| matches!(s, Shape::Rect(r) if ok(r.rect.width()))),
            _ => false,
        }
    };
    assert!(frame_shapes.iter().any(has_modal_w));

    let search_r = find_text_rect(&frame_shapes, "Search extension or app...").unwrap();
    let add_r = find_text_rect(&frame_shapes, "Add").unwrap();
    assert!((search_r.center().y - add_r.center().y).abs() < 8.0);

    let render_rules_shapes = |app: &mut ControlCenterApp| {
        run_shapes(&ctx, Vec2::new(720.0, 500.0), |ui| {
            let (mut acts, pal) = (Vec::new(), palette(ThemeMode::DarkStudio));
            let rules = &app.state.settings.open_with_rules;
            render_open_rules_section(ui, &pal, rules, &mut app.open_rule_draft, &mut acts);
        })
    };

    app.apply_action(UiAction::StartAddOpenWithRule);
    assert!(app.open_rule_draft.editor_open);
    let ed_shapes = render_rules_shapes(&mut app);
    let ed_texts = collect_texts(&ed_shapes);
    for lbl in ["Extension / Group", "Application", "Browse", "Save"] {
        assert!(ed_texts.iter().any(|t| t == lbl));
    }
    let find_r = |s| find_text_rect(&ed_shapes, s).unwrap();
    let (ext_r, app_r, brw_r) = (
        find_r("Extension / Group"),
        find_r("Application"),
        find_r("Browse"),
    );
    let (title_r, cancel_r, save_r) = (
        find_r("New Open With Rule"),
        find_r("Cancel"),
        find_r("Save"),
    );
    assert!(
        (ext_r.center().y - app_r.center().y).abs() < 4.0
            && app_r.left() > ext_r.left()
            && brw_r.left() > app_r.left()
    );
    assert!(
        (title_r.center().y - save_r.center().y).abs() < 6.0
            && (cancel_r.center().y - save_r.center().y).abs() < 4.0
    );
    assert!(
        title_r.right() < cancel_r.left()
            && cancel_r.right() < save_r.left()
            && save_r.bottom() < brw_r.top()
    );
    (app.open_rule_draft.pattern, app.open_rule_draft.app_command) =
        ("foo.bar".into(), "vlc".into());
    app.apply_action(UiAction::SaveOpenWithRule);
    assert!(app.open_rule_draft.editor_open && app.open_rule_draft.error.is_some());

    app.open_rule_draft.pattern = ".MP4, mkv".into();
    app.apply_action(UiAction::SaveOpenWithRule);
    assert!(!app.open_rule_draft.editor_open && app.state.settings.open_with_rules.len() == 1);

    app.apply_action(UiAction::StartAddOpenWithRule);
    (app.open_rule_draft.pattern, app.open_rule_draft.app_command) = ("mkv".into(), "mpv".into());
    app.apply_action(UiAction::SaveOpenWithRule);
    assert!(app.open_rule_draft.editor_open && app.open_rule_draft.error.is_some());

    app.apply_action(UiAction::CancelOpenWithRuleEdit);
    app.apply_action(UiAction::StartEditOpenWithRule(0));
    app.open_rule_draft.pattern = "mp4, webm".into();
    app.apply_action(UiAction::SaveOpenWithRule);
    assert_eq!(app.state.settings.open_with_rules[0].pattern, "mp4, webm");
    app.apply_action(UiAction::StartAddOpenWithRule);
    (app.open_rule_draft.pattern, app.open_rule_draft.app_command) =
        ("pdf".into(), "sumatrapdf".into());
    app.apply_action(UiAction::SaveOpenWithRule);
    app.open_rule_draft.search_query = "webm".into();
    let flt = collect_texts(&render_rules_shapes(&mut app));
    assert!(flt.iter().any(|t| t == "mp4, webm") && !flt.iter().any(|t| t == "sumatrapdf"));
    let ch = bridge.controller_handle();
    ch.lock()
        .unwrap()
        .settings
        .open_with_rules
        .push(opendrive_core::OpenWithRule {
            pattern: "pdf".into(),
            app_command: "foxit".into(),
        });
    app.state = bridge.state_snapshot();
    app.apply_action(UiAction::RemoveOpenWithRule(2));
    assert_eq!(app.state.settings.open_with_rules.len(), 2);
}
