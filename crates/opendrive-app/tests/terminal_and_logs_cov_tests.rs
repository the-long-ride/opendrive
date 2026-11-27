//! Coverage tests for in-app log hub, tracing layer, and Terminal / App Logs UI branches.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui;
use opendrive_app::desktop::terminal::{
    filter_log_entries, LogFilterLevel, TerminalTab, TerminalViewState,
};
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::app_logs::{
    app_log_hub, format_app_logs_text, format_terminal_text, AppLogEntry, AppLogHub,
    AppLogTracingLayer,
};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{LogSeverity, Settings};
use std::sync::OnceLock;
use tracing_subscriber::prelude::*;

fn walk_texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
    match shape {
        egui::Shape::Text(t) => out.push((
            t.galley.text().to_owned(),
            shape.visual_bounding_rect().center(),
        )),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk_texts(s, out)),
        _ => {}
    }
}

fn collect_texts(out: &egui::FullOutput) -> Vec<String> {
    let mut items = Vec::new();
    out.shapes
        .iter()
        .for_each(|cs| walk_texts(&cs.shape, &mut items));
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

fn click_text(ctx: &egui::Context, app: &mut ControlCenterApp, label: &str) -> bool {
    let _ = run_frame(ctx, app, Vec::new());
    let out = run_frame(ctx, app, Vec::new());
    let mut items = Vec::new();
    out.shapes
        .iter()
        .for_each(|cs| walk_texts(&cs.shape, &mut items));
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
fn app_log_hub_formatting_ring_eviction_and_tracing_layer_work() {
    static TEST_HUB: OnceLock<AppLogHub> = OnceLock::new();
    let hub = TEST_HUB.get_or_init(AppLogHub::default);
    hub.clear();

    hub.record_event(LogSeverity::Info, "opendrive::cov", None, "   \t\n ");
    assert!(hub.snapshot().is_empty());

    for i in 0..1005 {
        hub.record_event(
            LogSeverity::Info,
            "opendrive::cov",
            None,
            &format!("entry-{i}"),
        );
    }
    let snap = hub.snapshot();
    assert_eq!(snap.len(), 1000);
    assert!(snap.first().is_some_and(|e| e.message == "entry-5"));

    let fmt_entries = vec![
        AppLogEntry {
            seq: 1,
            timestamp: "2026-01-01 00:00:00Z".into(),
            level: LogSeverity::Info,
            target: "opendrive::sync".into(),
            code: None,
            message: "plain info".into(),
            terminal_line: "2026-01-01 00:00:00Z  INFO opendrive::sync: plain info".into(),
        },
        AppLogEntry {
            seq: 2,
            timestamp: "2026-01-01 00:00:01Z".into(),
            level: LogSeverity::Warn,
            target: "opendrive::sync".into(),
            code: Some("OD-SYNC-003".into()),
            message: "conflict detected".into(),
            terminal_line:
                "2026-01-01 00:00:01Z  WARN opendrive::sync: [OD-SYNC-003] conflict detected".into(),
        },
    ];
    let term_txt = format_terminal_text(&fmt_entries);
    assert!(term_txt.contains("plain info") && term_txt.contains("[OD-SYNC-003]"));
    let app_txt = format_app_logs_text(&fmt_entries);
    assert!(app_txt.contains("[-] (opendrive::sync) plain info"));
    assert!(app_txt.contains("[OD-SYNC-003] (opendrive::sync) conflict detected"));

    hub.clear();
    let sub = tracing_subscriber::registry().with(AppLogTracingLayer::with_hub(hub));
    tracing::subscriber::with_default(sub, || {
        tracing::info!(
            target: "opendrive::cov",
            code = "OD-AUTH-001",
            extra = "v",
            "info msg"
        );
        tracing::warn!(target: "external::other", detail = 7u32, "warn msg");
        tracing::error!(target: "opendrive::cov", code = ?"OD-NET-001", count = 3u32);
        tracing::info!(target: "external::other", "ignored info");
        tracing::debug!(target: "opendrive::cov", "ignored debug");
    });

    let traced = hub.snapshot();
    assert_eq!(traced.len(), 3);
    assert_eq!(traced[0].code.as_deref(), Some("OD-AUTH-001"));
    assert!(traced[0].message.contains("info msg") && traced[0].message.contains("extra=v"));
    assert_eq!(traced[1].level, LogSeverity::Warn);
    assert!(traced[1].message.contains("warn msg") && traced[1].message.contains("detail=7"));
    assert_eq!(traced[2].level, LogSeverity::Error);
    assert_eq!(traced[2].code.as_deref(), Some("OD-NET-001"));
    assert!(traced[2].message.contains("count=3"));

    hub.clear();
    assert!(hub.snapshot().is_empty());
}

#[test]
fn terminal_filters_empty_states_severities_and_panel_buttons_work() {
    let sample = vec![
        AppLogEntry {
            seq: 1,
            timestamp: "2026-01-01 00:00:00Z".into(),
            level: LogSeverity::Info,
            target: "opendrive::ui".into(),
            code: None,
            message: "started mount".into(),
            terminal_line: "started mount".into(),
        },
        AppLogEntry {
            seq: 2,
            timestamp: "2026-01-01 00:00:01Z".into(),
            level: LogSeverity::Warn,
            target: "opendrive::sync".into(),
            code: Some("OD-SYNC-003".into()),
            message: "local edit collision".into(),
            terminal_line: "collision".into(),
        },
        AppLogEntry {
            seq: 3,
            timestamp: "2026-01-01 00:00:02Z".into(),
            level: LogSeverity::Error,
            target: "opendrive::net".into(),
            code: Some("OD-CUSTOM-999".into()),
            message: "socket reset".into(),
            terminal_line: "socket reset".into(),
        },
    ];

    let st_all = TerminalViewState::default();
    assert_eq!(filter_log_entries(&sample, &st_all).len(), 3);
    let st_warn = TerminalViewState {
        level_filter: LogFilterLevel::WarnPlus,
        ..Default::default()
    };
    assert_eq!(filter_log_entries(&sample, &st_warn).len(), 2);
    let st_err = TerminalViewState {
        level_filter: LogFilterLevel::ErrorOnly,
        search_query: "opendrive::net".into(),
        ..Default::default()
    };
    assert_eq!(filter_log_entries(&sample, &st_err).len(), 1);
    let st_code = TerminalViewState {
        search_query: "od-sync-003".into(),
        ..Default::default()
    };
    assert_eq!(filter_log_entries(&sample, &st_code).len(), 1);
    let st_none = TerminalViewState {
        search_query: "does-not-exist".into(),
        ..Default::default()
    };
    assert!(filter_log_entries(&sample, &st_none).is_empty());

    let hub = app_log_hub();
    hub.clear();
    let (bridge, _rx) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::new(bridge);
    let ctx = egui::Context::default();

    app.apply_action(UiAction::ToggleTerminal(Some(true)));
    let empty_console = collect_texts(&run_frame(&ctx, &mut app, Vec::new()));
    assert!(empty_console
        .iter()
        .any(|t| t == "No console log output captured yet."));

    app.apply_action(UiAction::SetTerminalTab(TerminalTab::AppLogs));
    let empty_logs = collect_texts(&run_frame(&ctx, &mut app, Vec::new()));
    assert!(empty_logs
        .iter()
        .any(|t| t == "No matching app log entries."));

    hub.record_event(
        LogSeverity::Info,
        "opendrive::ui",
        None,
        "console info line",
    );
    hub.record_event(
        LogSeverity::Warn,
        "opendrive::sync",
        Some("OD-SYNC-003"),
        "known warn line",
    );
    hub.record_event(
        LogSeverity::Error,
        "opendrive::custom",
        Some("OD-CUSTOM-999"),
        "unknown err code",
    );
    hub.record_event(
        LogSeverity::Error,
        "opendrive::custom",
        None,
        "plain error line",
    );

    app.apply_action(UiAction::SetTerminalTab(TerminalTab::Console));
    let console_texts = collect_texts(&run_frame(&ctx, &mut app, Vec::new()));
    assert!(console_texts
        .iter()
        .any(|t| t.contains("console info line")));

    assert!(click_text(&ctx, &mut app, "App Logs"));
    assert_eq!(app.terminal.active_tab, TerminalTab::AppLogs);
    assert!(click_text(&ctx, &mut app, "Warn+"));
    assert_eq!(app.terminal.level_filter, LogFilterLevel::WarnPlus);
    assert!(click_text(&ctx, &mut app, "Errors"));
    assert_eq!(app.terminal.level_filter, LogFilterLevel::ErrorOnly);
    assert!(click_text(&ctx, &mut app, "All"));
    assert_eq!(app.terminal.level_filter, LogFilterLevel::All);

    app.apply_action(UiAction::CopyLogs);
    let copied = run_frame(&ctx, &mut app, Vec::new());
    assert!(copied
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyText(s) if s.contains("[OD-SYNC-003]"))));

    assert!(click_text(&ctx, &mut app, "Copy Logs"));
    assert!(click_text(&ctx, &mut app, "Terminal Log"));
    assert_eq!(app.terminal.active_tab, TerminalTab::Console);
    assert!(click_text(&ctx, &mut app, "Clear"));
    assert!(click_text(&ctx, &mut app, "Back"));
    assert!(!app.terminal_open);
}
