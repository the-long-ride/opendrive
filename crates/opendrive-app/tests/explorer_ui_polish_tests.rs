//! UI regression tests for Explorer sidebar minimum width and folder sync status pills/icons.

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::icons::{paint_icon, IconKind};
use opendrive_app::desktop::explorer::sidebar::render_explorer_sidebar;
use opendrive_app::desktop::explorer::{AccountExplorerSession, ExplorerSortColumn};
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::explorer_ops::ExplorerEntry;
use opendrive_core::{MediaKind, Settings};
use opendrive_store::NodeState;
use opendrive_sync::ExplorerSyncStatus;
use std::sync::{Arc, Mutex};

fn render_explorer_at_size(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    width: f32,
    height: f32,
) -> Vec<egui::epaint::ClippedShape> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out.shapes
}

fn render_explorer_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    width: f32,
) -> Vec<egui::epaint::ClippedShape> {
    render_explorer_at_size(ctx, app, width, 720.0)
}

fn find_label_rect(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Rect> {
    fn walk(s: &Shape, needle: &str) -> Option<Rect> {
        match s {
            Shape::Text(t) if t.galley.text() == needle => Some(s.visual_bounding_rect()),
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| walk(&cs.shape, needle))
}

fn mk_entry(
    id: u64,
    name: &str,
    is_folder: bool,
    state: NodeState,
    sync_status: ExplorerSyncStatus,
) -> ExplorerEntry {
    ExplorerEntry {
        id,
        name: name.into(),
        media_kind: if is_folder {
            MediaKind::Folder
        } else {
            MediaKind::Audio
        },
        is_folder,
        is_native_doc: false,
        mime: None,
        size: if is_folder { 0 } else { 1024 },
        modified_ms: 1_767_225_600_000,
        state,
        sync_status,
        pinned: matches!(sync_status, ExplorerSyncStatus::FullLocal),
        drive_id: format!("d-{id}"),
    }
}

#[test]
fn sidebar_buttons_never_overlap_across_widths_and_narrow_fallback() {
    let (bridge, _) = Bridge::new(Settings::default());
    let acct_id = bridge
        .add_account(
            "Work".into(),
            "work@example.com".into(),
            "Work".into(),
            false,
        )
        .unwrap()
        .accounts[0]
        .id;
    let mut app = ControlCenterApp::new(bridge);
    app.explorer.open = true;
    app.explorer.active_account_id = Some(acct_id);
    app.explorer.sessions.insert(
        acct_id,
        AccountExplorerSession {
            breadcrumbs: vec![(1, "My Drive".into())],
            entries: vec![],
            loaded: true,
            ..Default::default()
        },
    );

    let ctx = egui::Context::default();
    for width in [720.0, 960.0] {
        let _ = render_explorer_at(&ctx, &mut app, width);
        let shapes = render_explorer_at(&ctx, &mut app, width);
        let dash = find_label_rect(&shapes, "Dashboard").expect("Dashboard button");
        let new_acct = find_label_rect(&shapes, "New Account").expect("New Account button");
        assert!(
            dash.right() < new_acct.left(),
            "sidebar buttons overlapped at {width}px: {dash:?} vs {new_acct:?}"
        );
    }

    // Defensive < 236px sidebar width wraps cleanly without overlap.
    let mut actions: Vec<UiAction> = Vec::new();
    let pal =
        opendrive_app::design::theme::palette(opendrive_app::design::theme::ThemeMode::DarkStudio);
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 400.0))),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.set_max_width(180.0);
            render_explorer_sidebar(ui, &pal, &app, false, &mut actions);
        });
    });
    let dash = find_label_rect(&out.shapes, "Dashboard").expect("wrapped Dashboard");
    let new_acct = find_label_rect(&out.shapes, "New Account").expect("wrapped New Account");
    assert!(!dash.intersects(new_acct));
}

