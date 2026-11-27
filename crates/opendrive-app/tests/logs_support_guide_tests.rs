//! Tests for error/warning code catalog, masked logging, Terminal view, AI Support modal, visual guide, and Settings links.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui;
use opendrive_app::desktop::modals::support::build_support_prompt;
use opendrive_app::desktop::{ControlCenterApp, TerminalTab, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{Account, AccountStatus, CredentialSet, LogSeverity, Settings};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn walk_shape_texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
    match shape {
        egui::Shape::Text(t) => {
            out.push((
                t.galley.text().to_owned(),
                shape.visual_bounding_rect().center(),
            ));
        }
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk_shape_texts(s, out)),
        _ => {}
    }
}

fn collect_texts(out: &egui::FullOutput) -> Vec<String> {
    let mut items = Vec::new();
    out.shapes
        .iter()
        .for_each(|cs| walk_shape_texts(&cs.shape, &mut items));
    items.into_iter().map(|(t, _)| t).collect()
}

fn run_events(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(960.0, 760.0),
        )),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

fn warm_frame(ctx: &egui::Context, app: &mut ControlCenterApp) -> egui::FullOutput {
    let _ = run_frame(ctx, app, Vec::new());
    run_frame(ctx, app, Vec::new())
}

fn click_text(ctx: &egui::Context, app: &mut ControlCenterApp, label: &str) -> bool {
    let out = warm_frame(ctx, app);
    let mut items = Vec::new();
    out.shapes
        .iter()
        .for_each(|cs| walk_shape_texts(&cs.shape, &mut items));
    let Some(pos) = items
        .into_iter()
        .rev()
        .find_map(|(t, p)| (t == label).then_some(p))
    else {
        return false;
    };
    for pressed in [true, false] {
        let _ = run_events(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    true
}

#[test]
fn terminal_support_guide_settings_and_connect_modal_ui_work() {
    let prompt = build_support_prompt("v0.2.0");
    for pat in [
        "I use OpenDrive v0.2.0.",
        "Read these official sources first:",
        "blob/v0.2.0/llm.txt",
        "blob/main/llm.txt",
        "blob/main/CHANGELOG.md",
        "https://github.com/the-long-ride/opendrive",
        "releases/tag/v0.2.0",
        "https://github.com/the-long-ride/opendrive/releases",
        "Ask which platform I use (Windows, macOS, or Linux)",
        "Do not invent features, shortcuts, settings, or error codes.",
    ] {
        assert!(prompt.contains(pat));
    }

    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let opened_ref = Arc::clone(&opened);
    let ep = AuthEndpoints {
        browser_opener: Arc::new(move |u| {
            opened_ref.lock().unwrap().push(u.to_string());
            Ok(())
        }),
        ..Default::default()
    };
    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep);
    let ctx = egui::Context::default();

    // Terminal view with both tabs + Copy + Clear
    opendrive_app::runtime::app_logs::app_log_hub().record_event(
        LogSeverity::Warn,
        "opendrive::ui",
        Some("OD-SYNC-003"),
        "conflict sample",
    );
    app.apply_action(UiAction::ToggleTerminal(Some(true)));
    let term_texts = collect_texts(&run_frame(&ctx, &mut app, Vec::new()));
    assert!(term_texts.iter().any(|t| t == "Terminal Log"));
    assert!(term_texts.iter().any(|t| t == "App Logs"));
    app.apply_action(UiAction::CopyLogs);
    let term_copy = run_frame(&ctx, &mut app, Vec::new());
    assert!(term_copy
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyText(s) if s.contains("OD-SYNC-003"))));
    app.apply_action(UiAction::SetTerminalTab(TerminalTab::AppLogs));
    let _ = run_frame(&ctx, &mut app, Vec::new());
    app.apply_action(UiAction::ClearLogs);
    app.apply_action(UiAction::ToggleTerminal(Some(false)));

    // Support prompt modal + CopySupportPrompt
    app.apply_action(UiAction::ToggleSupportModal(Some(true)));
    let sup_texts = collect_texts(&warm_frame(&ctx, &mut app));
    assert!(sup_texts.iter().any(|t| t == "Example support prompt"));
    app.apply_action(UiAction::CopySupportPrompt);
    let out = run_frame(&ctx, &mut app, Vec::new());
    assert!(out
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyText(s) if s.contains("OpenDrive"))));
    assert!(click_text(&ctx, &mut app, "View App Logs"));
    assert!(app.terminal_open && app.terminal.active_tab == TerminalTab::AppLogs);
    app.apply_action(UiAction::ToggleTerminal(Some(false)));

    // Guide modal: Option A expanded by default, Option B collapsed; collapse A then toggle Option B
    assert!(app.guide_state.option_a_open && !app.guide_state.option_b_open);
    app.apply_action(UiAction::ToggleGuide(Some(true)));
    let guide_texts = collect_texts(&warm_frame(&ctx, &mut app));
    assert!(guide_texts.iter().any(|t| t == "GCP Project"));
    assert!(!guide_texts.iter().any(|t| t == "Service Acct"));
    assert!(click_text(
        &ctx,
        &mut app,
        "Option A — Browser Sign-In (OAuth 2.0 Desktop App)"
    ));
    assert!(!app.guide_state.option_a_open);
    assert!(click_text(
        &ctx,
        &mut app,
        "Option B — Service Account JSON (No-Login Mode)"
    ));
    assert!(app.guide_state.option_b_open);
    let guide_texts2 = collect_texts(&warm_frame(&ctx, &mut app));
    assert!(guide_texts2.iter().any(|t| t == "Service Acct"));
    app.apply_action(UiAction::ToggleGuide(Some(false)));

    let ver_lbl = concat!("v", env!("CARGO_PKG_VERSION"));
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    assert!(click_text(&ctx, &mut app, ver_lbl));
    assert!(opened
        .lock()
        .unwrap()
        .iter()
        .any(|u| u.ends_with("CHANGELOG.md")));
    app.apply_action(UiAction::ToggleSettings(Some(false)));
    let (cid, aid) = (Uuid::new_v4(), Uuid::new_v4());
    bridge.with_controller(|_| {});
    {
        let h = bridge.controller_handle();
        let mut g = h.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cid,
            label: "OAuth".into(),
            client_id: "cid-1".into(),
        });
        g.accounts.add_account(Account {
            id: aid,
            nickname: "Work".into(),
            email: "work@example.com".into(),
            credential_set_id: cid,
            folder_name: "work".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::NeedsReauth,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
    }
    app.refresh();
    let home_texts = collect_texts(&run_frame(&ctx, &mut app, Vec::new()));
    assert!(home_texts.iter().any(|t| t == "Credentials"));
    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    assert!(app.draft.client_id.is_empty() && app.draft.client_secret.is_empty());
    // Stacked Escape closes only the topmost modal (Guide before Connect)
    app.apply_action(UiAction::ToggleGuide(Some(true)));
    let esc = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = run_events(&ctx, &mut app, vec![esc.clone()]);
    assert!(!app.guide_open && app.drawer_open);
    let _ = run_events(&ctx, &mut app, vec![esc]);
    assert!(!app.drawer_open);
}
