//! Headless integration tests for Cloud Explorer multi-selection, shortcuts, and batch actions.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton};
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, NoticeTone, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::Settings;
use opendrive_fake_drive::FakeDriveServer;
use opendrive_store::{NodeState, Store};
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use support::{
    add_acct, click_at, dirty_cached, eid, has, has_sub, seed_drive, send_frame, send_mods,
    state_of, tap_key, text_pos, texts, wait_entry, wait_until,
};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_multi_selection_shortcuts_and_batch_ops_work() {
    let tmp = std::env::temp_dir().join(format!("od-exp-msel-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();

    let (fdir, txt) = ("application/vnd.google-apps.folder", "text/plain");
    let drive = seed_drive(
        &[
            ("d-dst", "Target", fdir, "root", &[]),
            ("f-a", "alpha.txt", txt, "root", b"alpha-v1"),
            ("f-b", "beta.txt", txt, "root", b"beta-longer-v1"),
            ("f-d", "delta.txt", txt, "root", b"d1"),
            ("f-g", "gamma.txt", txt, "root", b"gamma-longest-file-v1"),
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

    let mut app = ControlCenterApp::new(bridge.clone());
    let ctx = egui::Context::default();
    let _ = texts(&ctx, &mut app);
    app.apply_action(UiAction::OpenExplorer(a_id.to_string()));
    wait_entry(&ctx, &mut app, a_id, "gamma.txt");

    assert!(app.explorer.sessions[&a_id]
        .entries
        .iter()
        .all(|e| e.name != "Shared with me"));
    let (t_id, a_nid, b_nid, d_nid, g_nid) = (
        eid(&app, a_id, "Target"),
        eid(&app, a_id, "alpha.txt"),
        eid(&app, a_id, "beta.txt"),
        eid(&app, a_id, "delta.txt"),
        eid(&app, a_id, "gamma.txt"),
    );

    // 1. Plain click, Shift+Click range, Ctrl+Click deselect anchor update, additive range.
    let (l, r, none, sh, cmd) = (
        PointerButton::Primary,
        PointerButton::Secondary,
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::COMMAND,
    );
    let click_row = |a: &mut ControlCenterApp, name: &str, btn, m| {
        let p = text_pos(&send_frame(&ctx, a, Vec::new()).shapes, name);
        click_at(&ctx, a, p, btn, m);
    };
    let sel = |a: &ControlCenterApp| a.explorer.sessions[&a_id].selected_ids.clone();

    // Plain click, Shift+Click range, Ctrl+Click deselect anchor update.
    click_row(&mut app, "alpha.txt", l, none);
    assert_eq!(sel(&app), vec![a_nid]);
    click_row(&mut app, "delta.txt", l, sh);
    assert_eq!(sel(&app), vec![a_nid, b_nid, d_nid]);
    click_row(&mut app, "beta.txt", l, cmd);
    assert_eq!(sel(&app), vec![a_nid, d_nid]);
    assert_eq!(app.explorer.sessions[&a_id].anchor_id, Some(b_nid));
    click_row(&mut app, "gamma.txt", l, sh);
    assert_eq!(sel(&app), vec![b_nid, d_nid, g_nid]);

    let sel_chip = text_pos(&send_frame(&ctx, &mut app, Vec::new()).shapes, "3 selected");
    let _ = send_frame(&ctx, &mut app, vec![Event::PointerMoved(sel_chip)]);
    let tip_shapes = send_frame(&ctx, &mut app, vec![Event::PointerMoved(sel_chip)]).shapes;
    assert!(has(
        &support::collect(&tip_shapes),
        "Clear current selection"
    ));

    click_row(&mut app, "delta.txt", r, none);
    assert_eq!(sel(&app), vec![b_nid, d_nid, g_nid]);
    tap_key(&ctx, &mut app, Key::Escape, none);
    click_row(&mut app, "Target", r, none);
    assert_eq!(sel(&app), vec![t_id]);
    tap_key(&ctx, &mut app, Key::Escape, none);
    tap_key(&ctx, &mut app, Key::Escape, none);
    assert!(sel(&app).is_empty());
    let idle_shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    assert!(!has_sub(&support::collect(&idle_shapes), "selected"));
    for lbl in ["Sync", "Copy", "Move", "Delete"] {
        click_at(&ctx, &mut app, text_pos(&idle_shapes, lbl), l, none);
    }
    assert!(!has(&texts(&ctx, &mut app), "Sync Down") && app.explorer.clipboard.is_none());

    // 2. Filtered Ctrl+A, Grid multi-select, Rename guard with Shared with me, search/overlay guards.
    app.explorer.active_session_mut().unwrap().search_query = "ta".into();
    let _ = texts(&ctx, &mut app);
    tap_key(&ctx, &mut app, Key::A, cmd);
    assert_eq!(sel(&app), vec![t_id, b_nid, d_nid]);
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    assert!(has(&texts(&ctx, &mut app), "3 selected"));
    app.explorer
        .active_session_mut()
        .unwrap()
        .search_query
        .clear();

    app.explorer.active_session_mut().unwrap().selected_ids = vec![t_id, a_nid];
    let multi_bar = texts(&ctx, &mut app);
    assert!(has(&multi_bar, "2 selected") && !has(&multi_bar, "Rename"));
    tap_key(&ctx, &mut app, Key::F2, none);
    assert!(app.explorer.modal.is_none());

    app.explorer.active_session_mut().unwrap().search_query = "alpha".into();
    assert_eq!(
        app.explorer.sessions[&a_id].effective_selected_ids(),
        vec![a_nid]
    );
    app.settings_open = true;
    tap_key(&ctx, &mut app, Key::Escape, none);
    assert!(!app.explorer.sessions[&a_id].selected_ids.is_empty());
    app.settings_open = false;
    app.explorer
        .active_session_mut()
        .unwrap()
        .search_query
        .clear();
    tap_key(&ctx, &mut app, Key::Escape, none);
    assert!(app.explorer.sessions[&a_id].selected_ids.is_empty());

    // 3. Batch Sync Down -> dirty edit -> Batch Sync Up -> Batch Sync Up & Clear Local.
    app.apply_action(UiAction::ExplorerSyncDown(vec![a_nid, b_nid]));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id)
            && state_of(&eng, a_nid) == NodeState::Cached
            && state_of(&eng, b_nid) == NodeState::Cached
    });
    dirty_cached(&eng, a_nid, b"alpha-v2");
    dirty_cached(&eng, b_nid, b"beta-v2");
    app.apply_action(UiAction::ExplorerSyncUpItems(vec![a_nid, b_nid]));
    wait_until(&ctx, &mut app, |a| !a.explorer.is_busy(a_id));
    assert_eq!(drive.get_file("f-a").unwrap().content, b"alpha-v2");
    assert_eq!(drive.get_file("f-b").unwrap().content, b"beta-v2");

    app.apply_action(UiAction::ExplorerSyncUpAndClear(vec![a_nid, b_nid]));
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id)
            && state_of(&eng, a_nid) == NodeState::Placeholder
            && state_of(&eng, b_nid) == NodeState::Placeholder
    });

    // 4. Read-only guard & key-repeat suppression & OS clipboard preservation.
    let set_ro = |ro| {
        let ctrl = bridge.controller_handle();
        ctrl.lock()
            .unwrap()
            .accounts
            .get_account_mut(a_id)
            .unwrap()
            .read_only = ro;
    };
    set_ro(true);
    app.state = bridge.state_snapshot();
    app.explorer
        .active_session_mut()
        .unwrap()
        .select_only(Some(a_nid));
    let ro_shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    let ro_t = support::collect(&ro_shapes);
    assert!(has(&ro_t, "Delete") && !has(&ro_t, "Rename"));
    click_at(&ctx, &mut app, text_pos(&ro_shapes, "Delete"), l, none);
    tap_key(&ctx, &mut app, Key::Delete, none);
    assert!(app.explorer.modal.is_none());
    set_ro(false);
    app.state = bridge.state_snapshot();

    // 5. Batch Copy/Move does not clobber OS clipboard; repeat Ctrl+V is ignored; partial failures reconcile.
    let key_v = |pressed| Event::Key {
        key: Key::V,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: cmd,
    };
    send_mods(&ctx, &mut app, cmd, vec![key_v(true)]);
    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![a_nid, b_nid],
        cut: false,
    });
    let clip_out = send_mods(&ctx, &mut app, cmd, vec![key_v(true)]);
    assert!(!clip_out
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyText(_))));
    assert!(!app.explorer.is_busy(a_id));
    let _ = send_frame(&ctx, &mut app, Vec::new());
    assert!(!drive
        .list_children("root")
        .iter()
        .any(|f| f.name == "alpha (copy).txt"));

    // Failed Delete preserves selection and clipboard; partial Cut+Move refreshes & prunes moved ID.
    let has_err = |a: &ControlCenterApp| {
        !a.explorer.is_busy(a_id)
            && a.notice
                .as_ref()
                .is_some_and(|n| n.tone == NoticeTone::Error)
    };
    app.explorer.active_session_mut().unwrap().selected_ids = vec![a_nid, b_nid];
    drive.set_forced_error(Some(500));
    app.apply_action(UiAction::ExplorerRequestDelete(vec![a_nid, b_nid]));
    assert!(has(&texts(&ctx, &mut app), "Delete Items"));
    app.apply_action(UiAction::ExplorerConfirmDelete);
    wait_until(&ctx, &mut app, has_err);
    assert_eq!(
        app.explorer.sessions[&a_id].selected_ids,
        vec![a_nid, b_nid]
    );
    assert!(app.explorer.clipboard.is_some());
    drive.set_forced_error(None);

    // Partial Cut+Move [alpha, Target] -> Target: alpha moves, Target self-move fails -> Target stays in clipboard.
    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![a_nid, t_id],
        cut: true,
    });
    app.apply_action(UiAction::ExplorerPasteClipboard(Some(t_id)));
    wait_until(&ctx, &mut app, has_err);
    let msg = app.notice.as_ref().unwrap().message.clone();
    assert!(has_sub(&[msg], "1 of 2 completed"));
    assert!(drive
        .list_children("d-dst")
        .iter()
        .any(|f| f.name == "alpha.txt"));
    assert_eq!(
        app.explorer.clipboard.as_ref().unwrap().node_ids,
        vec![t_id]
    );
    app.apply_action(UiAction::ExplorerClearClipboard);

    // Successful Batch Delete removes deleted items.
    app.apply_action(UiAction::ExplorerRequestDelete(vec![b_nid, d_nid]));
    app.apply_action(UiAction::ExplorerConfirmDelete);
    wait_until(&ctx, &mut app, |a| {
        !a.explorer.is_busy(a_id)
            && !a.explorer.sessions[&a_id]
                .entries
                .iter()
                .any(|e| e.id == b_nid || e.id == d_nid)
    });
    assert!(drive.get_file("f-b").unwrap().trashed && drive.get_file("f-d").unwrap().trashed);
    assert!(!drive.get_file("f-g").unwrap().trashed);

    // Releasing V, then emitting both Ctrl+V and OS Event::Paste in one frame pastes only once.
    let copy = UiAction::ExplorerSetClipboard {
        node_ids: vec![g_nid],
        cut: false,
    };
    app.apply_action(copy);
    send_mods(&ctx, &mut app, cmd, vec![key_v(false)]);
    send_mods(
        &ctx,
        &mut app,
        cmd,
        vec![key_v(true), Event::Paste("os-text".into())],
    );
    wait_until(&ctx, &mut app, |a| !a.explorer.is_busy(a_id));
    let n_copies = drive
        .list_children("root")
        .into_iter()
        .filter(|f| f.name.starts_with("gamma ("))
        .count();
    assert_eq!(n_copies, 1);

    h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
