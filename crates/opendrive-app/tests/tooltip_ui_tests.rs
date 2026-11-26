//! Unit geometry and headless egui integration tests for viewport-aware smart tooltips.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Modifiers, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::formatters::StatusTone;
use opendrive_app::design::icons::{paint_icon, IconKind};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::design::tooltip::{
    compute_smart_tooltip_rect, status_badge_spec, TooltipPlacement, TOOLTIP_GAP, TOOLTIP_MARGIN,
    TOOLTIP_SHADOW,
};
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction, UiNotice};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::Settings;
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, has, seed_drive, texts, wait_entry};
use uuid::Uuid;

fn frame_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    events: Vec<Event>,
) -> egui::FullOutput {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

fn hover_pos(ctx: &egui::Context, app: &mut ControlCenterApp, pos: Pos2) -> egui::FullOutput {
    let _ = frame_at(ctx, app, vec![Event::PointerMoved(pos)]);
    frame_at(ctx, app, vec![Event::PointerMoved(pos)])
}

fn find_text_rect(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Rect> {
    fn walk(s: &Shape, needle: &str) -> Option<Rect> {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()))
            }
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| walk(&cs.shape, needle))
}

fn find_enclosing_box(shapes: &[egui::epaint::ClippedShape], pt: Pos2) -> Option<Rect> {
    fn walk(s: &Shape, pt: Pos2) -> Option<Rect> {
        match s {
            Shape::Rect(rs) if rs.rect.contains(pt) && rs.stroke.width > 0.0 => Some(rs.rect),
            Shape::Vec(v) => v.iter().rev().find_map(|x| walk(x, pt)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| walk(&cs.shape, pt))
}

fn find_sized_rects(
    shapes: &[egui::epaint::ClippedShape],
    w: Option<f32>,
    h: f32,
    y_max: f32,
) -> Vec<Rect> {
    fn walk(s: &Shape, w: Option<f32>, h: f32, y_max: f32, out: &mut Vec<Rect>) {
        match s {
            Shape::Rect(rs)
                if rs.rect.top() < y_max
                    && w.is_none_or(|ew| (rs.rect.width() - ew).abs() < 1.0)
                    && (rs.rect.height() - h).abs() < 1.0
                    && rs.stroke.width > 0.0 =>
            {
                out.push(rs.rect);
            }
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, w, h, y_max, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|cs| walk(&cs.shape, w, h, y_max, &mut out));
    out
}

fn assert_tooltip_fully_visible(tip_box: Rect, vp: Rect) {
    assert!(tip_box.left() >= vp.left() + TOOLTIP_MARGIN - 0.5);
    assert!(tip_box.right() + TOOLTIP_SHADOW <= vp.right() - TOOLTIP_MARGIN + 0.5);
    assert!(tip_box.top() >= vp.top() + TOOLTIP_MARGIN - 0.5);
    assert!(tip_box.bottom() + TOOLTIP_SHADOW <= vp.bottom() - TOOLTIP_MARGIN + 0.5);
}

fn assert_hover_tip(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    pos: Pos2,
    expected: &str,
    vp: Rect,
) -> Rect {
    let out = hover_pos(ctx, app, pos);
    let tip_r =
        find_text_rect(&out.shapes, expected).unwrap_or_else(|| panic!("missing {expected}"));
    let tip_box = find_enclosing_box(&out.shapes, tip_r.center()).expect("tooltip box");
    assert_tooltip_fully_visible(tip_box, vp);
    tip_box
}

#[test]
fn compute_smart_tooltip_rect_handles_all_viewport_edges() {
    let vp = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
    let tip = Vec2::new(200.0, 32.0);

    // Normal centered button -> below target.
    let mid = Rect::from_min_size(Pos2::new(350.0, 200.0), Vec2::new(80.0, 28.0));
    let (r, place) = compute_smart_tooltip_rect(mid, tip, vp);
    assert_eq!(place, TooltipPlacement::Bottom);
    assert!((r.top() - (mid.bottom() + TOOLTIP_GAP)).abs() < 0.01);
    assert!((r.center().x - mid.center().x).abs() < 0.01);

    // Near bottom -> flips to Top.
    let low = Rect::from_min_size(Pos2::new(350.0, 565.0), Vec2::new(80.0, 28.0));
    let (r, place) = compute_smart_tooltip_rect(low, tip, vp);
    assert_eq!(place, TooltipPlacement::Top);
    assert!((r.bottom() - (low.top() - TOOLTIP_GAP)).abs() < 0.01);

    // Near left edge -> clamped inward so left >= margin.
    let left = Rect::from_min_size(Pos2::new(2.0, 100.0), Vec2::new(28.0, 28.0));
    let (r, place) = compute_smart_tooltip_rect(left, tip, vp);
    assert_eq!(place, TooltipPlacement::Bottom);
    assert_tooltip_fully_visible(r, vp);

    // Bottom-right corner -> flips Top AND shifts left so tooltip + shadow fit 100%.
    let corner = Rect::from_min_size(Pos2::new(760.0, 566.0), Vec2::new(32.0, 28.0));
    let (r, place) = compute_smart_tooltip_rect(corner, tip, vp);
    assert_eq!(place, TooltipPlacement::Top);
    assert_tooltip_fully_visible(r, vp);

    for (lbl, tone, expected_icon) in [
        ("This PC Active", StatusTone::Connected, IconKind::Monitor),
        ("Windows", StatusTone::Connected, IconKind::Monitor),
        ("Sync Active", StatusTone::Connected, IconKind::Refresh),
        ("All Paused", StatusTone::Paused, IconKind::Pause),
        ("Attention", StatusTone::Warn, IconKind::Alert),
        ("Standby", StatusTone::Standby, IconKind::Cloud),
        ("Links (.gdoc)", StatusTone::Standby, IconKind::Link),
        ("Office Export", StatusTone::Standby, IconKind::DocSheet),
        ("Connected", StatusTone::Connected, IconKind::Check),
        ("Paused", StatusTone::Paused, IconKind::Pause),
        ("Needs Reauth", StatusTone::Warn, IconKind::Key),
        ("Error", StatusTone::Error, IconKind::Alert),
        ("Syncing...", StatusTone::Standby, IconKind::Refresh),
        ("Custom", StatusTone::Standby, IconKind::Cloud),
    ] {
        let (icon, tip_str) = status_badge_spec(lbl, tone);
        assert_eq!(icon, expected_icon);
        assert!(!tip_str.is_empty());
    }
    let ctx = egui::Context::default();
    let pal = palette(ThemeMode::DarkStudio);
    let mut icon_out = ctx.run_ui(RawInput::default(), |ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
        paint_icon(ui.painter(), r, IconKind::Monitor, pal.text_primary);
        paint_icon(ui.painter(), r, IconKind::Link, pal.text_primary);
    });
    icon_out.textures_delta.clear();
    assert!(!icon_out.shapes.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smart_tooltips_show_on_buttons_switches_and_context_menus() {
    let vp = Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0));
    let tmp = std::env::temp_dir().join(format!("od-tip-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let drive = seed_drive(
        &[("f-alpha", "alpha.txt", "text/plain", "root", b"hello")],
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

    // 1. Top-right Close icon button -> Bottom placement shifted left inside viewport.
    let base = frame_at(&ctx, &mut app, Vec::new());
    let mut top_btns = find_sized_rects(&base.shapes, None, 28.0, 65.0);
    top_btns.sort_by(|a, b| a.right().total_cmp(&b.right()));
    let close_rect = *top_btns.last().expect("titlebar close button");
    let tip_box = assert_hover_tip(&ctx, &mut app, close_rect.center(), "Close window", vp);
    assert!(tip_box.top() >= close_rect.bottom() + 4.0);

    // 2. Account Sync On switch -> switch tooltip visible below switch.
    let sync_lbl = find_text_rect(&base.shapes, "Sync On").expect("Sync On label");
    let sw = Pos2::new(sync_lbl.left() - 28.0, sync_lbl.center().y);
    assert_hover_tip(
        &ctx,
        &mut app,
        sw,
        "Sync active for this account (click to pause)",
        vp,
    );

    // 2b. Dashboard & Account icon-only status badges show tooltips on hover.
    for badge in find_sized_rects(&base.shapes, Some(28.0), 24.0, 720.0) {
        let all = support::collect(&hover_pos(&ctx, &mut app, badge.center()).shapes);
        assert!(all.iter().any(|t| {
            t.contains("Mounted in Windows Explorer")
                || t.contains("Local OpenDrive mount")
                || t.contains("Google Docs, Sheets & Slides")
                || t.contains("Background Google Drive sync")
                || t.contains("Account is authenticated")
        }));
    }

    // 3. Bottom-right error toast Retry / Close icon button -> flips to Top & stays in viewport.
    app.notice = Some(UiNotice {
        tone: NoticeTone::Error,
        message: "Sync failed for Alpha".into(),
    });
    app.notice_expires_at = None;
    let _ = frame_at(&ctx, &mut app, Vec::new());
    let toast_out = frame_at(&ctx, &mut app, Vec::new());
    let retry_pos = find_text_rect(&toast_out.shapes, "Retry")
        .expect("Retry button")
        .center();
    let retry_box = assert_hover_tip(&ctx, &mut app, retry_pos, "Retry the failed operation", vp);
    assert!(retry_box.bottom() <= retry_pos.y);

    let mut toast_btns: Vec<Rect> = find_sized_rects(&toast_out.shapes, None, 28.0, 720.0)
        .into_iter()
        .filter(|r| r.top() > 500.0)
        .collect();
    toast_btns.sort_by(|a, b| a.right().total_cmp(&b.right()));
    let toast_close = *toast_btns.last().expect("toast close button");
    let tip_box = assert_hover_tip(&ctx, &mut app, toast_close.center(), "Close", vp);
    assert!(tip_box.bottom() <= toast_close.top());
    app.apply_action(UiAction::DismissNotice);

    // 4. Explorer icon-only Cloud badge + right-click context menu item tooltip.
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "alpha.txt");
    let exp_out = frame_at(&ctx, &mut app, Vec::new());
    assert!(!has(&support::collect(&exp_out.shapes), "Cloud"));
    let entry_pos = find_text_rect(&exp_out.shapes, "alpha.txt")
        .expect("alpha.txt row")
        .center();
    let row_badge = find_sized_rects(&exp_out.shapes, Some(28.0), 24.0, 720.0)
        .into_iter()
        .find(|b| (b.center().y - entry_pos.y).abs() < 12.0 && b.left() > 300.0)
        .expect("row sync icon badge");
    assert_hover_tip(
        &ctx,
        &mut app,
        row_badge.center(),
        "Cloud — Streamed on demand (not stored locally)",
        vp,
    );

    for pressed in [true, false] {
        let ev = Event::PointerButton {
            pos: entry_pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let _ = frame_at(&ctx, &mut app, vec![Event::PointerMoved(entry_pos), ev]);
    }
    let menu_out = frame_at(&ctx, &mut app, Vec::new());
    let del_pos = find_text_rect(&menu_out.shapes, "Delete")
        .expect("Delete menu item")
        .center();
    assert_hover_tip(
        &ctx,
        &mut app,
        del_pos,
        "Delete this file or folder from cloud and local",
        vp,
    );

    h_a.abort();
    let _ = std::fs::remove_dir_all(tmp);
}
