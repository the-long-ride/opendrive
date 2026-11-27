//! Final UI interaction and CLI coverage tests for Settings, Release, Ledger/Drawer, Explorer toolbar/sidebar, and CLI aliases.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Modifiers, PointerButton};
use opendrive_app::design::formatters::ConnectAuthMode;
use opendrive_app::desktop::{ControlCenterApp, ExplorerModal, ExplorerViewMode, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::{cmd_explore, dispatch_bridge, list_root_with_base};
use opendrive_app::runtime::release_check::ReleaseInfo;
use opendrive_app::runtime::ui::build_bridge_in_dirs;
use opendrive_core::{AccountStatus, AppController, Settings};
use opendrive_fake_drive::FakeDriveServer;
use std::sync::{Arc, Mutex};
use support::{add_acct, click_at, collect, has, seed_drive, send_frame, text_pos, wait_entry};
use uuid::Uuid;

fn click_if(ctx: &egui::Context, app: &mut ControlCenterApp, label: &str) -> bool {
    let _ = send_frame(ctx, app, Vec::new());
    let shapes = send_frame(ctx, app, Vec::new()).shapes;
    if has(&collect(&shapes), label) {
        let p = text_pos(&shapes, label);
        click_at(ctx, app, p, PointerButton::Primary, Modifiers::NONE);
        true
    } else {
        false
    }
}

#[test]
fn settings_release_ledger_drawer_and_explorer_button_clicks() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = seed_drive(
        &[
            (
                "d1",
                "FolderA",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f1", "note.txt", "text/plain", "root", b"hello"),
        ],
        1024,
        4096,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let oref = Arc::clone(&opened);
    let (bridge, _rx) = Bridge::new(Settings::default());
    let aid1 = add_acct(&bridge, bridge.engine(), "Main", addr, 1024, 4096);
    let aid2 = bridge
        .add_account(
            "Reauth".into(),
            "r@example.com".into(),
            "reauth".into(),
            true,
        )
        .unwrap()
        .accounts
        .last()
        .unwrap()
        .id;
    {
        let c = bridge.controller_handle();
        let mut g = c.lock().unwrap();
        let cred = opendrive_core::CredentialSet {
            id: Uuid::new_v4(),
            label: "G".into(),
            client_id: "cid".into(),
        };
        let cid = cred.id;
        g.accounts.add_credential(cred);
        let a2 = g.accounts.get_account_mut(aid2).unwrap();
        a2.credential_set_id = cid;
        a2.status = AccountStatus::NeedsReauth;
    }

    let ep = AuthEndpoints {
        browser_opener: Arc::new(move |u| {
            oref.lock().unwrap().push(u.to_string());
            Ok(())
        }),
        folder_opener: Arc::new(|_| Ok(())),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ep);
    let ctx = egui::Context::default();

    // Dashboard / Ledger / Drawer interactions
    let _ = send_frame(&ctx, &mut app, Vec::new());
    click_if(&ctx, &mut app, "Open");
    click_if(&ctx, &mut app, "Sign In");
    click_if(&ctx, &mut app, "Copy Link");
    app.apply_action(UiAction::StartEditAlias(aid1.to_string()));
    click_if(&ctx, &mut app, "Cancel");
    assert!(app.alias_edit.is_none());

    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.draft.auth_mode = ConnectAuthMode::ServiceAccount;
    let _ = send_frame(&ctx, &mut app, Vec::new());
    click_if(&ctx, &mut app, "Encrypted Vault Backup");
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    app.apply_action(UiAction::StartAddOpenWithRule);
    app.open_rule_draft.pattern = "txt".into();
    app.open_rule_draft.app_command = "notepad".into();
    app.apply_action(UiAction::SaveOpenWithRule);
    let _ = send_frame(&ctx, &mut app, Vec::new());
    click_if(&ctx, &mut app, "Link (.gdoc)");
    click_if(&ctx, &mut app, "ExportOffice");
    click_if(&ctx, &mut app, "Check Now");
    click_if(&ctx, &mut app, "Edit");
    click_if(&ctx, &mut app, "Cancel");
    click_if(&ctx, &mut app, "Remove");
    click_if(&ctx, &mut app, "Setup Guide");
    app.settings_open = false;
    app.guide_open = false;

    // Release modal button clicks
    app.available_release = Some(ReleaseInfo {
        tag_name: "v9.9.9".into(),
        name: "Nine".into(),
        body: "Notes".into(),
        html_url: "https://example.com/r".into(),
    });
    assert!(click_if(&ctx, &mut app, "Open Release Page"));
    app.available_release = Some(ReleaseInfo {
        tag_name: "v9.9.9".into(),
        name: "Nine".into(),
        body: "Notes".into(),
        html_url: "https://example.com/r".into(),
    });
    assert!(click_if(&ctx, &mut app, "Never Show This Version"));
    assert!(app.available_release.is_none());

    // Explorer toolbar, Grid click, Delete confirm modal Cancel + Confirm
    app.apply_action(UiAction::OpenExplorer(aid1.to_string()));
    wait_entry(&ctx, &mut app, aid1, "note.txt");
    let f1 = support::eid(&app, aid1, "note.txt");
    app.apply_action(UiAction::ExplorerSelectEntry(Some(f1)));
    let _ = send_frame(&ctx, &mut app, Vec::new());
    click_if(&ctx, &mut app, "Rename");
    click_if(&ctx, &mut app, "Cancel");
    app.explorer.modal = Some(ExplorerModal::ConfirmDelete {
        account_id: aid1,
        node_ids: vec![f1],
        name: "note.txt".into(),
        is_folder: false,
    });
    click_if(&ctx, &mut app, "Delete");
    let _ = support::wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    assert!(click_if(&ctx, &mut app, "New Account"));
    assert!(app.explorer.open && app.drawer_open);

    drive_h.abort();
}

#[test]
fn cli_move_copy_delete_aliases_and_list_root_error() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt
        .block_on(list_root_with_base("t", "http://127.0.0.1:1"))
        .is_err());

    let drive = seed_drive(
        &[
            (
                "d1",
                "Dest",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f1", "doc.txt", "text/plain", "root", b"data"),
        ],
        50,
        500,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));
    let tmp = std::env::temp_dir().join(format!("od-cli-alias-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let br =
        build_bridge_in_dirs(AppController::new(Settings::default()), None, Some(&tmp)).unwrap();
    let acct = add_acct(&br, br.engine(), "Work", addr, 50, 500);
    br.explorer_list(acct, None, true).unwrap();

    dispatch_bridge(
        "move",
        ["Work", "doc.txt", "Dest"].into_iter().map(str::to_string),
        &br,
    )
    .unwrap();
    cmd_explore(&br, "Work", Some("Dest")).unwrap();
    dispatch_bridge(
        "copy",
        ["Work", "Dest/doc.txt", "Dest"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap();
    dispatch_bridge(
        "delete",
        ["Work", "Dest/doc.txt"].into_iter().map(str::to_string),
        &br,
    )
    .unwrap();
    // Subfolder copy without dest_folder + error branches across CLI helpers.
    drive.add_file(opendrive_fake_drive::FakeFile {
        id: "f2".into(),
        name: "clip.mp4".into(),
        mime_type: "video/mp4".into(),
        size: 4,
        parents: vec!["d1".into()],
        trashed: false,
        content: b"1234".to_vec(),
        head_revision_id: Some("r1".into()),
    });
    br.explorer_list(acct, None, true).unwrap();
    let d1_id = opendrive_app::runtime::cli::resolve_remote_node(&br, acct, "Dest").unwrap();
    br.explorer_list(acct, Some(d1_id), true).unwrap();
    dispatch_bridge(
        "cp",
        ["Work", "Dest/clip.mp4"].into_iter().map(str::to_string),
        &br,
    )
    .unwrap();
    dispatch_bridge(
        "mv",
        ["Work", "Dest/clip.mp4", "Dest"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap();
    assert!(opendrive_app::runtime::cli::dispatch("help", std::iter::empty()).is_ok());
    assert!(opendrive_app::runtime::cli::dispatch("unknown-cmd", std::iter::empty()).is_ok());
    assert!(opendrive_app::runtime::cli::dispatch("list-root", std::iter::empty()).is_err());
    for (c, args) in [
        ("explore", vec!["missing-acct"]),
        ("explore", vec!["Work", "no/such/path"]),
        ("sync-down", vec!["Work", "no/such/file"]),
        ("sync-up", vec!["Work", "/no/such/local/file-xyz"]),
        ("sync-clear", vec!["Work", "no/such/file"]),
        ("open-item", vec!["Work", "no/such/file"]),
        ("stream", vec!["Work", "no/such/file"]),
        ("rename", vec!["Work", "no/such/file", "n"]),
        ("rm", vec!["Work", "no/such/file"]),
        ("cp", vec!["Work", "no/such/file"]),
        ("mv", vec!["Work", "no/such/file", "Dest"]),
    ] {
        assert!(dispatch_bridge(c, args.into_iter().map(str::to_string), &br).is_err());
    }

    drive_h.abort();
    drop(br);
    let _ = std::fs::remove_dir_all(&tmp);
}
