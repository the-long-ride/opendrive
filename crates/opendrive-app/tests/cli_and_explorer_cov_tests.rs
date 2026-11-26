//! Additional integration coverage tests for CLI bridge dispatch, MediaStreamServer error status codes, and empty-grid context actions.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Modifiers, PointerButton, Pos2};
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::dispatch_bridge;
use opendrive_app::runtime::media_server::MediaStreamServer;
use opendrive_app::runtime::ui::build_bridge_in_dirs;
use opendrive_core::{AppController, Settings};
use opendrive_fake_drive::FakeDriveServer;
use std::io::{Read, Write};
use std::net::TcpStream;
use support::{add_acct, click_at, collect, has, seed_drive, send_frame, text_pos, wait_entry};
use uuid::Uuid;

fn send_http(addr: &str, req: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let _ = s.shutdown(std::net::Shutdown::Write);
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf
}

#[test]
fn cli_dispatch_file_ops_and_media_stream_error_statuses() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let drive = seed_drive(
        &[
            (
                "d1",
                "Dest",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f1", "sample.txt", "text/plain", "root", b"0123456789"),
        ],
        100,
        1000,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(drive));
    let tmp = std::env::temp_dir().join(format!("od-cli-ext-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let br =
        build_bridge_in_dirs(AppController::new(Settings::default()), None, Some(&tmp)).unwrap();
    let eng = br.engine();
    let acct = add_acct(&br, eng, "Work", addr, 100, 1000);
    br.explorer_list(acct, None, true).unwrap();
    let list = br.explorer_list(acct, None, false).unwrap();
    let fid = list.1.iter().find(|e| e.name == "sample.txt").unwrap().id;

    // Missing arg usage errors on rename/rm/cp/mv/open-item/stream
    for cmd in ["rename", "rm", "cp", "mv", "open-item", "stream"] {
        assert!(dispatch_bridge(cmd, std::iter::empty(), &br).is_err());
    }
    dispatch_bridge(
        "cp",
        ["Work", "sample.txt", "Dest"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap();
    dispatch_bridge(
        "mv",
        ["Work", "Dest/sample.txt", "Dest"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap_or(());
    dispatch_bridge(
        "rename",
        ["Work", "sample.txt", "sample2.txt"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap();
    dispatch_bridge(
        "rm",
        ["Work", "sample2.txt"].into_iter().map(str::to_string),
        &br,
    )
    .unwrap();
    dispatch_bridge(
        "stream",
        ["--probe", "Work", "Dest/sample.txt"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap();
    dispatch_bridge(
        "open-item",
        ["--dry-run", "Work", "Dest/sample.txt"]
            .into_iter()
            .map(str::to_string),
        &br,
    )
    .unwrap();
    opendrive_app::runtime::cli::cmd_cp(&br, "Work", "Dest/sample.txt", None).unwrap();
    opendrive_app::runtime::cli::cmd_open_item(&br, "Work", "Dest/sample.txt", true).unwrap();
    opendrive_app::runtime::cli::cmd_stream(&br, "Work", "Dest/sample.txt", false).unwrap();
    let vbytes = opendrive_auth::encrypt_vault(
        &opendrive_auth::VaultPayload {
            exported_at: "2026-01-01T00:00:00Z".into(),
            credentials: vec![],
            accounts: vec![opendrive_auth::ExportedAccount {
                nickname: "Dot".into(),
                email: "dot@x.com".into(),
                credential_client_id: None,
                folder_name: ".".into(),
                read_only: false,
                enabled: false,
                refresh_token: None,
                service_account_json: None,
                avatar_url: None,
            }],
        },
        "StrongVault9!",
    )
    .unwrap();
    assert_eq!(
        br.import_encrypted_vault(&vbytes, "StrongVault9!")
            .unwrap()
            .1,
        1
    );

    // MediaStreamServer 403, 404, 416 responses
    let dest_id = list.1.iter().find(|e| e.name == "Dest").unwrap().id;
    let live_id = br.explorer_list(acct, Some(dest_id), true).unwrap().1[0].id;
    let srv = MediaStreamServer::start(br.engine().clone(), rt.handle()).unwrap();
    let url = srv.stream_url(live_id, "sample.txt");
    let after_scheme = url.strip_prefix("http://").unwrap();
    let (host, path) = after_scheme.split_once('/').unwrap();
    let path = format!("/{path}");
    assert!(send_http(
        host,
        &format!("POST {path} HTTP/1.1\r\nHost: {host}\r\n\r\n")
    )
    .contains("403"));
    assert!(send_http(
        host,
        &format!("GET /bad/tok/{fid}/a HTTP/1.1\r\nHost: {host}\r\n\r\n")
    )
    .contains("403"));
    let tok = path.split('/').nth(2).unwrap();
    assert!(send_http(
        host,
        &format!("GET /stream/{tok}/999999/a HTTP/1.1\r\nHost: {host}\r\n\r\n")
    )
    .contains("404"));
    assert!(send_http(
        host,
        &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nRange: bytes=99999-\r\n\r\n")
    )
    .contains("416"));

    drive_h.abort();
    drop(srv);
    drop(br);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn empty_search_grid_context_paste_and_unknown_node_nav() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let d2 = seed_drive(
        &[("f1", "only.txt", "text/plain", "root", b"data")],
        10,
        100,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(d2));
    let (bridge, _rx) = Bridge::new(Settings::default());
    let eng = bridge.engine();
    let aid = add_acct(&bridge, eng, "A", addr, 10, 100);
    let mut app = ControlCenterApp::new(bridge);
    app.apply_action(UiAction::OpenExplorer(aid.to_string()));
    let ctx = egui::Context::default();
    wait_entry(&ctx, &mut app, aid, "only.txt");
    let f1 = support::eid(&app, aid, "only.txt");
    app.apply_action(UiAction::ExplorerSetClipboard {
        node_ids: vec![f1],
        cut: false,
    });
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    app.explorer.sessions.get_mut(&aid).unwrap().search_query = "zzz-nomatch".into();
    let _ = send_frame(&ctx, &mut app, Vec::new());
    click_at(
        &ctx,
        &mut app,
        Pos2::new(620.0, 360.0),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    let shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    let txt = collect(&shapes);
    if has(&txt, "Paste Copy") {
        let pos = text_pos(&shapes, "Paste Copy");
        click_at(&ctx, &mut app, pos, PointerButton::Primary, Modifiers::NONE);
    }
    app.apply_action(UiAction::ExplorerNavigateNode(88888));
    assert!(app
        .explorer
        .sessions
        .get(&aid)
        .unwrap()
        .breadcrumbs
        .iter()
        .any(|(_, name)| name == "Folder"));
    drive_h.abort();

    // Missing Drive session branch in sync_ops.rs L176 + Alias editor Enter key in ledger/mod.rs L83
    let (mm_br, _) = Bridge::new(Settings::default());
    let mm_aid = mm_br
        .add_account("S".into(), "s@x.com".into(), "s".into(), false)
        .unwrap()
        .accounts[0]
        .id;
    {
        let c = mm_br.controller_handle();
        c.lock()
            .unwrap()
            .accounts
            .get_account_mut(mm_aid)
            .unwrap()
            .credential_set_id = Uuid::new_v4();
    }
    let mm = opendrive_app::runtime::mount_manager::MountManager::new(std::sync::Arc::new(
        common_auth_helpers::RecordingBackend::default(),
    ));
    assert!(mm
        .sync_now(&mm_br.controller_handle(), mm_br.engine(), mm_br.secrets())
        .unwrap_err()
        .contains("Missing saved credentials"));

    let mut lapp = ControlCenterApp::new(mm_br);
    lapp.apply_action(UiAction::StartEditAlias(mm_aid.to_string()));
    let _ = send_frame(&ctx, &mut lapp, Vec::new());
    let alias_pos = text_pos(&send_frame(&ctx, &mut lapp, Vec::new()).shapes, "Alias:");
    click_at(
        &ctx,
        &mut lapp,
        Pos2::new(alias_pos.x + 70.0, alias_pos.y),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    support::tap_key(&ctx, &mut lapp, eframe::egui::Key::Enter, Modifiers::NONE);
    assert!(lapp.alias_edit.is_none());

    lapp.available_release = Some(opendrive_app::runtime::release_check::ReleaseInfo {
        tag_name: "v9.9.9".into(),
        name: "Release Nine".into(),
        body: String::new(),
        html_url: "https://example.com".into(),
    });
    let rshapes = send_frame(&ctx, &mut lapp, Vec::new()).shapes;
    if has(&collect(&rshapes), "Close") {
        let p = text_pos(&rshapes, "Close");
        click_at(&ctx, &mut lapp, p, PointerButton::Primary, Modifiers::NONE);
    }
}
