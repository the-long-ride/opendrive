//! Targeted UI coverage tests for narrow Grid/Ledger layouts, Settings switches, Open Rules UI, Explorer error guards, and Sync State badges.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Modifiers, PointerButton, Pos2, Rect, Shape, Vec2};
use opendrive_app::design::theme::palette;
use opendrive_app::desktop::modals::open_rules_ui::render_open_rules_section;
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{OpenWithRule, Settings};
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::NodeState;
use std::sync::Arc;
use support::{add_acct, click_at, seed_drive, send_frame, wait_entry, wait_until};
use uuid::Uuid;

fn run_size(ctx: &egui::Context, size: Vec2, app: &mut ControlCenterApp) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

fn find_rect(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Rect> {
    fn walk(s: &Shape, needle: &str) -> Option<Rect> {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()))
            }
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes.iter().find_map(|cs| walk(&cs.shape, needle))
}

#[test]
fn settings_switches_open_rules_narrow_grid_ledger_and_explorer_guards() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = seed_drive(
        &[
            (
                "d1",
                "Dir",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f1", "dirty.txt", "text/plain", "root", b"a"),
            ("f2", "conflict.txt", "text/plain", "root", b"b"),
            ("f3", "hydrating.txt", "text/plain", "root", b"c"),
        ],
        200,
        1000,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(drive));
    let (bridge, _rx) = Bridge::new(Settings::default());
    let aid1 = add_acct(&bridge, bridge.engine(), "Alpha", addr, 200, 1000);
    let aid2 = add_acct(&bridge, bridge.engine(), "Beta", addr, 300, 1000);

    let ep = AuthEndpoints {
        upload_files_picker: Arc::new(Vec::new),
        upload_folder_picker: Arc::new(|| None),
        app_exe_picker: Arc::new(|| None),
        item_launcher: Arc::new(|_, _| Ok(())),
        folder_opener: Arc::new(|_| Ok(())),
        browser_opener: Arc::new(|_| Ok(())),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ep);
    let ctx = egui::Context::default();

    // 1. Narrow Ledger layout (< 560px) and reorder chevron click.
    let _ = run_size(&ctx, Vec2::new(500.0, 720.0), &mut app);
    if let Some(r) = find_rect(
        &run_size(&ctx, Vec2::new(500.0, 720.0), &mut app).shapes,
        "Remove",
    ) {
        click_at(
            &ctx,
            &mut app,
            Pos2::new(r.right() + 26.0, r.center().y),
            PointerButton::Primary,
            Modifiers::NONE,
        );
    }

    // 2. Settings switches (Autostart, VirtualDrive, DocsMode, FolderCache, CheckUpdates).
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    let _ = send_frame(&ctx, &mut app, Vec::new());
    for title in [
        "Autostart on Login",
        "Show OpenDrive in This PC",
        "Export Google Docs as Office Files",
        "Cache Explorer Folder Paths",
        "Check for GitHub Releases",
    ] {
        let shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
        if let Some(r) = find_rect(&shapes, title) {
            click_at(
                &ctx,
                &mut app,
                Pos2::new(r.min.x - 24.0, r.center().y),
                PointerButton::Primary,
                Modifiers::NONE,
            );
        }
    }
    app.settings_open = false;

    // 3. Direct OpenWithRules section: edit mode, error banner, 2+ rules, button clicks.
    let pal = palette(app.theme_mode);
    let rules = vec![
        OpenWithRule {
            pattern: "mp4".into(),
            app_command: "vlc".into(),
        },
        OpenWithRule {
            pattern: "pdf".into(),
            app_command: "sumatra".into(),
        },
    ];
    app.open_rule_draft.editor_open = true;
    app.open_rule_draft.editing_index = Some(0);
    app.open_rule_draft.error = Some("bad rule".into());
    for label in ["Browse", "Save", "Cancel", "Add", "Edit", "Remove"] {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 600.0))),
                ..Default::default()
            },
            |ui| {
                let mut acts = Vec::new();
                render_open_rules_section(ui, &pal, &rules, &mut app.open_rule_draft, &mut acts);
            },
        );
        let pos = find_rect(&out.shapes, label).map(|r| r.center());
        out.textures_delta.clear();
        if let Some(p) = pos {
            let mut out2 = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 600.0))),
                    events: vec![
                        egui::Event::PointerMoved(p),
                        egui::Event::PointerButton {
                            pos: p,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                        egui::Event::PointerButton {
                            pos: p,
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    let mut acts = Vec::new();
                    render_open_rules_section(
                        ui,
                        &pal,
                        &rules,
                        &mut app.open_rule_draft,
                        &mut acts,
                    );
                },
            );
            out2.textures_delta.clear();
        }
    }

    // 4. Explorer error guards, cache-off switch, Dirty/Conflict/Hydrating pills, narrow Grid.
    app.apply_action(UiAction::OpenExplorer("bad-uuid".into()));
    app.apply_action(UiAction::OpenExplorer(Uuid::new_v4().to_string()));
    app.apply_action(UiAction::ExplorerNavigateNode(1));
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(99));
    app.apply_action(UiAction::ExplorerRefresh);
    app.apply_action(UiAction::ExplorerSyncDown(Vec::new()));
    app.apply_action(UiAction::ExplorerSyncUpItems(Vec::new()));
    app.apply_action(UiAction::ExplorerSyncUpAndClear(Vec::new()));
    app.apply_action(UiAction::ExplorerSyncUpFiles);
    app.apply_action(UiAction::ExplorerSyncUpFolder);

    app.apply_action(UiAction::OpenExplorer(aid1.to_string()));
    wait_entry(&ctx, &mut app, aid1, "dirty.txt");
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(99));
    app.apply_action(UiAction::ExplorerSyncUpFiles);
    app.apply_action(UiAction::ExplorerSyncUpFolder);

    if let Some(sess) = app.explorer.sessions.get_mut(&aid1) {
        for e in &mut sess.entries {
            if e.name == "dirty.txt" {
                e.state = NodeState::Dirty;
            } else if e.name == "conflict.txt" {
                e.state = NodeState::Conflict;
            } else if e.name == "hydrating.txt" {
                e.state = NodeState::Hydrating;
            }
        }
        sess.select_only(Some(sess.entries[0].id));
    }
    // Right-click folder "Dir" with clipboard set so Paste Copy Here renders in context menu.
    if let Some(s) = app.explorer.sessions.get(&aid1) {
        let f_id = s.entries.iter().find(|e| e.name == "dirty.txt").unwrap().id;
        app.apply_action(UiAction::ExplorerSetClipboard {
            node_ids: vec![f_id],
            cut: false,
        });
    }
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let narrow_out = run_size(&ctx, Vec2::new(520.0, 700.0), &mut app);
    if let Some(r) = find_rect(&narrow_out.shapes, "Dir") {
        click_at(
            &ctx,
            &mut app,
            r.center(),
            PointerButton::Secondary,
            Modifiers::NONE,
        );
        let _ = run_size(&ctx, Vec2::new(520.0, 700.0), &mut app);
    }

    // Rapid click on dirty.txt then adjacent conflict.txt selects conflict.txt without opening dirty.txt;
    // two rapid clicks on dirty.txt opens dirty.txt cleanly.
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::List));
    let list_out = run_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    let dirty_p = find_rect(&list_out.shapes, "dirty.txt").unwrap().center();
    let conf_p = find_rect(&list_out.shapes, "conflict.txt")
        .unwrap()
        .center();
    let conf_id = app.explorer.sessions[&aid1]
        .entries
        .iter()
        .find(|e| e.name == "conflict.txt")
        .unwrap()
        .id;
    click_at(
        &ctx,
        &mut app,
        dirty_p,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    click_at(
        &ctx,
        &mut app,
        conf_p,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(app.explorer.sessions[&aid1].selected_id, Some(conf_id));
    let dirty_id = app.explorer.sessions[&aid1]
        .entries
        .iter()
        .find(|e| e.name == "dirty.txt")
        .unwrap()
        .id;
    click_at(
        &ctx,
        &mut app,
        dirty_p,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let _ = send_frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerButton {
            pos: dirty_p + egui::vec2(1.0, 0.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(app.explorer.sessions[&aid1].selected_id, Some(dirty_id));

    // Re-open already loaded Explorer when explorer_folder_cache is false -> triggers cache-off reload.
    app.state.settings.explorer_folder_cache = false;
    app.apply_action(UiAction::ExplorerSelectAccount(aid1.to_string()));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));
    app.apply_action(UiAction::ExplorerSelectAccount(aid2.to_string()));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid2));

    drive_h.abort();
}
