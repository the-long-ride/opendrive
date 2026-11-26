//! Explorer z-index/scroll clipping, non-text cursor, badges, and UI/CLI file op tests.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, CursorIcon, Event, Modifiers, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::dispatch_bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::Settings;
use opendrive_fake_drive::{FakeDriveServer, FakeFile};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, eid, has, seed_drive, texts, wait_entry, wait_until};
use uuid::Uuid;

fn find_text(
    shapes: &[egui::epaint::ClippedShape],
    pat: &str,
    exact: bool,
) -> Option<(String, Pos2, Rect)> {
    fn walk(s: &Shape, clip: Rect, pat: &str, exact: bool) -> Option<(String, Pos2, Rect)> {
        match s {
            Shape::Text(ts)
                if (exact && ts.galley.text() == pat)
                    || (!exact && ts.galley.text().starts_with(pat)) =>
            {
                let p = ts.galley.rect.translate(ts.pos.to_vec2()).center();
                clip.contains(p)
                    .then(|| (ts.galley.text().to_owned(), p, clip))
            }
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, clip, pat, exact)),
            _ => None,
        }
    }
    shapes
        .iter()
        .find_map(|cs| walk(&cs.shape, cs.clip_rect, pat, exact))
}

fn find_unclipped_bleed(
    shapes: &[egui::epaint::ClippedShape],
    prefix: &str,
    min_top: f32,
) -> Vec<String> {
    fn walk(s: &Shape, clip: Rect, prefix: &str, min_top: f32, bad: &mut Vec<String>) {
        match s {
            Shape::Text(ts)
                if ts.galley.text().starts_with(prefix) && clip.top() < min_top - 1.0 =>
            {
                bad.push(format!("{}: clip_top={:.1}", ts.galley.text(), clip.top()));
            }
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, clip, prefix, min_top, bad)),
            _ => {}
        }
    }
    let mut bad = Vec::new();
    shapes
        .iter()
        .for_each(|cs| walk(&cs.shape, cs.clip_rect, prefix, min_top, &mut bad));
    bad
}

fn run_scroll_and_hover(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    pos: Option<Pos2>,
    wheel_y: f32,
) -> egui::FullOutput {
    let mut events: Vec<Event> = pos.into_iter().map(Event::PointerMoved).collect();
    if wheel_y != 0.0 {
        events.push(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, wheel_y),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        });
    }
    send_evs(ctx, app, events)
}

