//! Widget-click coverage for panels zoom/toast, release/settings modals, drawer/ledger rows, and Explorer.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::desktop::{
    ControlCenterApp, ExplorerClipboard, ExplorerModal, ExplorerSortColumn, ExplorerViewMode,
    NoticeTone, UiAction, UiNotice,
};
use opendrive_app::runtime::{
    auth_runner::AuthEndpoints, bridge::Bridge, release_check::ReleaseInfo,
};
use opendrive_core::{AccountStatus, Settings};
use opendrive_fake_drive::FakeDriveServer;
use std::{sync::Arc, time::Duration};
use support::{add_acct, seed_drive, tap_key, wait_entry};
use uuid::Uuid;

fn frame_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    sz: Vec2,
    events: Vec<Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, sz)),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out.shapes
}

fn hit(ctx: &egui::Context, app: &mut ControlCenterApp, sz: Vec2, pos: Pos2) {
    for p in [true, false] {
        let btn = Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: p,
            modifiers: Modifiers::NONE,
        };
        let _ = frame_at(ctx, app, sz, vec![Event::PointerMoved(pos), btn]);
    }
}

fn walk_shapes(s: &Shape, y0: f32, y1: f32, txts: &mut Vec<(String, Rect)>, pills: &mut Vec<Rect>) {
    match s {
        Shape::Text(t) => {
            let r = t.galley.rect.translate(t.pos.to_vec2());
            if r.top() >= y0 && r.bottom() <= y1 {
                txts.push((t.galley.text().to_owned(), r));
            }
        }
        Shape::Rect(r)
            if r.stroke.width > 0.0
                && (r.rect.height() - 28.0).abs() < 0.5
                && r.rect.top() >= y0
                && r.rect.bottom() <= y1 =>
        {
            pills.push(r.rect)
        }
        Shape::Vec(v) => v.iter().for_each(|x| walk_shapes(x, y0, y1, txts, pills)),
        _ => {}
    }
}

fn scan(
    shapes: &[egui::epaint::ClippedShape],
    y0: f32,
    y1: f32,
) -> (Vec<(String, Rect)>, Vec<Rect>) {
    let (mut txts, mut pills) = (Vec::new(), Vec::new());
    shapes
        .iter()
        .for_each(|c| walk_shapes(&c.shape, y0, y1, &mut txts, &mut pills));
    pills.sort_by(|a, b| a.left().total_cmp(&b.left()));
    (txts, pills)
}

fn clk(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    sz: Vec2,
    needle: &str,
    min_y: f32,
) -> bool {
    let _ = frame_at(ctx, app, sz, vec![]);
    scan(&frame_at(ctx, app, sz, vec![]), min_y, 9999.0)
        .0
        .into_iter()
        .rev()
        .find(|(t, _)| t == needle)
        .is_some_and(|(_, r)| {
            hit(ctx, app, sz, r.center());
            true
        })
}

