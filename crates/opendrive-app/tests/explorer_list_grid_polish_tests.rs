//! Headless GUI tests for List checkboxes, column sorting, click-outside clear, Grid metadata, and compact sidebar card.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Modifiers, PointerButton, Pos2, Shape};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::{ControlCenterApp, ExplorerSortColumn, ExplorerViewMode, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::Settings;
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, click_at, eid, has, seed_drive, send_frame, text_pos, texts, wait_entry};
use uuid::Uuid;

fn all_text_pos(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Vec<Pos2> {
    fn walk(s: &Shape, needle: &str, out: &mut Vec<Pos2>) {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                out.push(ts.galley.rect.translate(ts.pos.to_vec2()).center())
            }
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, needle, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|cs| walk(&cs.shape, needle, &mut out));
    out
}

fn text_left(shapes: &[egui::epaint::ClippedShape], needle: &str) -> f32 {
    fn walk(s: &Shape, needle: &str) -> Option<f32> {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()).left())
            }
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes
        .iter()
        .find_map(|cs| walk(&cs.shape, needle))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_list_grid_polish_behaviors_work() {
    let tmp = std::env::temp_dir().join(format!("od-exp-polish-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();

    let drive = seed_drive(
        &[
            (
                "d-dst",
                "Target",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f-a", "alpha.txt", "text/plain", "root", b"alpha-v1"),
            ("f-b", "beta.txt", "text/plain", "root", b"beta-longer-v1"),
            (
                "f-g",
                "gamma.txt",
                "text/plain",
                "root",
                b"gamma-longest-file-v1",
            ),
        ],
        1024,
        4096,
    );
    let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br
        .with_data_dir(data)
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 1024, 4096);

    let mut app = ControlCenterApp::new(bridge);
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "gamma.txt");

    let (a_nid, b_nid) = (eid(&app, a_id, "alpha.txt"), eid(&app, a_id, "beta.txt"));
    let shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    let (l, none) = (PointerButton::Primary, Modifiers::NONE);
    let pos_a = text_pos(&shapes, "alpha.txt");

    // 1. List row & header checkboxes toggle multi-selection; clicking outside clears it.
    let cb_x = text_left(&shapes, "alpha.txt") - 44.0;
    let click_cb = |a: &mut ControlCenterApp, label: &str| {
        let sh = send_frame(&ctx, a, Vec::new()).shapes;
        click_at(&ctx, a, Pos2::new(cb_x, text_pos(&sh, label).y), l, none);
    };
    click_cb(&mut app, "alpha.txt");
    click_cb(&mut app, "beta.txt");
    assert_eq!(
        app.explorer.sessions[&a_id].selected_ids,
        vec![a_nid, b_nid]
    );
    click_cb(&mut app, "Name");
    assert!(app.explorer.sessions[&a_id].selected_ids.len() >= 4);
    click_cb(&mut app, "Name");
    assert!(app.explorer.sessions[&a_id].selected_ids.is_empty());
    click_cb(&mut app, "alpha.txt");
    assert!(!app.explorer.sessions[&a_id].selected_ids.is_empty());
    click_at(&ctx, &mut app, Pos2::new(pos_a.x, 540.0), l, none);
    assert!(app.explorer.sessions[&a_id].selected_ids.is_empty());

    // 2. Column headers toggle sort column and direction cleanly.
    let click_hdr = |a: &mut ControlCenterApp, col: &str| {
        let p = text_pos(&send_frame(&ctx, a, Vec::new()).shapes, col);
        click_at(&ctx, a, p, l, none);
    };
    click_hdr(&mut app, "Size");
    assert_eq!(
        (
            app.explorer.sessions[&a_id].sort_col,
            app.explorer.sessions[&a_id].sort_desc
        ),
        (ExplorerSortColumn::Size, false)
    );
    click_hdr(&mut app, "Size");
    let desc_shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    assert!(app.explorer.sessions[&a_id].sort_desc);
    assert!(text_pos(&desc_shapes, "gamma.txt").y < text_pos(&desc_shapes, "beta.txt").y);
    assert!(text_pos(&desc_shapes, "beta.txt").y < text_pos(&desc_shapes, "alpha.txt").y);
    click_hdr(&mut app, "Sync State");
    assert_eq!(
        app.explorer.sessions[&a_id].sort_col,
        ExplorerSortColumn::SyncState
    );
    click_hdr(&mut app, "Modified");
    assert_eq!(
        app.explorer.sessions[&a_id].sort_col,
        ExplorerSortColumn::Modified
    );
    click_hdr(&mut app, "Name");
    assert_eq!(
        (
            app.explorer.sessions[&a_id].sort_col,
            app.explorer.sessions[&a_id].sort_desc
        ),
        (ExplorerSortColumn::Name, false)
    );

    // 3. Compact sidebar card height <= 74px and Grid cards render both modified date and size.
    let alpha_pt = text_pos(&shapes, "Alpha");
    let card_r = shapes
        .iter()
        .rev()
        .find_map(|cs| match &cs.shape {
            Shape::Rect(rs)
                if rs.rect.contains(alpha_pt)
                    && rs.rect.width() < 260.0
                    && rs.rect.height() > 35.0 =>
            {
                Some(rs.rect)
            }
            _ => None,
        })
        .expect("sidebar card rect");
    assert!(
        card_r.height() <= 74.0,
        "sidebar card too tall: {}",
        card_r.height()
    );

    if let Some(e) = app
        .explorer
        .sessions
        .get_mut(&a_id)
        .and_then(|s| s.entries.iter_mut().find(|e| e.id == a_nid))
    {
        e.modified_ms = 1_795_294_920_000;
    }
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let grid_t = texts(&ctx, &mut app);
    assert!(has(&grid_t, "8 B") && has(&grid_t, "2026-11-21 21:02"));

    h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dashboard_explore_reorder_light_canvas_and_toast_shadow_work() {
    let tmp = std::env::temp_dir().join(format!("od-dash-polish-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();

    let pal = palette(ThemeMode::ColorPop);
    assert!(
        pal.canvas_bg.r() > 190 && pal.canvas_bg.g() > 190 && pal.canvas_bg.b() > pal.canvas_bg.r()
    );

    let store = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _rx) =
        Bridge::new_with_engine(Settings::default(), Arc::clone(&store), Some(cfg.clone()));
    let bridge = br.with_data_dir(data).with_mount_manager(Arc::new(
        MountManager::new_null().with_mount_root(mnt.clone()),
    ));
    let ctx = egui::Context::default();
    let mut app_empty = ControlCenterApp::new(bridge.clone());
    app_empty.apply_action(UiAction::OpenExplorer(String::new()));
    assert!(
        app_empty.explorer.open
            && app_empty.explorer.active_account_id.is_none()
            && app_empty.notice.is_none()
    );
    app_empty.apply_action(UiAction::CloseExplorer);
    app_empty.apply_action(UiAction::AccountAction {
        action: "move_down",
        id: Some("bad-id".into()),
    });
    let _ = send_frame(&ctx, &mut app_empty, Vec::new());
    let t_sh = send_frame(&ctx, &mut app_empty, Vec::new()).shapes;
    let mut rects = Vec::new();
    fn walk_r(s: &Shape, c: egui::Rect, o: &mut Vec<(egui::epaint::RectShape, egui::Rect)>) {
        match s {
            Shape::Rect(rs) => o.push((rs.clone(), c)),
            Shape::Vec(v) => v.iter().for_each(|x| walk_r(x, c, o)),
            _ => {}
        }
    }
    t_sh.iter()
        .for_each(|cs| walk_r(&cs.shape, cs.clip_rect, &mut rects));
    let msg_p = text_pos(&t_sh, "invalid account id");
    let tr = rects
        .iter()
        .find_map(|(r, _)| (r.stroke.width >= 1.9 && r.rect.contains(msg_p)).then_some(r.rect))
        .expect("toast frame");
    assert!(rects.iter().any(|(r, c)| {
        r.stroke.width == 0.0
            && r.fill.a() > 0
            && (r.rect.min - tr.min - egui::Vec2::splat(3.5)).length() < 0.5
            && (r.rect.size() - tr.size()).length() < 0.5
            && c.contains_rect(r.rect)
    }));
    let add = |n: &str, e: &str| {
        bridge
            .add_account(n.into(), e.into(), String::new(), false)
            .unwrap()
            .accounts
            .pop()
            .unwrap()
            .id
    };
    let (a_id, b_id) = (add("Alpha", "a@x.com"), add("Beta", "b@x.com"));
    let set_q = |id, u, q| {
        let ctrl = bridge.controller_handle();
        let mut g = ctrl.lock().unwrap();
        let a = g.accounts.get_account_mut(id).unwrap();
        (a.storage_used, a.storage_quota) = (u, q);
    };
    set_q(a_id, 512, 2048);
    set_q(b_id, 1024, 4096);

    let mut app = ControlCenterApp::new(bridge.clone());
    let sh = send_frame(&ctx, &mut app, Vec::new()).shapes;
    let op_p = text_pos(&sh, "Open");
    assert!(all_text_pos(&sh, "Explore")
        .iter()
        .any(|ex| ex.x < op_p.x && (ex.y - op_p.y).abs() < 2.0));
    let alpha_path =
        opendrive_app::design::formatters::join_mount_path(&mnt.display().to_string(), "Alpha");
    let (pa_p, us_p) = (
        text_pos(&sh, &alpha_path),
        text_pos(&sh, "512 B / 2.0 KB (25%)"),
    );
    assert!(pa_p.x < us_p.x && (pa_p.y - us_p.y).abs() < 2.0);

    app.apply_action(UiAction::AccountAction {
        action: "move_down",
        id: Some(a_id.to_string()),
    });
    assert_eq!(
        app.state.accounts.iter().map(|a| a.id).collect::<Vec<_>>(),
        vec![b_id, a_id]
    );
    assert_eq!(opendrive_core::load_accounts(&cfg).unwrap()[0].id, b_id);

    app.apply_action(UiAction::OpenExplorer(String::new()));
    assert_eq!(app.explorer.active_account_id, Some(b_id));
    let sb = send_frame(&ctx, &mut app, Vec::new()).shapes;
    assert!(text_pos(&sb, "Beta").y < text_pos(&sb, "Alpha").y);

    let _ = std::fs::remove_dir_all(&tmp);
}