fn send_evs(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    events: Vec<Event>,
) -> egui::FullOutput {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 420.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_clipping_cursor_and_ui_cli_file_ops_work() {
    let tmp = std::env::temp_dir().join(format!("od-exp-fops-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();

    let fdir = "application/vnd.google-apps.folder";
    let drive = seed_drive(&[("d-dest", "Target", fdir, "root", &[])], 1024, 4096);
    for i in 0..24 {
        drive.add_file(FakeFile {
            id: format!("f-{i:02}"),
            name: format!("item-{i:02}.txt"),
            mime_type: "text/plain".into(),
            size: 4,
            parents: vec!["root".into()],
            content: b"data".into(),
            ..Default::default()
        });
    }
    let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
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
    wait_entry(&ctx, &mut app, a_id, "item-00.txt");

    // 1. Scroll clipping regression test in List and Grid views.
    let init = run_scroll_and_hover(&ctx, &mut app, Some(Pos2::new(560.0, 280.0)), 0.0);
    let hdr_y = find_text(&init.shapes, "Sync State", true).unwrap().1.y;
    for _ in 0..3 {
        let _ = run_scroll_and_hover(&ctx, &mut app, Some(Pos2::new(560.0, 280.0)), -180.0);
    }
    let scrolled = run_scroll_and_hover(&ctx, &mut app, Some(Pos2::new(560.0, 280.0)), 0.0);
    assert!(find_unclipped_bleed(&scrolled.shapes, "item-", hdr_y).is_empty());

    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    for _ in 0..3 {
        let _ = run_scroll_and_hover(&ctx, &mut app, Some(Pos2::new(560.0, 280.0)), -180.0);
    }
    let grid_scrolled = run_scroll_and_hover(&ctx, &mut app, Some(Pos2::new(560.0, 280.0)), 0.0);
    assert!(
        find_text(&grid_scrolled.shapes, "item-", false)
            .unwrap()
            .2
            .top()
            > 120.0
    );
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::List));

    // 2. Non-text-select pointer cursor over filename text, click, context menu & shortcuts.
    for _ in 0..5 {
        let _ = run_scroll_and_hover(&ctx, &mut app, None, 0.0);
    }
    let settled = run_scroll_and_hover(&ctx, &mut app, None, 0.0);
    let (vis_name, fname_pos, _) = find_text(&settled.shapes, "item-", false).unwrap();
    let _ = run_scroll_and_hover(&ctx, &mut app, Some(fname_pos), 0.0);
    let hovered = run_scroll_and_hover(&ctx, &mut app, Some(fname_pos), 0.0);
    assert_eq!(
        hovered.platform_output.cursor_icon,
        CursorIcon::PointingHand
    );
    let btn = |pos, b, p| Event::PointerButton {
        pos,
        button: b,
        pressed: p,
        modifiers: Modifiers::NONE,
    };
    let click = |a: &mut ControlCenterApp, pos, b| {
        for pressed in [true, false] {
            send_evs(
                &ctx,
                a,
                vec![Event::PointerMoved(pos), btn(pos, b, pressed)],
            );
        }
    };
    click(&mut app, fname_pos, egui::PointerButton::Primary);
    let sel_out = send_evs(&ctx, &mut app, Vec::new());
    let sel_t = support::collect(&sel_out.shapes);
    assert!(
        has(&sel_t, "Rename")
            && has(&sel_t, "Copy")
            && has(&sel_t, "Move")
            && has(&sel_t, "Delete")
    );
    assert_eq!(
        app.explorer.sessions[&a_id].selected_id,
        Some(eid(&app, a_id, &vis_name))
    );

    let sel_pos = find_text(&sel_out.shapes, &vis_name, true).unwrap().1;
    click(&mut app, sel_pos, egui::PointerButton::Secondary);
    let menu_t = support::collect(&send_evs(&ctx, &mut app, Vec::new()).shapes);
    assert!(has(&menu_t, "Sync Down (Pin Local)") && has(&menu_t, "Rename"));
    let tap_key = |a: &mut ControlCenterApp, k, m| {
        for pressed in [true, false] {
            send_evs(
                &ctx,
                a,
                vec![Event::Key {
                    key: k,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: m,
                }],
            );
        }
        send_evs(&ctx, a, Vec::new())
    };
    tap_key(&mut app, egui::Key::Escape, Modifiers::NONE);
    let _ = tap_key(&mut app, egui::Key::F2, Modifiers::NONE);
    assert!(has(
        &support::collect(&send_evs(&ctx, &mut app, Vec::new()).shapes),
        "Rename Item"
    ));
    tap_key(&mut app, egui::Key::Escape, Modifiers::NONE);
    let _ = tap_key(&mut app, egui::Key::C, Modifiers::COMMAND);
    assert!(has(
        &support::collect(&send_evs(&ctx, &mut app, Vec::new()).shapes),
        "Paste Copy Here"
    ));
    app.apply_action(UiAction::ExplorerClearClipboard);

    // 3. UI Rename, Copy, Move, and Delete actions.
    let item0_id = eid(&app, a_id, "item-00.txt");
    let target_id = eid(&app, a_id, "Target");
    app.apply_action(UiAction::ExplorerStartRename(item0_id));
    if let Some(opendrive_app::desktop::ExplorerModal::Rename { draft, .. }) =
        app.explorer.modal.as_mut()
    {
        *draft = "item-00-renamed.txt".into();
    }
    app.apply_action(UiAction::ExplorerSaveRename);
    wait_entry(&ctx, &mut app, a_id, "item-00-renamed.txt");

    let ren_id = eid(&app, a_id, "item-00-renamed.txt");
    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![ren_id],
        cut: false,
    });
    app.apply_action(UiAction::ExplorerPasteClipboard(None));
    wait_entry(&ctx, &mut app, a_id, "item-00-renamed (copy).txt");

    let copy_id = eid(&app, a_id, "item-00-renamed (copy).txt");
    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![copy_id],
        cut: true,
    });
    app.apply_action(UiAction::ExplorerNavigateNode(target_id));
    wait_until(&ctx, &mut app, |a| !a.explorer.is_busy(a_id));
    app.apply_action(UiAction::ExplorerPasteClipboard(None));
    wait_entry(&ctx, &mut app, a_id, "item-00-renamed (copy).txt");
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(0));
    wait_entry(&ctx, &mut app, a_id, "item-00-renamed.txt");

    app.apply_action(UiAction::ExplorerRequestDelete(vec![ren_id]));
    assert!(has(&texts(&ctx, &mut app), "Delete Item"));
    app.apply_action(UiAction::ExplorerConfirmDelete);
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id)
            && !a.explorer.sessions[&a_id]
                .entries
                .iter()
                .any(|e| e.name == "item-00-renamed.txt")
    });

    // 4. CLI rename, cp, mv, rm dispatch.
    for args in [
        vec!["rename", "Alpha", "item-01.txt", "cli-one.txt"],
        vec!["cp", "Alpha", "cli-one.txt"],
        vec!["mv", "Alpha", "cli-one (copy).txt", "Target"],
        vec!["rm", "Alpha", "cli-one.txt"],
    ] {
        dispatch_bridge(args[0], args[1..].iter().map(|s| s.to_string()), &bridge).unwrap();
    }

    h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
