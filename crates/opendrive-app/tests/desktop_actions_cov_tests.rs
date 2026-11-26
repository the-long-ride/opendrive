//! Integration coverage tests for Explorer selection shortcuts, file-ops modals, items grid, and Desktop settings/ledger/modals.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use opendrive_app::design::formatters::BANDWIDTH_5M;
use opendrive_app::desktop::{
    ControlCenterApp, ExplorerModal, ExplorerSortColumn, ExplorerViewMode, NoticeTone, UiAction,
};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{AccountStatus, DocsMode, OpenWithRule, Settings};
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::{Arc, Mutex};
use support::{add_acct, seed_drive, send_frame, tap_key, text_pos, wait_entry};
use uuid::Uuid;

fn frame_at(ctx: &egui::Context, app: &mut ControlCenterApp, w: f32, h: f32) -> egui::FullOutput {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

#[test]
fn explorer_shortcuts_grid_modals_and_file_ops_coverage() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = seed_drive(
        &[
            (
                "f_dir",
                "SubFolder",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f_vid", "clip.mp4", "video/mp4", "root", b"0123456789"),
            ("f_txt", "alpha.txt", "text/plain", "root", b"hello"),
            ("d_a", "dup.txt", "text/plain", "root", b"hello"),
            ("d_b", "dup.txt", "text/plain", "root", b"hello"),
        ],
        512,
        4096,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(drive));
    let tmp = std::env::temp_dir().join(format!("od-ui-cov-{}", Uuid::new_v4()));
    let local_up = tmp.join("up.txt");
    let up_dir = tmp.join("updir");
    std::fs::create_dir_all(&up_dir).unwrap();
    std::fs::write(&local_up, b"upload").unwrap();
    std::fs::write(up_dir.join("in.txt"), b"inside").unwrap();

    let (bridge, _rx) = Bridge::new(Settings::default());
    let eng = bridge.engine();
    let aid = add_acct(&bridge, eng, "Alpha", addr, 512, 4096);
    let launched = Arc::new(Mutex::new(Vec::<String>::new()));
    let lref = Arc::clone(&launched);
    let ep = AuthEndpoints {
        upload_files_picker: Arc::new({
            let p = local_up.clone();
            move || vec![p.clone()]
        }),
        upload_folder_picker: Arc::new({
            let d = up_dir.clone();
            move || Some(d.clone())
        }),
        item_launcher: Arc::new(move |_, t| {
            lref.lock().unwrap().push(t.to_string());
            if t.contains("clip") || t.contains(".m3u") {
                Err("player missing".into())
            } else {
                Ok(())
            }
        }),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep);
    app.apply_action(UiAction::OpenExplorer(aid.to_string()));
    let ctx = egui::Context::default();
    wait_entry(&ctx, &mut app, aid, "alpha.txt");

    // Sort columns + selection range / additive / prune / remove
    app.apply_action(UiAction::ExplorerToggleSort(ExplorerSortColumn::Modified));
    app.apply_action(UiAction::ExplorerToggleSort(ExplorerSortColumn::Modified));
    let id_dir = support::eid(&app, aid, "SubFolder");
    let id_txt = support::eid(&app, aid, "alpha.txt");
    let id_vid = support::eid(&app, aid, "clip.mp4");
    app.apply_action(UiAction::ExplorerSelectEntry(Some(id_dir)));
    app.apply_action(UiAction::ExplorerSelectRange {
        node_id: id_txt,
        additive: true,
    });
    app.apply_action(UiAction::ExplorerSelectRange {
        node_id: 99999,
        additive: false,
    });
    {
        let s = app.explorer.sessions.get_mut(&aid).unwrap();
        s.selected_ids = vec![id_dir, id_txt];
        s.anchor_id = Some(id_dir);
        s.search_query = "alpha".into();
        s.prune_to_visible();
        assert_eq!(s.selected_ids, vec![id_txt]);
        s.search_query.clear();
        s.select_all_visible();
        s.remove_selected(&[id_txt]);
        assert!(!s.selected_ids.contains(&id_txt));
        s.breadcrumbs[0].1 = "A very long root workspace breadcrumb name exceeding limit".into();
    }

    // Keyboard shortcuts: Ctrl+A, F2 (when 1 mutable selected), Delete, Escape, Ctrl+C/X/V, Enter
    tap_key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
    app.apply_action(UiAction::ExplorerSelectEntry(Some(id_txt)));
    tap_key(&ctx, &mut app, Key::F2, Modifiers::NONE);
    assert!(matches!(
        app.explorer.modal,
        Some(ExplorerModal::Rename { .. })
    ));
    tap_key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(app.explorer.modal.is_none());

    tap_key(&ctx, &mut app, Key::Delete, Modifiers::NONE);
    assert!(matches!(
        app.explorer.modal,
        Some(ExplorerModal::ConfirmDelete { .. })
    ));
    tap_key(&ctx, &mut app, Key::Escape, Modifiers::NONE);

    tap_key(&ctx, &mut app, Key::C, Modifiers::COMMAND);
    assert!(app.explorer.clipboard.as_ref().is_some_and(|c| !c.is_cut));
    tap_key(&ctx, &mut app, Key::X, Modifiers::COMMAND);
    assert!(app.explorer.clipboard.as_ref().is_some_and(|c| c.is_cut));
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let _ = frame_at(&ctx, &mut app, 400.0, 560.0);
    app.apply_action(UiAction::ExplorerClearClipboard);

    // Stationary grid double-click + Rename modal Enter + busy completion wait + on_exit
    app.apply_action(UiAction::ExplorerSelectEntry(None));
    let gshapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    let alpha_pos = text_pos(&gshapes, "alpha.txt");
    for t in [1.0f64, 1.1] {
        for pressed in [true, false] {
            let evs = vec![
                Event::PointerMoved(alpha_pos),
                Event::PointerButton {
                    pos: alpha_pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ];
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 620.0))),
                time: Some(t),
                events: evs,
                ..Default::default()
            };
            ctx.run_ui(raw, |ui| app.render_root(ui))
                .textures_delta
                .clear();
        }
    }
    let _ = support::wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid));

    app.explorer.modal = Some(ExplorerModal::Rename {
        account_id: aid,
        node_id: id_txt,
        draft: "alpha2.txt".into(),
        focus_requested: false,
    });
    let _ = send_frame(&ctx, &mut app, Vec::new());
    tap_key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    let _ = support::wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid));

    for act in [
        UiAction::ExplorerCopyStreamUrl(id_vid),
        UiAction::ExplorerSyncDown(vec![id_txt]),
        UiAction::ExplorerSyncUpItems(vec![id_txt]),
        UiAction::ExplorerSyncUpAndClear(vec![id_txt]),
        UiAction::ExplorerSyncUpFiles,
        UiAction::ExplorerSyncUpFolder,
        UiAction::ExplorerOpenItem(id_dir),
        UiAction::ExplorerNavigateBreadcrumb(99),
        UiAction::ExplorerNavigateBreadcrumb(0),
        UiAction::ExplorerRefresh,
    ] {
        app.apply_action(act);
    }
    let _ = support::wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid));

    app.apply_action(UiAction::ToggleExplorerFolderCache);
    app.apply_action(UiAction::ToggleExplorerFolderCache);
    app.apply_action(UiAction::PurgeCache);
    <ControlCenterApp as eframe::App>::on_exit(&mut app, None);
    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn settings_vault_errors_ledger_reorder_and_modals_render() {
    let tmp = std::env::temp_dir().join(format!("od-ui-modal-cov-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let store = Store::open_in_memory().unwrap();
    let eng = Arc::new(SyncEngine::new(store));
    let cfg = Settings {
        docs_mode: DocsMode::ExportOffice,
        open_with_rules: vec![OpenWithRule {
            pattern: "mp4".into(),
            app_command: "vlc".into(),
        }],
        ..Settings::default()
    };
    let (bridge, _rx) = Bridge::new_with_engine(cfg, eng, None);
    let mut ids = Vec::new();
    for (n, ro, st) in [
        ("One", true, AccountStatus::Paused),
        ("Two", false, AccountStatus::Error),
    ] {
        let id = bridge
            .add_account(n.into(), format!("{n}@x.com"), n.into(), ro)
            .unwrap()
            .accounts
            .last()
            .unwrap()
            .id;
        bridge
            .controller_handle()
            .lock()
            .unwrap()
            .accounts
            .get_account_mut(id)
            .unwrap()
            .status = st;
        ids.push(id);
    }
    let aid1 = ids[0];

    let missing_file = tmp.join("missing.odcred");
    let unwritable = tmp.join("subdir");
    std::fs::create_dir_all(&unwritable).unwrap();
    let save_path = Arc::new(Mutex::new(Some(unwritable.clone())));
    let open_path = Arc::new(Mutex::new(Some(missing_file.clone())));
    let ep = AuthEndpoints {
        vault_save_picker: Arc::new(move || save_path.lock().unwrap().take()),
        vault_open_picker: Arc::new(move || open_path.lock().unwrap().clone()),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ep);
    app.apply_action(UiAction::SetBandwidth(Some(BANDWIDTH_5M)));
    app.apply_action(UiAction::StartEditAlias(aid1.to_string()));
    app.apply_action(UiAction::AccountAction {
        action: "move_up",
        id: Some("bad-uuid".into()),
    });
    assert_eq!(app.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Error));

    // Export write-to-dir error & weak password error & import missing file error
    app.vault_draft.password = "weak".into();
    app.vault_draft.confirm_password = "weak".into();
    app.apply_action(UiAction::ExportVault);
    app.vault_draft.password = "StrongVault9!".into();
    app.vault_draft.confirm_password = "StrongVault9!".into();
    app.apply_action(UiAction::ExportVault);
    assert!(app.vault_draft.feedback_is_err);
    app.apply_action(UiAction::ExportVault); // picker returns None
    app.apply_action(UiAction::ImportVault); // missing file
    std::fs::write(&missing_file, b"corrupt").unwrap();
    app.apply_action(UiAction::ImportVault); // invalid vault payload
    assert!(app.vault_draft.feedback_is_err);

    // Narrow Dashboard (<560px) with inline alias editor + release_checking + Settings (<420px) + no-match OpenWith filter
    app.tray_active = true;
    let ctx = egui::Context::default();
    let _ = frame_at(&ctx, &mut app, 520.0, 620.0);
    app.apply_action(UiAction::SaveAlias);
    app.settings_open = true;
    app.release_checking = true;
    app.open_rule_draft.editor_open = true;
    app.open_rule_draft.search_query = "nomatch-xyz".into();
    let _ = frame_at(&ctx, &mut app, 380.0, 480.0);
    let _ = std::fs::remove_dir_all(&tmp);
}