#[test]
fn panels_zoom_ledger_row_drawer_explorer_and_modals_widget_clicks() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = std::env::temp_dir().join(format!("od-ui-w2-{}", Uuid::new_v4()));
    let save_p = tmp.join("v.odcred");
    let (tok_url, tok_srv) = rt.block_on(common_auth_helpers::mock_token_server(
        r#"{"error":"denied"}"#,
    ));
    let drive = seed_drive(&[("f1", "doc.txt", "text/plain", "root", b"hi")], 10, 1000);
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(drive));
    let (bridge, _rx) = Bridge::new(Settings::default());
    let ep = AuthEndpoints {
        auth_url: "http://127.0.0.1:9/auth".into(),
        token_url: tok_url.clone(),
        browser_opener: Arc::new(|_| Ok(())),
        folder_opener: Arc::new(|_| Ok(())),
        vault_save_picker: Arc::new(move || Some(save_p.clone())),
        vault_open_picker: Arc::new(|| None),
        upload_files_picker: Arc::new(Vec::new),
        upload_folder_picker: Arc::new(|| None),
        app_exe_picker: Arc::new(|| None),
        item_launcher: Arc::new(|_, _| Ok(())),
        ..AuthEndpoints::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep);
    let (ctx, sz) = (egui::Context::default(), Vec2::new(1080.0, 900.0));

    // Empty explorer (0 accounts) + Panels zoom & toast Close + Release dialog Later
    app.explorer.open = true;
    let _ = frame_at(&ctx, &mut app, sz, vec![]);
    app.explorer.open = false;

    for k in [Key::Plus, Key::Minus, Key::Num0] {
        tap_key(&ctx, &mut app, k, Modifiers::COMMAND);
    }
    assert!((ctx.zoom_factor() - 1.0).abs() < 0.01);
    app.notice = Some(UiNotice {
        tone: NoticeTone::Error,
        message: "err".into(),
    });
    let _ = frame_at(&ctx, &mut app, sz, vec![]);
    let (txts, pills) = scan(&frame_at(&ctx, &mut app, sz, vec![]), 0.0, 9999.0);
    let retry = txts.into_iter().find(|(t, _)| t == "Retry").unwrap().1;
    let close = pills
        .into_iter()
        .find(|p| p.left() > retry.right() && (p.center().y - retry.center().y).abs() < 12.0)
        .unwrap();
    hit(&ctx, &mut app, sz, close.center());
    assert!(app.notice.is_none());
    app.available_release = Some(ReleaseInfo {
        tag_name: "v9.9.9".into(),
        name: "9.9.9".into(),
        html_url: "https://ex.test".into(),
        body: "b".into(),
    });
    assert!(clk(&ctx, &mut app, sz, "Later", 0.0) && app.available_release.is_none());

    let c = |a: &mut ControlCenterApp, n: &str| clk(&ctx, a, sz, n, 0.0);
    (app.drawer_open, app.draft.read_only) = (true, true);
    assert!(c(&mut app, "Import Backup"));
    (app.settings_open, app.drawer_open) = (false, true);
    assert!(c(&mut app, "Service Account JSON (No Login)") && c(&mut app, "Browser Sign-In"));
    (app.draft.client_id, app.draft.client_secret) = ("cid".into(), "sec".into());
    assert!(c(&mut app, "Copy Link"));
    app.apply_action(UiAction::CancelAuth);
    assert!(c(&mut app, "Service Account JSON (No Login)"));
    app.draft.service_account_input = common_auth_helpers::sample_service_account_json(&tok_url);
    assert!(c(&mut app, "Connect Service Account"));
    for _ in 0..40 {
        let _ = frame_at(&ctx, &mut app, sz, vec![]);
        if app.form_error.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    (app.drawer_open, app.settings_open) = (false, true);
    (app.vault_draft.feedback, app.vault_draft.feedback_is_err) = (Some("Saved".into()), false);
    for l in ["Show", "Export (.odcred)", "Import (.odcred)"] {
        clk(&ctx, &mut app, sz, l, 0.0);
    }
    app.settings_open = false;

    // 2 accounts: Explorer sidebar/toolbar/grid/modals
    let a1 = add_acct(&bridge, bridge.engine(), "A1", addr, 10, 1000);
    let _a2 = add_acct(&bridge, bridge.engine(), "A2", addr, 10, 1000);
    app.refresh();
    app.apply_action(UiAction::OpenExplorer(a1.to_string()));
    wait_entry(&ctx, &mut app, a1, "doc.txt");
    let nid = app.explorer.active_session().unwrap().entries[0].id;
    assert!(clk(&ctx, &mut app, sz, "New Account", 0.0) && app.drawer_open);
    (app.drawer_open, app.explorer.open) = (false, true);
    assert!(clk(&ctx, &mut app, sz, "Dashboard", 0.0) && !app.explorer.open);
    app.explorer.open = true;
    assert!(
        clk(&ctx, &mut app, Vec2::new(520.0, 700.0), "A2", 60.0)
            && clk(&ctx, &mut app, sz, "A1", 100.0)
    );
    clk(&ctx, &mut app, sz, "Search folder...", 40.0);
    scan(&frame_at(&ctx, &mut app, sz, vec![]), 56.0, 130.0)
        .1
        .into_iter()
        .filter(|p| p.width() < 40.0)
        .for_each(|p| hit(&ctx, &mut app, sz, p.center()));
    app.apply_action(UiAction::ExplorerToggleSort(ExplorerSortColumn::Name));
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let _ = frame_at(&ctx, &mut app, Vec2::new(360.0, 700.0), vec![]);
    app.apply_action(UiAction::ExplorerSelectAll);
    tap_key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    app.apply_action(UiAction::ExplorerPasteClipboard(None));
    app.explorer.clipboard = Some(ExplorerClipboard {
        account_id: Uuid::new_v4(),
        node_ids: vec![nid],
        name: "x".into(),
        is_cut: false,
    });
    app.apply_action(UiAction::ExplorerPasteClipboard(None));
    for (draft, click) in [("  ", false), ("doc2.txt", true)] {
        app.explorer.modal = Some(ExplorerModal::Rename {
            account_id: a1,
            node_id: nid,
            draft: draft.into(),
            focus_requested: true,
        });
        if click {
            assert!(clk(&ctx, &mut app, sz, "Save", 0.0));
        } else {
            app.apply_action(UiAction::ExplorerSaveRename);
        }
    }
    for (ids, click) in [(vec![], false), (vec![nid], true)] {
        app.explorer.modal = Some(ExplorerModal::ConfirmDelete {
            account_id: a1,
            node_ids: ids,
            name: "x".into(),
            is_folder: false,
        });
        if click {
            assert!(clk(&ctx, &mut app, sz, "Delete", 0.0));
        } else {
            app.apply_action(UiAction::ExplorerConfirmDelete);
        }
    }
    app.explorer.open = false;

    // Ledger row widget clicks
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: false,
        copy_link: false,
    });
    app.drawer_open = false;
    for pending in [true, false] {
        if !pending {
            app.apply_action(UiAction::CancelAuth);
        }
        for lbl in ["Sign In", "Copy Link"] {
            app.reauth_account_id = pending.then_some(a1);
            app.state.accounts[0].status = AccountStatus::NeedsReauth;
            app.state.accounts[0].credential_set_id = Uuid::new_v4();
            clk(&ctx, &mut app, sz, lbl, 240.0);
        }
    }
    app.drawer_open = false;
    let _ = frame_at(&ctx, &mut app, sz, vec![]);
    let sync_r = scan(&frame_at(&ctx, &mut app, sz, vec![]), 240.0, 9999.0)
        .0
        .into_iter()
        .find(|(t, _)| t == "Sync On" || t == "Paused")
        .unwrap()
        .1;
    let sw = Pos2::new(sync_r.left() - 20.0, sync_r.center().y);
    hit(&ctx, &mut app, sz, sw);
    hit(&ctx, &mut app, sz, sw);
    assert!(clk(&ctx, &mut app, sz, "Open", 240.0) && clk(&ctx, &mut app, sz, "Explore", 240.0));
    app.explorer.open = false;
    let (txts, pills) = scan(&frame_at(&ctx, &mut app, sz, vec![]), 240.0, 9999.0);
    let rem = txts.into_iter().find(|(t, _)| t == "Remove").unwrap().1;
    pills
        .into_iter()
        .filter(|p| {
            p.top() >= rem.top() - 10.0
                && p.bottom() <= rem.bottom() + 10.0
                && p.left() > rem.right()
                && p.width() < 36.0
        })
        .for_each(|p| hit(&ctx, &mut app, sz, p.center()));
    assert!(clk(&ctx, &mut app, sz, "Remove", 240.0) && clk(&ctx, &mut app, sz, "Connect", 200.0));
    assert!(clk(&ctx, &mut app, sz, "Close", 200.0));
    tok_srv.abort();
    drive_h.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