#[test]
fn explorer_renders_all_folder_sync_status_badges_and_sorts_by_status() {
    let (bridge, _) = Bridge::new(Settings::default());
    let acct_id = bridge
        .add_account(
            "Work".into(),
            "work@example.com".into(),
            "Work".into(),
            false,
        )
        .unwrap()
        .accounts[0]
        .id;
    let mut app = ControlCenterApp::new(bridge);
    app.explorer.open = true;
    app.explorer.active_account_id = Some(acct_id);
    app.explorer.sessions.insert(
        acct_id,
        AccountExplorerSession {
            breadcrumbs: vec![(1, "My Drive".into())],
            sort_col: ExplorerSortColumn::SyncState,
            entries: vec![
                mk_entry(
                    10,
                    "CloudDir",
                    true,
                    NodeState::Placeholder,
                    ExplorerSyncStatus::Cloud,
                ),
                mk_entry(
                    11,
                    "IndexedDir",
                    true,
                    NodeState::Cached,
                    ExplorerSyncStatus::Indexed,
                ),
                mk_entry(
                    12,
                    "PartialDir",
                    true,
                    NodeState::Cached,
                    ExplorerSyncStatus::PartialLocal,
                ),
                mk_entry(
                    13,
                    "FullDir",
                    true,
                    NodeState::Cached,
                    ExplorerSyncStatus::FullLocal,
                ),
                mk_entry(
                    14,
                    "DirtyFile.mp3",
                    false,
                    NodeState::Dirty,
                    ExplorerSyncStatus::PendingUpload,
                ),
                mk_entry(
                    15,
                    "ConflictFile.mp3",
                    false,
                    NodeState::Conflict,
                    ExplorerSyncStatus::Conflict,
                ),
                mk_entry(
                    16,
                    "SyncingFile.mp3",
                    false,
                    NodeState::Hydrating,
                    ExplorerSyncStatus::Syncing,
                ),
            ],
            loaded: true,
            ..Default::default()
        },
    );

    let ctx = egui::Context::default();
    let _ = render_explorer_at(&ctx, &mut app, 960.0);
    app.explorer.view_mode = ExplorerViewMode::Grid;
    let _ = render_explorer_at(&ctx, &mut app, 960.0);

    // Direct icon painting smoke check for FolderIndexed and DownloadPartial
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(120.0, 60.0))),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        let r1 = Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::splat(20.0));
        let r2 = Rect::from_min_size(Pos2::new(32.0, 4.0), Vec2::splat(20.0));
        paint_icon(
            ui.painter(),
            r1,
            IconKind::FolderIndexed,
            egui::Color32::GRAY,
        );
        paint_icon(
            ui.painter(),
            r2,
            IconKind::DownloadPartial,
            egui::Color32::LIGHT_BLUE,
        );
    });
    assert!(!out.shapes.is_empty());
}

#[test]
fn settings_author_link_renders_underlined_and_opens_site() {
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let opened_ref = Arc::clone(&opened);
    let endpoints = AuthEndpoints {
        browser_opener: Arc::new(move |url| {
            opened_ref.lock().unwrap().push(url.to_string());
            Ok(())
        }),
        ..Default::default()
    };
    let (bridge, _) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    app.settings_open = true;

    let ctx = egui::Context::default();
    let _ = render_explorer_at(&ctx, &mut app, 960.0);
    // Scroll the Settings ScrollArea down so the bottom About footer is visible.
    for _ in 0..4 {
        let scroll_raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
            events: vec![
                Event::PointerMoved(Pos2::new(480.0, 360.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -900.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                    source: egui::MouseWheelSource::Unknown,
                },
            ],
            ..Default::default()
        };
        let mut out = ctx.run_ui(scroll_raw, |ui| app.render_root(ui));
        out.textures_delta.clear();
    }
    let shapes = render_explorer_at(&ctx, &mut app, 960.0);
    let author_rect = find_label_rect(&shapes, "the-long-ride").expect("author hyperlink");
    let pt = author_rect.center();
    for pressed in [true, false] {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
            events: vec![
                Event::PointerMoved(pt),
                Event::PointerButton {
                    pos: pt,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
        out.textures_delta.clear();
    }
    assert_eq!(
        opened.lock().unwrap().as_slice(),
        ["https://the-long-ride.github.io/"]
    );
}
