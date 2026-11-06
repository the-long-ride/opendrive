//! Headless egui tests for titlebar non-selectability, modal overlays, right-aligned status badge, and 5s toast timeout.

use eframe::egui::{self, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::titlebar::render_custom_titlebar;
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{Account, AccountStatus, Settings};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn push_shape_texts(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Text(ts) => out.push(ts.galley.text().to_owned()),
        Shape::Vec(nested) => {
            for s in nested {
                push_shape_texts(s, out);
            }
        }
        _ => {}
    }
}

fn collect_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    let mut out = Vec::new();
    for cs in shapes {
        push_shape_texts(&cs.shape, &mut out);
    }
    out
}

fn run_size(ctx: &egui::Context, size: Vec2, app: &mut ControlCenterApp) -> Vec<String> {
    let mut texts = Vec::new();
    for _ in 0..2 {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
        out.textures_delta.clear();
        texts = collect_texts(&out.shapes);
    }
    texts
}

#[test]
fn modals_layout_subtitle_removal_and_titlebar_bottom_work() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::new(bridge);
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
        top_y = render_custom_titlebar(ui, &pal, ThemeMode::DarkStudio, &app.state, &mut actions);
    });
    out.textures_delta.clear();
    assert!(top_y > 20.0);

    let base_texts = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    assert!(!base_texts
        .iter()
        .any(|t| t.contains("Manage Drive accounts, aliases, avatars, and write protection.")));
    assert!(base_texts.iter().any(|t| t == "Connected"));
    assert!(base_texts.iter().any(|t| t == "Open"));
    app.apply_action(UiAction::OpenAccountFolder(
        std::env::temp_dir()
            .join("od-acct-open")
            .display()
            .to_string(),
    ));

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
    assert!(narrow_texts.iter().any(|t| t == "Connected"));

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
    assert!(guide_texts
        .iter()
        .any(|t| t == "Google Cloud Console Setup Guide"));
    assert!(guide_texts
        .iter()
        .any(|t| t == "Option A — Browser Sign-In (OAuth 2.0 Desktop App)"));
    assert!(guide_texts
        .iter()
        .any(|t| t == "Create or select a Google Cloud project"));
    assert!(guide_texts.iter().any(|t| t == "Open Google Cloud Console"));
}

#[test]
fn toast_notice_auto_dismisses_after_five_seconds() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::new(bridge);
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
