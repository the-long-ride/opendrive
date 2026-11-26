//! Toolbar geometry alignment and CLI fallback dispatch tests for Cloud Explorer.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Modifiers, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::{dispatch, dispatch_bridge, status_cmd};
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::Settings;
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, seed_drive, texts, wait_entry};
use uuid::Uuid;

fn run_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    w: f32,
    events: Vec<Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, 720.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out.shapes
}

fn render_shapes_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    w: f32,
) -> Vec<egui::epaint::ClippedShape> {
    let _ = run_at(ctx, app, w, Vec::new());
    run_at(ctx, app, w, Vec::new())
}

fn collect_band_rects(shapes: &[egui::epaint::ClippedShape], y_max: f32) -> Vec<Rect> {
    fn visit(s: &Shape, y_max: f32, out: &mut Vec<Rect>) {
        match s {
            Shape::Rect(rs)
                if rs.rect.left() > 280.0
                    && rs.rect.top() > 50.0
                    && rs.rect.top() < y_max
                    && (rs.rect.height() - 28.0).abs() < 0.5
                    && rs.stroke.width > 0.0 =>
            {
                out.push(rs.rect);
            }
            Shape::Vec(v) => v.iter().for_each(|x| visit(x, y_max, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|cs| visit(&cs.shape, y_max, &mut out));
    out
}

fn find_last_text_center(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
    fn visit(s: &Shape, needle: &str) -> Option<Pos2> {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()).center())
            }
            Shape::Vec(v) => v.iter().rev().find_map(|x| visit(x, needle)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| visit(&cs.shape, needle))
}

fn find_box(shapes: &[egui::epaint::ClippedShape], pt: Pos2, target_h: f32) -> Option<Rect> {
    fn visit(s: &Shape, pt: Pos2, target_h: f32) -> Option<Rect> {
        match s {
            Shape::Rect(rs)
                if (rs.rect.height() - target_h).abs() < 1.5
                    && rs.stroke.width > 0.0
                    && rs.rect.contains(pt) =>
            {
                Some(rs.rect)
            }
            Shape::Vec(v) => v.iter().rev().find_map(|x| visit(x, pt, target_h)),
            _ => None,
        }
    }
    shapes
        .iter()
        .rev()
        .find_map(|cs| visit(&cs.shape, pt, target_h))
}

fn group_rows(mut rects: Vec<Rect>) -> Vec<Vec<Rect>> {
    rects.sort_by(|a, b| {
        a.top()
            .total_cmp(&b.top())
            .then(a.left().total_cmp(&b.left()))
    });
    let mut rows: Vec<Vec<Rect>> = Vec::new();
    for r in rects {
        if let Some(last) = rows
            .last_mut()
            .filter(|g| (r.top() - g[0].top()).abs() < 2.0)
        {
            last.push(r);
        } else {
            rows.push(vec![r]);
        }
    }
    rows
}

fn assert_row_aligned(rects: &mut [Rect]) {
    rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
    let (t0, b0) = (rects[0].top(), rects[0].bottom());
    for r in rects.iter() {
        assert!((r.top() - t0).abs() < 0.5 && (r.bottom() - b0).abs() < 0.5);
    }
    for w in rects.windows(2) {
        assert!(w[1].left() - w[0].right() >= 6.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_toolbar_rows_align_and_cli_fallbacks_work() {
    let tmp = std::env::temp_dir().join(format!("od-exp-lay-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let drive = seed_drive(
        &[("f-vid", "demo.mp4", "video/mp4", "root", &[7u8; 256])],
        1024,
        4096,
    );
    let (addr, h_a) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br
        .with_data_dir(data)
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 1024, 4096);

    let mut app = ControlCenterApp::new(bridge.clone());
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "demo.mp4");

    let toolbar_rows = |shapes| group_rows(collect_band_rects(shapes, 190.0));
    let wide_shapes = render_shapes_at(&ctx, &mut app, 1600.0);
    let y_wide_0 = find_last_text_center(&wide_shapes, "demo.mp4").unwrap().y;
    let wide_t = support::collect(&wide_shapes);
    assert!(!support::has_sub(&wide_t, "selected"));
    let mut wide = toolbar_rows(&wide_shapes);
    assert_eq!((wide.len(), wide[0].len(), wide[1].len()), (2, 1, 9));
    assert!(wide[1][0].top() - wide[0][0].bottom() >= 8.0);
    for row in &mut wide {
        assert_row_aligned(row);
    }
    let up_pt = find_last_text_center(&wide_shapes, "Upload").unwrap();
    assert!(wide[1][7].contains(up_pt) && wide[1][7].right() < wide[1][8].left());
    let demo_id = support::eid(&app, a_id, "demo.mp4");
    app.apply_action(UiAction::ExplorerSelectEntry(Some(demo_id)));
    let sel_shapes = render_shapes_at(&ctx, &mut app, 1600.0);
    let y_wide_1 = find_last_text_center(&sel_shapes, "demo.mp4").unwrap().y;
    assert!((y_wide_0 - y_wide_1).abs() < 0.5);
    let sel_t = support::collect(&sel_shapes);
    assert!(["1 selected", "Open", "Copy Stream URL"]
        .iter()
        .all(|k| support::has(&sel_t, k)));
    let click_w = |app: &mut ControlCenterApp, w: f32, pos: Pos2, button| {
        for pressed in [true, false] {
            let btn = Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let _ = run_at(&ctx, app, w, vec![Event::PointerMoved(pos), btn]);
        }
    };
    let sync_pt = find_last_text_center(&sel_shapes, "Sync").unwrap();
    click_w(&mut app, 1600.0, sync_pt, egui::PointerButton::Primary);
    let sync_menu = support::collect(&render_shapes_at(&ctx, &mut app, 1600.0));
    assert!(support::has(&sync_menu, "Sync Down") && support::has(&sync_menu, "Sync Up"));
    app.apply_action(UiAction::ExplorerSelectEntry(None));
    let sh0 = render_shapes_at(&ctx, &mut app, 1280.0);
    let y0 = find_last_text_center(&sh0, "demo.mp4").unwrap().y;
    app.apply_action(UiAction::ExplorerSelectEntry(Some(demo_id)));
    let sh1 = render_shapes_at(&ctx, &mut app, 1280.0);
    assert!((y0 - find_last_text_center(&sh1, "demo.mp4").unwrap().y).abs() < 0.5);
    app.apply_action(UiAction::ExplorerSelectEntry(None));

    let _ = run_at(&ctx, &mut app, 960.0, vec![Event::PointerMoved(Pos2::ZERO)]);
    let def_shapes = render_shapes_at(&ctx, &mut app, 960.0);
    let dash_r = find_last_text_center(&def_shapes, "Dashboard").unwrap();
    let new_r = find_last_text_center(&def_shapes, "New Account").unwrap();
    let hdr_r = find_last_text_center(&def_shapes, "Drive Accounts").unwrap();
    assert!(!support::has(
        &support::collect(&def_shapes),
        "Connect Account"
    ));
    assert!((dash_r.y - new_r.y).abs() < 2.0 && dash_r.x < new_r.x && new_r.y < hdr_r.y);

    let mut def_rows = toolbar_rows(&def_shapes);
    let def_lens: Vec<_> = def_rows.iter().map(Vec::len).collect();
    assert_eq!(def_lens, [1, 5, 4]);
    for w in def_rows.windows(2) {
        assert!(w[1][0].top() - w[0][0].bottom() >= 6.0);
    }
    for row in &mut def_rows {
        assert_row_aligned(row);
    }
    assert!(
        (def_rows[1][0].left() - def_rows[0][0].left()).abs() < 1.0
            && (def_rows[1][0].left() - def_rows[2][0].left()).abs() < 1.0
    );

    // Long nested breadcrumbs never overlap Search box or right controls across widths.
    let saved_crumbs = app.explorer.sessions[&a_id].breadcrumbs.clone();
    app.explorer.sessions.get_mut(&a_id).unwrap().breadcrumbs = vec![
        (0, "My Drive".into()),
        (1, "Nghe Tiểu Thuyết Mỗi Ngày".into()),
        (2, "003-Từ Phế Linh Căn Bắt Đầu Cho Thuê Đạo Quân".into()),
    ];
    for w in [960.0, 760.0] {
        let nested_shapes = render_shapes_at(&ctx, &mut app, w);
        let rects = collect_band_rects(&nested_shapes, 240.0);
        assert!(
            rects.len() >= 9,
            "width {w}: expected breadcrumbs + controls"
        );
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!a.intersects(*b), "overlap at {w}px: {a:?} vs {b:?}");
            }
        }
    }
    app.explorer.sessions.get_mut(&a_id).unwrap().breadcrumbs = saved_crumbs;

    // Context-menu items span uniform full menu panel width.
    let base = render_shapes_at(&ctx, &mut app, 960.0);
    let demo_pos = find_last_text_center(&base, "demo.mp4").expect("demo.mp4 row");
    click_w(&mut app, 960.0, demo_pos, egui::PointerButton::Secondary);
    let menu_shapes = render_shapes_at(&ctx, &mut app, 960.0);
    let items = [
        "Open",
        "Sync Down (Pin Local)",
        "Rename",
        "Delete",
        "Copy Stream URL",
    ];
    let menu_boxes: Vec<Rect> = items
        .iter()
        .map(|lbl| {
            let pt = find_last_text_center(&menu_shapes, lbl).expect(lbl);
            find_box(&menu_shapes, pt, 28.0).expect(lbl)
        })
        .collect();
    let w0 = menu_boxes[0].width();
    let x0 = menu_boxes[0].left();
    assert!((188.0..=250.0).contains(&w0), "context menu too wide: {w0}");
    for b in &menu_boxes {
        assert!((b.width() - w0).abs() < 0.5 && (b.left() - x0).abs() < 0.5);
    }

    // Rename modal single-line input text is vertically centered inside the 34px box.
    let demo_id = support::eid(&app, a_id, "demo.mp4");
    app.apply_action(UiAction::ExplorerStartRename(demo_id));
    if let Some(opendrive_app::desktop::ExplorerModal::Rename { draft, .. }) =
        app.explorer.modal.as_mut()
    {
        *draft = "Google-ultra.zip".into();
    }
    let modal_shapes = render_shapes_at(&ctx, &mut app, 960.0);
    let txt_pos = find_last_text_center(&modal_shapes, "Google-ultra.zip").expect("rename text");
    let input_rect = find_box(&modal_shapes, txt_pos, 34.0).expect("rename input rect");
    assert!(
        (txt_pos.y - input_rect.center().y).abs() <= 1.5,
        "input text not vertically centered: {txt_pos:?} in {input_rect:?}"
    );
    app.apply_action(UiAction::ExplorerDismissModal);

    assert!(status_cmd().is_ok());
    assert!(dispatch("unknown-cmd", std::iter::empty()).is_ok());
    let bad = ["missing-acct", "local"].map(String::from);
    assert!(dispatch_bridge("sync-up", bad.into_iter(), &bridge).is_err());

    h_a.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
