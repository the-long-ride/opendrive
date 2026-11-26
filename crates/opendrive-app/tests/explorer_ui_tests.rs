//! Headless UI and FakeDrive integration tests for Cloud Explorer, sessions, sync, and Open With rules.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::formatters::GIB;
use opendrive_app::design::theme::palette;
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, NoticeTone, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::{load_settings, Settings};
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::{NodeState, Store};
use opendrive_sync::SyncEngine;
use std::path::Path;
use std::sync::{Arc, Mutex};
use support::{
    add_acct, dirty_cached, eid, has, has_sub, render_rules_and_close, seed_drive, state_of, texts,
    wait_entry, wait_state, wait_until, LaunchLog,
};
use uuid::Uuid;

fn render_shapes_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    w: f32,
    h: f32,
) -> Vec<egui::epaint::ClippedShape> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
        ..Default::default()
    };
    ctx.run_ui(raw.clone(), |ui| app.render_root(ui))
        .textures_delta
        .clear();
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out.shapes
}

fn walk_shapes<'a>(shapes: &'a [egui::epaint::ClippedShape], f: &mut impl FnMut(&'a Shape)) {
    fn visit<'a>(s: &'a Shape, f: &mut impl FnMut(&'a Shape)) {
        f(s);
        if let Shape::Vec(v) = s {
            v.iter().for_each(|x| visit(x, f));
        }
    }
    shapes.iter().for_each(|cs| visit(&cs.shape, f));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_ui_session_sync_rules_and_quota_work() {
    let tmp = std::env::temp_dir().join(format!("od-exp-ui-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let up_file = tmp.join("upload.txt");
    std::fs::write(&up_file, b"uploaded-from-test").unwrap();

    let fdir = "application/vnd.google-apps.folder";
    let drive_a = seed_drive(
        &[
            ("dir-proj", "Projects", fdir, "root", &[]),
            (
                "f-plan",
                "plan.pdf",
                "application/pdf",
                "dir-proj",
                b"pdf-v1",
            ),
            ("f-note", "notes.txt", "text/plain", "dir-proj", b"note-v1"),
            ("f-pic", "photo.png", "image/png", "root", b"png"),
            ("f-vid", "demo.mp4", "video/mp4", "root", &[7u8; 4096]),
        ],
        2 * GIB,
        10 * GIB,
    );
    let drive_b = seed_drive(
        &[("f-b", "beta.txt", "text/plain", "root", b"beta")],
        512,
        0,
    );
    let ((addr_a, h_a), (addr_b, h_b)) = tokio::join!(
        FakeDriveServer::start(Arc::clone(&drive_a)),
        FakeDriveServer::start(Arc::clone(&drive_b))
    );

    let eng = Arc::new(
        SyncEngine::new(Store::open_in_memory().unwrap())
            .with_stream_cache_dir(tmp.join("sc"))
            .unwrap(),
    );
    let (br, _rx) =
        Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg.clone()));
    let bridge = br
        .with_data_dir(data)
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr_a, 2 * GIB, 10 * GIB);
    let b_id = add_acct(&bridge, &eng, "Beta", addr_b, 512, 0);

    let launched: LaunchLog = Arc::new(Mutex::new(Vec::new()));
    let (lref, pup) = (Arc::clone(&launched), up_file.clone());
    let endpoints = AuthEndpoints {
        upload_files_picker: Arc::new(move || vec![pup.clone()]),
        item_launcher: Arc::new(move |cmd, tgt| {
            lref.lock()
                .unwrap()
                .push((cmd.map(str::to_string), tgt.into()));
            Ok(())
        }),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    let ctx = egui::Context::default();

    // (a) Dashboard Explore header placement & full-viewport equal-height Bento Explorer.
    let dash = texts(&ctx, &mut app);
    assert!(has(&dash, "Explore") && has(&dash, "Connected Accounts") && !has(&dash, "Connected"));
    let dash_shapes = render_shapes_at(&ctx, &mut app, 960.0, 720.0);
    let ok = palette(app.theme_mode).ok_tint;
    let (mut exp_r, mut conn_r, mut sync_r) = (None, None, None);
    walk_shapes(&dash_shapes, &mut |s| match s {
        Shape::Rect(rs) if rs.fill == ok && rs.rect.width() < 30.0 && rs.rect.top() > 260.0 => {
            conn_r.get_or_insert(rs.rect);
        }
        Shape::Text(ts) => {
            let r = ts.galley.rect.translate(ts.pos.to_vec2());
            match ts.galley.text() {
                "Explore" if r.top() > 260.0 => {
                    exp_r.get_or_insert(r);
                }
                "Sync On" => {
                    sync_r.get_or_insert(r);
                }
                _ => {}
            }
        }
        _ => {}
    });
    let (exp_r, conn_r, sync_r) = (exp_r.unwrap(), conn_r.unwrap(), sync_r.unwrap());
    assert!((exp_r.center().y - conn_r.center().y).abs() < 6.0);
    assert!(exp_r.right() <= conn_r.left() && exp_r.bottom() < sync_r.top());

    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    let exp_t = wait_entry(&ctx, &mut app, a_id, "Projects");
    assert!(
        app.explorer.open && has(&exp_t, "Drive Accounts") && !has(&exp_t, "Connected Accounts")
    );
    assert!(has_sub(&exp_t, "2.0 GB / 10 GB") && !has_sub(&exp_t, "512 B"));
    let exp_shapes = render_shapes_at(&ctx, &mut app, 960.0, 720.0);
    let card_bg = palette(app.theme_mode).card_bg;
    let mut bento_cards = Vec::new();
    walk_shapes(&exp_shapes, &mut |s| {
        if let Shape::Rect(rs) = s {
            if rs.fill == card_bg && rs.rect.height() > 400.0 {
                bento_cards.push(rs.rect);
            }
        }
    });
    assert_eq!(bento_cards.len(), 2);
    assert!((bento_cards[0].top() - bento_cards[1].top()).abs() < 0.5);
    assert!((bento_cards[0].height() - bento_cards[1].height()).abs() < 0.5);
    assert!(bento_cards[0].right() < bento_cards[1].left());

    // (b) Session folder persistence, search, List/Grid toggle, Dashboard return.
    let (proj_id, photo_id, video_id) = (
        eid(&app, a_id, "Projects"),
        eid(&app, a_id, "photo.png"),
        eid(&app, a_id, "demo.mp4"),
    );
    app.apply_action(UiAction::ExplorerNavigateNode(proj_id));
    wait_entry(&ctx, &mut app, a_id, "plan.pdf");
    app.explorer.active_session_mut().unwrap().search_query = "plan".into();
    let filt = texts(&ctx, &mut app);
    assert!(has(&filt, "plan.pdf") && !has(&filt, "notes.txt"));
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    assert!(has(&texts(&ctx, &mut app), "plan.pdf"));
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::List));

    app.apply_action(UiAction::ExplorerSelectAccount(b_id.to_string()));
    assert!(has(
        &wait_entry(&ctx, &mut app, b_id, "beta.txt"),
        "beta.txt"
    ));
    app.apply_action(UiAction::ExplorerSelectAccount(a_id.to_string()));
    let back = texts(&ctx, &mut app);
    let sess = &app.explorer.sessions[&a_id];
    let crumbs: Vec<&str> = sess.breadcrumbs.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(
        (crumbs, sess.search_query.as_str()),
        (vec!["My Drive", "Projects"], "plan")
    );
    assert!(has(&back, "plan.pdf"));
    app.explorer
        .active_session_mut()
        .unwrap()
        .search_query
        .clear();
    app.apply_action(UiAction::CloseExplorer);
    assert!(!app.explorer.open && has(&texts(&ctx, &mut app), "Connected Accounts"));
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));

    // (c) Sync Down, Sync Up, Sync Up & Clear Local, and Safety Invariant #3 aborted clear.
    let (plan_id, notes_id) = (eid(&app, a_id, "plan.pdf"), eid(&app, a_id, "notes.txt"));
    app.apply_action(UiAction::ExplorerSyncDown(vec![plan_id]));
    assert!(!has(
        &wait_state(&ctx, &mut app, &eng, a_id, plan_id, NodeState::Cached),
        "Local"
    ));
    app.apply_action(UiAction::ExplorerSyncUpFiles);
    wait_entry(&ctx, &mut app, a_id, "upload.txt");
    let up = drive_a
        .list_children("dir-proj")
        .into_iter()
        .find(|f| f.name == "upload.txt")
        .unwrap();
    assert_eq!(up.content, b"uploaded-from-test");

    let plan_cache = dirty_cached(&eng, plan_id, b"edited-plan-v2");
    app.apply_action(UiAction::ExplorerSyncUpAndClear(vec![plan_id]));
    assert!(!has(
        &wait_state(&ctx, &mut app, &eng, a_id, plan_id, NodeState::Placeholder),
        "Cloud"
    ));
    assert!(
        drive_a.get_file("f-plan").unwrap().content == b"edited-plan-v2"
            && !Path::new(&plan_cache).exists()
    );

    app.apply_action(UiAction::ExplorerSyncDown(vec![notes_id]));
    wait_state(&ctx, &mut app, &eng, a_id, notes_id, NodeState::Cached);
    let notes_cache = dirty_cached(&eng, notes_id, b"unsynced");
    drive_a.set_forced_error(Some(500));
    app.apply_action(UiAction::ExplorerSyncUpAndClear(vec![notes_id]));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id)
            && a.notice
                .as_ref()
                .is_some_and(|n| n.tone == NoticeTone::Error)
    });
    assert!(Path::new(&notes_cache).exists() && state_of(&eng, notes_id) == NodeState::Dirty);
    drive_a.set_forced_error(None);

    // (d) Direct open + OpenWithRule UI & settings.json persistence + video stream launch.
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(0));
    wait_entry(&ctx, &mut app, a_id, "photo.png");
    app.apply_action(UiAction::ExplorerOpenItem(photo_id));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id) && !launched.lock().unwrap().is_empty()
    });
    assert!(
        launched.lock().unwrap()[0].0.is_none()
            && Path::new(&launched.lock().unwrap()[0].1).exists()
    );

    app.apply_action(UiAction::ToggleSettings(Some(true)));
    assert!(has(&texts(&ctx, &mut app), "OpenDrive Settings"));
    for (pat, cmd) in [("@video", "mpv"), ("pdf", "sumatrapdf")] {
        app.apply_action(UiAction::StartAddOpenWithRule);
        app.open_rule_draft.pattern = pat.into();
        app.open_rule_draft.app_command = cmd.into();
        app.apply_action(UiAction::SaveOpenWithRule);
    }
    app.apply_action(UiAction::RemoveOpenWithRule(1));
    let saved = load_settings(&cfg).unwrap();
    assert_eq!(saved.open_with_rules.len(), 1);
    let r = &saved.open_with_rules[0];
    assert_eq!(
        (r.pattern.as_str(), r.app_command.as_str()),
        ("@video", "mpv")
    );
    let r_t = render_rules_and_close(&ctx, &mut app);
    assert!(
        has(&r_t, "Open With Rules (Custom Format Apps)")
            && has(&r_t, "@video")
            && has(&r_t, "mpv")
            && !app.settings_open
    );

    app.apply_action(UiAction::ExplorerOpenItem(video_id));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id) && launched.lock().unwrap().len() == 2
    });
    let (vcmd, vurl) = launched.lock().unwrap()[1].clone();
    assert!(
        vcmd.as_deref() == Some("mpv")
            && vurl.starts_with("http://127.0.0.1:")
            && vurl.contains("/stream/")
    );

    h_a.abort();
    h_b.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
