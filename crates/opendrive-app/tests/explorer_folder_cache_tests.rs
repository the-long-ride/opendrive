//! End-to-end tests for Cloud Explorer visited folder path caching, user-update invalidation, and Settings toggle.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use opendrive_app::desktop::{ControlCenterApp, ExplorerModal, ExplorerViewMode, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{load_settings, Settings};
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{add_acct, click_at, eid, has, seed_drive, send_frame, text_pos, texts, wait_entry};
use uuid::Uuid;

fn add_remote_file(drive: &FakeDrive, id: &str, name: &str, parent: &str) {
    drive.add_file(FakeFile {
        id: id.into(),
        name: name.into(),
        mime_type: "text/plain".into(),
        size: 5,
        parents: vec![parent.into()],
        content: b"hello".to_vec(),
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_caches_folder_path_on_back_and_forth_and_invalidates_only_on_user_update() {
    let tmp = std::env::temp_dir().join(format!("od-exp-fcache-{}", Uuid::new_v4()));
    let cfg = tmp.join("cfg");
    std::fs::create_dir_all(&cfg).unwrap();

    let drive = seed_drive(
        &[
            (
                "d-proj",
                "Projects",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f-root", "root.txt", "text/plain", "root", b"r1"),
            (
                "d-sub",
                "Sub",
                "application/vnd.google-apps.folder",
                "d-proj",
                &[],
            ),
            ("f-proj", "proj.txt", "text/plain", "d-proj", b"p1"),
            ("f-deep", "deep.txt", "text/plain", "d-sub", b"d1"),
        ],
        1024,
        8192,
    );
    let (addr, drive_h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (bridge, _rx) =
        Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg.clone()));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 1024, 8192);

    let mut app = ControlCenterApp::new(bridge);
    let ctx = egui::Context::default();
    assert!(app.state.settings.explorer_folder_cache);

    app.apply_action(UiAction::ToggleSettings(Some(true)));
    assert!(has(&texts(&ctx, &mut app), "Cache Explorer Folder Paths"));
    app.apply_action(UiAction::ToggleSettings(Some(false)));

    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "root.txt");
    let proj_id = eid(&app, a_id, "Projects");

    app.apply_action(UiAction::ExplorerSelectEntry(Some(proj_id)));
    let root_shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    click_at(
        &ctx,
        &mut app,
        text_pos(&root_shapes, "Open"),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    wait_entry(&ctx, &mut app, a_id, "proj.txt");
    assert_eq!(
        app.explorer.sessions[&a_id].breadcrumbs.last().unwrap().1,
        "Projects"
    );
    assert!(app.notice.is_none());
    let sub_id = eid(&app, a_id, "Sub");

    app.apply_action(UiAction::ExplorerOpenItem(sub_id));
    wait_entry(&ctx, &mut app, a_id, "deep.txt");
    assert_eq!(
        app.explorer.sessions[&a_id].breadcrumbs.last().unwrap().1,
        "Sub"
    );
    assert!(app.notice.is_none());
    let deep_id = eid(&app, a_id, "deep.txt");

    // Simulate upstream changes + remote HTTP 500: cached back/forth must not refetch.
    drive.set_forced_error(Some(500));
    add_remote_file(&drive, "f-unseen-root", "unseen-root.txt", "root");
    add_remote_file(&drive, "f-unseen-proj", "unseen-proj.txt", "d-proj");

    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(1));
    assert!(!app.explorer.is_busy(a_id) && app.notice.is_none());
    let proj_entries = &app.explorer.sessions[&a_id].entries;
    assert!(proj_entries.iter().any(|e| e.name == "proj.txt"));
    assert!(!proj_entries.iter().any(|e| e.name == "unseen-proj.txt"));

    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(0));
    assert!(!app.explorer.is_busy(a_id) && app.notice.is_none());
    let root_entries = &app.explorer.sessions[&a_id].entries;
    assert!(root_entries.iter().any(|e| e.name == "root.txt"));
    assert!(!root_entries.iter().any(|e| e.name == "unseen-root.txt"));

    app.apply_action(UiAction::ExplorerNavigateNode(proj_id));
    assert!(!app.explorer.is_busy(a_id));
    app.apply_action(UiAction::ExplorerNavigateNode(sub_id));
    assert!(!app.explorer.is_busy(a_id) && app.notice.is_none());
    assert!(app.explorer.sessions[&a_id]
        .entries
        .iter()
        .any(|e| e.name == "deep.txt"));

    // User update invalidates cache so parent folders fetch fresh on navigation.
    drive.set_forced_error(None);
    app.apply_action(UiAction::ExplorerStartRename(deep_id));
    if let Some(ExplorerModal::Rename { draft, .. }) = app.explorer.modal.as_mut() {
        *draft = "deep-v2.txt".into();
    }
    app.apply_action(UiAction::ExplorerSaveRename);
    wait_entry(&ctx, &mut app, a_id, "deep-v2.txt");

    // User update in cached subfolder must force-refresh Drive (not re-cache stale SQLite).
    add_remote_file(&drive, "f-after-mut-proj", "after-mut-proj.txt", "d-proj");
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(1));
    wait_entry(&ctx, &mut app, a_id, "unseen-proj.txt");
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(0));
    wait_entry(&ctx, &mut app, a_id, "unseen-root.txt");

    // Mutate directly inside cached Projects subfolder and verify fresh Drive listing arrives.
    app.apply_action(UiAction::ExplorerNavigateNode(proj_id));
    let proj_file_id = eid(&app, a_id, "proj.txt");
    app.apply_action(UiAction::ExplorerStartRename(proj_file_id));
    if let Some(ExplorerModal::Rename { draft, .. }) = app.explorer.modal.as_mut() {
        *draft = "proj-v2.txt".into();
    }
    app.apply_action(UiAction::ExplorerSaveRename);
    wait_entry(&ctx, &mut app, a_id, "after-mut-proj.txt");

    // Changing docs mode clears Explorer folder caches.
    app.apply_action(UiAction::SetDocsMode(
        opendrive_core::DocsMode::ExportOffice,
    ));
    assert!(app.explorer.sessions[&a_id].folder_cache.is_empty());

    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabling_explorer_folder_cache_in_settings_persists_and_fetches_every_navigation() {
    let tmp = std::env::temp_dir().join(format!("od-exp-fcache-off-{}", Uuid::new_v4()));
    let cfg = tmp.join("cfg");
    std::fs::create_dir_all(&cfg).unwrap();

    let drive = seed_drive(
        &[
            (
                "d-proj",
                "Projects",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f-root", "root.txt", "text/plain", "root", b"r1"),
            ("f-proj", "proj.txt", "text/plain", "d-proj", b"p1"),
        ],
        512,
        4096,
    );
    let (addr, drive_h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (bridge, _rx) =
        Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg.clone()));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 512, 4096);

    let mut app = ControlCenterApp::new(bridge);
    let ctx = egui::Context::default();
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "root.txt");
    let proj_id = eid(&app, a_id, "Projects");
    app.apply_action(UiAction::ExplorerNavigateNode(proj_id));
    wait_entry(&ctx, &mut app, a_id, "proj.txt");

    app.apply_action(UiAction::ToggleExplorerFolderCache);
    assert!(!app.state.settings.explorer_folder_cache);
    assert!(!load_settings(&cfg).unwrap().explorer_folder_cache);
    assert!(app.explorer.sessions[&a_id].folder_cache.is_empty());

    add_remote_file(&drive, "f-fresh-root", "fresh-root.txt", "root");
    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(0));
    wait_entry(&ctx, &mut app, a_id, "fresh-root.txt");

    add_remote_file(&drive, "f-fresh-proj", "fresh-proj.txt", "d-proj");
    app.apply_action(UiAction::ExplorerNavigateNode(proj_id));
    wait_entry(&ctx, &mut app, a_id, "fresh-proj.txt");

    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn double_click_opens_folder_and_file_without_layout_shift() {
    let tmp = std::env::temp_dir().join(format!("od-exp-dbl-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let drive = seed_drive(
        &[
            (
                "d-proj",
                "Projects",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f-proj", "proj.txt", "text/plain", "d-proj", b"p1"),
        ],
        512,
        4096,
    );
    let (addr, drive_h) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let eng = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let mm = opendrive_app::runtime::mount_manager::MountManager::new_null().with_mount_root(mnt);
    let bridge = br.with_data_dir(data).with_mount_manager(Arc::new(mm));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr, 512, 4096);
    let launched = Arc::new(std::sync::Mutex::new(Vec::new()));
    let lc = Arc::clone(&launched);
    let endpoints = AuthEndpoints {
        item_launcher: Arc::new(move |app, tgt| {
            lc.lock()
                .unwrap()
                .push((app.map(str::to_string), tgt.to_string()));
            Ok(())
        }),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    let ctx = egui::Context::default();
    let run_1240 = |app: &mut ControlCenterApp, t: f64, events: Vec<Event>| {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1240.0, 720.0))),
            time: Some(t),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
        out.textures_delta.clear();
        out.shapes
    };
    let dbl_click = |app: &mut ControlCenterApp, pos: Pos2, t0: f64| {
        for (dt, pressed) in [(0.0, true), (0.02, false), (0.12, true), (0.14, false)] {
            let btn = Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            run_1240(app, t0 + dt, vec![Event::PointerMoved(pos), btn]);
        }
    };

    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "Projects");
    let proj_pos = text_pos(&run_1240(&mut app, 0.9, Vec::new()), "Projects");
    dbl_click(&mut app, proj_pos, 1.0);
    wait_entry(&ctx, &mut app, a_id, "proj.txt");

    let file_pos = text_pos(&run_1240(&mut app, 1.9, Vec::new()), "proj.txt");
    dbl_click(&mut app, file_pos, 2.0);
    support::wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id) && !launched.lock().unwrap().is_empty()
    });

    app.apply_action(UiAction::ExplorerNavigateBreadcrumb(0));
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    wait_entry(&ctx, &mut app, a_id, "Projects");
    let grid_pos = text_pos(&run_1240(&mut app, 2.9, Vec::new()), "Projects");
    dbl_click(&mut app, grid_pos, 3.0);
    wait_entry(&ctx, &mut app, a_id, "proj.txt");

    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
