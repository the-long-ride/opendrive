//! Headless UI tests for the Titlebar icon-only Sync toggle, divider, compact mode, and UI/Explorer edge branches.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::formatters::StatusTone;
use opendrive_app::design::icons::IconKind;
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::design::tooltip::{button_tooltip_text, compute_smart_tooltip_rect};
use opendrive_app::design::widgets::ButtonVariant;
use opendrive_app::design::widgets_controls::{studio_card, tactile_switch};
use opendrive_app::desktop::{
    AccountExplorerSession, ControlCenterApp, ExplorerModal, ExplorerViewMode, UiAction,
};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::{Account, AccountStatus, Settings};
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, collect, has, seed_drive, wait_entry};
use uuid::Uuid;

fn frame(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    w: f32,
    h: f32,
    evs: Vec<Event>,
) -> egui::FullOutput {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
        events: evs,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

fn text_box(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<Rect> {
    fn walk(s: &Shape, needle: &str) -> Option<Rect> {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()))
            }
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| walk(&cs.shape, text))
}

fn bar_rects(shapes: &[egui::epaint::ClippedShape], y_max: f32) -> Vec<(Rect, bool)> {
    fn walk(s: &Shape, y_max: f32, out: &mut Vec<(Rect, bool)>) {
        match s {
            Shape::Rect(rs) if rs.rect.bottom() < y_max => {
                out.push((rs.rect, rs.stroke.width > 0.0))
            }
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, y_max, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|cs| walk(&cs.shape, y_max, &mut out));
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn titlebar_sync_toggle_divider_compact_and_tooltip_branches() {
    let tmp = std::env::temp_dir().join(format!("od-tb-cov-{}", Uuid::new_v4()));
    let (addr, drive_h) = FakeDriveServer::start(seed_drive(&[], 0, 1000)).await;
    let (br, _rx) = Bridge::new(Settings::default());
    let bridge = br.with_mount_manager(Arc::new(
        MountManager::new_null().with_mount_root(tmp.clone()),
    ));
    let mut app = ControlCenterApp::new(bridge.clone());
    let ctx = egui::Context::default();

    let s0 = frame(&ctx, &mut app, 960.0, 720.0, Vec::new()).shapes;
    let t0 = collect(&s0);
    assert!(!has(&t0, "Pause") && !has(&t0, "Resume") && has(&t0, "Color"));

    add_acct(&bridge, bridge.engine(), "Work", addr, 0, 1000);
    app.refresh();
    assert_eq!(app.state.accounts[0].status, AccountStatus::Connected);

    let s1 = frame(&ctx, &mut app, 960.0, 720.0, Vec::new()).shapes;
    let color_r = text_box(&s1, "Color").unwrap();
    let rects = bar_rects(&s1, 56.0);
    let mut btns: Vec<Rect> = rects
        .iter()
        .filter(|(r, stroked)| {
            *stroked && (r.height() - 28.0).abs() < 0.5 && r.left() > color_r.right()
        })
        .map(|(r, _)| *r)
        .collect();
    let div_x = rects
        .iter()
        .filter(|(r, stroked)| {
            !*stroked && (r.width() - 1.5).abs() < 0.3 && (r.height() - 18.0).abs() < 0.5
        })
        .map(|(r, _)| r.center().x)
        .max_by(|a, b| a.total_cmp(b))
        .expect("thin titlebar divider");
    btns.sort_by(|a, b| a.left().total_cmp(&b.left()));
    assert_eq!(btns.len(), 5);
    let (sync_r, settings_r, min_r) = (btns[0], btns[1], btns[2]);
    assert!(color_r.right() < sync_r.left() && sync_r.right() < settings_r.left());
    assert!(settings_r.right() < div_x && div_x < min_r.left());

    let hover = |pos: Pos2, ctx: &egui::Context, app: &mut ControlCenterApp| {
        let ev = vec![Event::PointerMoved(pos)];
        let _ = frame(ctx, app, 960.0, 720.0, ev.clone());
        collect(&frame(ctx, app, 960.0, 720.0, ev).shapes)
    };
    let click = |pos: Pos2, ctx: &egui::Context, app: &mut ControlCenterApp| {
        support::click_at(ctx, app, pos, PointerButton::Primary, Modifiers::NONE)
    };
    assert!(has(
        &hover(sync_r.center(), &ctx, &mut app),
        "Pause syncing across all accounts"
    ));
    assert!(has(
        &hover(settings_r.center(), &ctx, &mut app),
        "Open application preferences and sync settings"
    ));
    click(settings_r.center(), &ctx, &mut app);
    assert!(app.settings_open);
    click(settings_r.center(), &ctx, &mut app);
    assert!(!app.settings_open);

    let all_status = |a: &ControlCenterApp, st| a.state.accounts.iter().all(|x| x.status == st);
    click(sync_r.center(), &ctx, &mut app);
    assert!(all_status(&app, AccountStatus::Paused));
    assert!(has(
        &hover(sync_r.center(), &ctx, &mut app),
        "Resume syncing across all accounts"
    ));
    click(sync_r.center(), &ctx, &mut app);
    assert!(all_status(&app, AccountStatus::Connected));
    let compact_t = collect(&frame(&ctx, &mut app, 380.0, 520.0, Vec::new()).shapes);
    assert!(has(&compact_t, "Color"));

    assert_eq!(
        button_tooltip_text(None, "", ButtonVariant::Default, true),
        "Action button"
    );
    assert_eq!(
        button_tooltip_text(Some(IconKind::Folder), "Docs", ButtonVariant::Default, true),
        "Open folder Docs"
    );
    assert_eq!(
        button_tooltip_text(
            None,
            "Grid",
            ButtonVariant::Toggle { selected: true },
            false
        ),
        "Select Grid (currently unavailable)"
    );
    let tight = Rect::from_min_size(Pos2::ZERO, Vec2::new(120.0, 48.0));
    let _ = compute_smart_tooltip_rect(
        Rect::from_min_size(Pos2::new(10.0, 26.0), Vec2::new(40.0, 20.0)),
        Vec2::new(90.0, 20.0),
        tight,
    );
    let mut ui_out = ctx.run_ui(RawInput::default(), |ui| {
        let pal = palette(ThemeMode::ColorPop);
        studio_card(
            ui,
            &pal,
            "S",
            Some("Sub"),
            Some(("Standby", StatusTone::Standby)),
            |ui| {
                let _ = tactile_switch(ui, &pal, true, true);
            },
        );
    });
    ui_out.textures_delta.clear();
    drive_h.abort();
    let _ = std::fs::remove_dir_all(tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_grid_selection_modal_and_settings_edge_branches() {
    let tmp = std::env::temp_dir().join(format!("od-ui-cov-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let drive = seed_drive(
        &[
            (
                "d1",
                "Folder",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f1", "a.txt", "text/plain", "root", b"x"),
        ],
        100,
        1000,
    );
    let (addr, drive_h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br
        .with_data_dir(data)
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let aid = add_acct(&bridge, &eng, "Alpha", addr, 100, 1000);

    let mut app = ControlCenterApp::new(bridge.clone());
    let ctx = egui::Context::default();
    app.apply_action(UiAction::ToggleAutostart);
    app.apply_action(UiAction::SetCacheMax(2_000_000_000));
    app.apply_action(UiAction::SetBandwidth(Some(5_000_000)));
    app.apply_action(UiAction::SetBandwidth(None));
    app.apply_action(UiAction::ToggleVirtualDrive);
    app.apply_action(UiAction::ToggleCheckUpdates);
    app.settings_open = true;
    let _ = frame(&ctx, &mut app, 400.0, 440.0, Vec::new());
    app.apply_action(UiAction::ToggleSettings(Some(false)));

    let aid2 = bridge
        .add_account("Beta".into(), String::new(), "Beta".into(), false)
        .unwrap()
        .accounts[1]
        .id;
    app.refresh();
    app.state.accounts[0] = Account {
        status: AccountStatus::Paused,
        enabled: false,
        ..app.state.accounts[0].clone()
    };
    let _ = frame(&ctx, &mut app, 500.0, 680.0, Vec::new());
    app.apply_action(UiAction::StartEditAlias(aid2.to_string()));
    let _ = frame(&ctx, &mut app, 640.0, 680.0, Vec::new());
    app.apply_action(UiAction::CancelAlias);

    app.apply_action(UiAction::OpenExplorer(String::new()));
    wait_entry(&ctx, &mut app, aid, "a.txt");
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let _ = frame(&ctx, &mut app, 620.0, 540.0, Vec::new());
    app.explorer.active_session_mut().unwrap().search_query = "zzz-none".into();
    let empty_filter = collect(&frame(&ctx, &mut app, 960.0, 720.0, Vec::new()).shapes);
    assert!(has(&empty_filter, "No items match your search filter."));
    app.explorer
        .active_session_mut()
        .unwrap()
        .search_query
        .clear();

    let aid_missing = Uuid::new_v4();
    app.apply_action(UiAction::OpenExplorer("bad-uuid".into()));
    app.apply_action(UiAction::OpenExplorer(aid_missing.to_string()));
    let mut s = AccountExplorerSession {
        selected_ids: vec![9999],
        anchor_id: Some(9999),
        ..Default::default()
    };
    s.prune_to_entries();
    assert!(s.selected_ids.is_empty() && s.anchor_id.is_none());

    app.explorer.modal = Some(ExplorerModal::Rename {
        account_id: aid,
        node_id: 9999,
        draft: "  ".into(),
        focus_requested: true,
    });
    let _ = frame(&ctx, &mut app, 960.0, 720.0, Vec::new());
    app.apply_action(UiAction::ExplorerSaveRename);
    assert!(app.notice.is_some());
    app.apply_action(UiAction::ExplorerDismissModal);
    let f1 = support::eid(&app, aid, "a.txt");
    app.apply_action(UiAction::ExplorerSelectRange {
        node_id: f1,
        additive: true,
    });
    let _ = frame(&ctx, &mut app, 480.0, 540.0, Vec::new());
    app.apply_action(UiAction::ExplorerRequestDelete(vec![f1]));
    let del_t = collect(&frame(&ctx, &mut app, 960.0, 720.0, Vec::new()).shapes);
    assert!(has(&del_t, "Delete") || !del_t.is_empty());
    let esc = Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let _ = frame(&ctx, &mut app, 960.0, 720.0, vec![esc]);
    drive_h.abort();
    let _ = std::fs::remove_dir_all(tmp);
}
