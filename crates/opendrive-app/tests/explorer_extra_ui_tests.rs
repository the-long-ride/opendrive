//! Extra headless UI & CLI integration tests for Cloud Explorer toolbar, drop, layouts, and states.

mod common_auth_helpers;
mod support;

use common_auth_helpers::{run_frame, MockDroppedFile};
use eframe::egui::{self, Modifiers, OutputCommand, PointerButton, Pos2, RawInput, Rect, Vec2};
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::{MediaKind, Settings};
use opendrive_fake_drive::{FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::{
    add_acct, click_at, collect, eid, has, has_sub, seed_drive, send_frame, text_pos, texts,
    wait_entry, wait_until,
};
use uuid::Uuid;

fn render_at(ctx: &egui::Context, app: &mut ControlCenterApp, w: f32, h: f32) -> Vec<String> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
        ..Default::default()
    };
    ctx.run_ui(raw.clone(), |ui| app.render_root(ui))
        .textures_delta
        .clear();
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    collect(&out.shapes)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_extra_toolbar_drop_narrow_and_media_states() {
    let tmp = std::env::temp_dir().join(format!("od-exp-extra-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    let up_dir = tmp.join("up_folder");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::create_dir_all(&up_dir).unwrap();
    std::fs::write(up_dir.join("nested.txt"), b"from-folder").unwrap();
    let drop_file = tmp.join("dropped.txt");
    std::fs::write(&drop_file, b"dropped-body").unwrap();
    let exe_path = tmp.join("player.exe");

    let fdir = "application/vnd.google-apps.folder";
    let gdoc = "application/vnd.google-apps.document";
    let oct = "application/octet-stream";
    let drive_a = seed_drive(
        &[
            ("dir-empty", "EmptyDir", fdir, "root", &[]),
            ("f-vid", "demo.mp4", "video/mp4", "root", &[7u8; 2048]),
            ("f-aud", "song.mp3", "audio/mpeg", "root", b"mp3"),
            ("f-arc", "pack.zip", "application/zip", "root", b"zip"),
            ("f-code", "notes.md", "text/markdown", "root", b"# md"),
            ("f-doc", "SpecDoc", gdoc, "root", b""),
            ("f-bin", "blob.bin", oct, "root", b"bin"),
        ],
        1024,
        4096,
    );
    let (addr_a, h_a) = FakeDriveServer::start(Arc::clone(&drive_a)).await;
    let store = Store::open_in_memory().unwrap();
    let eng = Arc::new(
        SyncEngine::new(store)
            .with_stream_cache_dir(tmp.join("sc"))
            .unwrap(),
    );
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&eng), Some(cfg));
    let bridge = br
        .with_data_dir(data)
        .with_mount_manager(Arc::new(MountManager::new_null().with_mount_root(mnt)));
    let a_id = add_acct(&bridge, &eng, "Alpha", addr_a, 1024, 4096);

    let (up_c, exe_c) = (up_dir.clone(), exe_path.clone());
    let endpoints = AuthEndpoints {
        upload_folder_picker: Arc::new(move || Some(up_c.clone())),
        app_exe_picker: Arc::new(move || Some(exe_c.clone())),
        item_launcher: Arc::new(|_, _| Ok(())),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), endpoints);
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);

    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "demo.mp4");

    let kinds: Vec<MediaKind> = app.explorer.sessions[&a_id]
        .entries
        .iter()
        .map(|e| e.media_kind)
        .collect();
    for expected in [
        MediaKind::Audio,
        MediaKind::Archive,
        MediaKind::CodeText,
        MediaKind::NativeDoc,
        MediaKind::Other,
        MediaKind::Video,
        MediaKind::Folder,
    ] {
        assert!(kinds.contains(&expected), "missing {expected:?}");
    }

    let video_id = eid(&app, a_id, "demo.mp4");
    app.apply_action(UiAction::ExplorerSelectEntry(Some(video_id)));
    assert!(has(&texts(&ctx, &mut app), "Copy Stream URL"));

    app.apply_action(UiAction::ExplorerCopyStreamUrl(video_id));
    let mut copied = None;
    let dl = Instant::now() + Duration::from_secs(5);
    while Instant::now() < dl {
        let out = run_frame(&ctx, &mut app, Vec::new());
        for cmd in out.platform_output.commands {
            if let OutputCommand::CopyText(t) = cmd {
                copied = Some(t);
            }
        }
        if !app.explorer.is_busy(a_id) && copied.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let url = copied.expect("stream url copied");
    assert!(url.starts_with("http://127.0.0.1:") && url.contains("/stream/"));
    assert!(app
        .notice
        .as_ref()
        .is_some_and(|n| n.message.contains("Copied")));

    drive_a.add_file(FakeFile {
        id: "f-ref".into(),
        name: "refreshed.txt".into(),
        mime_type: "text/plain".into(),
        size: 3,
        parents: vec!["root".into()],
        content: b"new".to_vec(),
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
    app.apply_action(UiAction::ExplorerRefresh);
    wait_entry(&ctx, &mut app, a_id, "refreshed.txt");

    let up_btn = text_pos(&send_frame(&ctx, &mut app, Vec::new()).shapes, "Upload");
    click_at(
        &ctx,
        &mut app,
        up_btn,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let up_menu = send_frame(&ctx, &mut app, Vec::new()).shapes;
    assert!(has(&collect(&up_menu), "Upload Files") && has(&collect(&up_menu), "Upload Folder"));
    click_at(
        &ctx,
        &mut app,
        text_pos(&up_menu, "Upload Folder"),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    wait_entry(&ctx, &mut app, a_id, "up_folder");
    let up_f = drive_a
        .list_children("root")
        .into_iter()
        .find(|f| f.name == "up_folder")
        .unwrap();
    assert!(drive_a
        .list_children(&up_f.id)
        .iter()
        .any(|f| f.name == "nested.txt" && f.content == b"from-folder"));

    let drop: egui::DroppedFileHandle = Arc::new(MockDroppedFile {
        path: drop_file,
        bytes: Err("unused".into()),
    });
    run_frame(&ctx, &mut app, vec![drop]);
    wait_entry(&ctx, &mut app, a_id, "dropped.txt");
    assert!(drive_a
        .list_children("root")
        .iter()
        .any(|f| f.name == "dropped.txt" && f.content == b"dropped-body"));

    app.apply_action(UiAction::BrowseOpenWithRuleApp);
    assert_eq!(
        app.open_rule_draft.app_command,
        exe_path.display().to_string()
    );

    let (aud_id, arc_id) = (eid(&app, a_id, "song.mp3"), eid(&app, a_id, "pack.zip"));
    for (nid, st) in [
        (aud_id, NodeState::Conflict),
        (arc_id, NodeState::Hydrating),
    ] {
        eng.store()
            .lock()
            .update_node_state(nid as i64, st.clone(), None)
            .unwrap();
        if let Some(e) = app
            .explorer
            .active_session_mut()
            .unwrap()
            .entries
            .iter_mut()
            .find(|e| e.id == nid)
        {
            e.state = st;
        }
    }

    let list_t = render_at(&ctx, &mut app, 960.0, 1400.0);
    assert!(
        has(&list_t, "song.mp3")
            && has(&list_t, "pack.zip")
            && has(&list_t, "SpecDoc")
            && !has(&list_t, "Conflict")
            && !has(&list_t, "Syncing")
    );
    assert!(has(&render_at(&ctx, &mut app, 540.0, 1400.0), "song.mp3"));

    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let grid_t = render_at(&ctx, &mut app, 960.0, 1400.0);
    assert!(has(&grid_t, "pack.zip") && has(&grid_t, "notes.md") && has(&grid_t, "blob.bin"));
    assert!(has(
        &render_at(&ctx, &mut app, 540.0, 1400.0),
        "Drive Accounts"
    ));

    app.explorer.active_session_mut().unwrap().search_query = "zzz-nomatch".into();
    assert!(has_sub(
        &texts(&ctx, &mut app),
        "No items match your search filter."
    ));
    app.explorer
        .active_session_mut()
        .unwrap()
        .search_query
        .clear();

    let vid_m3u = bridge.explorer_open_item(a_id, video_id, true).unwrap();
    assert!(
        vid_m3u.is_stream_url
            && vid_m3u.target.ends_with(".m3u")
            && std::path::Path::new(&vid_m3u.target).exists()
    );
    bridge
        .update_open_with_rules(vec![opendrive_core::OpenWithRule {
            pattern: "mp4".into(),
            app_command: exe_path.display().to_string(),
        }])
        .unwrap();
    let vid_local = bridge.explorer_open_item(a_id, video_id, true).unwrap();
    assert!(
        !vid_local.is_stream_url
            && vid_local.app_command.is_some()
            && std::path::Path::new(&vid_local.target).exists()
    );
    std::fs::write(&vid_local.target, b"edited-mp4").unwrap();
    app.apply_action(UiAction::ExplorerSyncUpItems(vec![video_id]));
    wait_until(&ctx, &mut app, |a| !a.explorer.is_busy(a_id));
    assert!(drive_a
        .list_children("root")
        .iter()
        .any(|f| f.id == "f-vid" && f.content == b"edited-mp4"));

    let empty_id = eid(&app, a_id, "EmptyDir");
    app.apply_action(UiAction::ExplorerNavigateNode(empty_id));
    let empty_t = wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id)
            && a.explorer
                .sessions
                .get(&a_id)
                .is_some_and(|s| s.breadcrumbs.len() == 2)
    });
    assert!(has_sub(&empty_t, "This folder is empty"));

    h_a.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
