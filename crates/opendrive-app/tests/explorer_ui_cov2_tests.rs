//! UI interaction coverage tests for Explorer toolbar menus, Open Rules validation, Settings vault buttons, and account row actions.

mod common_auth_helpers;
mod support;

use eframe::egui::{self, Modifiers, PointerButton};
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::Settings;
use opendrive_fake_drive::FakeDriveServer;
use std::sync::{Arc, Mutex};
use support::{
    add_acct, click_at, collect, eid, has, seed_drive, send_frame, text_pos, wait_entry, wait_until,
};

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
fn explorer_menus_open_rules_validation_and_vault_ui_buttons() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = std::env::temp_dir().join(format!("od-ui2-cov-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let up_file = tmp.join("upload.txt");
    let up_dir = tmp.join("updir");
    std::fs::create_dir_all(&up_dir).unwrap();
    std::fs::write(&up_file, b"hello").unwrap();
    std::fs::write(up_dir.join("in.txt"), b"dir").unwrap();

    let drive = seed_drive(
        &[
            ("f1", "clip.mp4", "video/mp4", "root", b"0123"),
            ("f2", "readme.txt", "text/plain", "root", b"abcd"),
        ],
        100,
        500,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(drive));
    let (bridge, _rx) = Bridge::new(Settings::default());
    let aid1 = add_acct(&bridge, bridge.engine(), "Main", addr, 100, 500);
    let _aid2 = bridge
        .add_account("Second".into(), "s@example.com".into(), "tok2".into(), true)
        .unwrap();

    let up_file_ref = up_file.clone();
    let up_dir_ref = up_dir.clone();
    let vault_ref = tmp.join("backup.odcred");
    let launched = Arc::new(Mutex::new(Vec::<String>::new()));
    let lref = Arc::clone(&launched);
    let ep = AuthEndpoints {
        upload_files_picker: Arc::new(move || vec![up_file_ref.clone()]),
        upload_folder_picker: Arc::new(move || Some(up_dir_ref.clone())),
        app_exe_picker: Arc::new(|| Some(r"C:\Tools\vlc.exe".into())),
        vault_save_picker: Arc::new({
            let v = vault_ref.clone();
            move || Some(v.clone())
        }),
        vault_open_picker: Arc::new(move || Some(vault_ref.clone())),
        item_launcher: Arc::new(move |_cmd, tgt| {
            lref.lock().unwrap().push(tgt.to_string());
            Ok(())
        }),
        folder_opener: Arc::new(|_| Ok(())),
        browser_opener: Arc::new(|_| Ok(())),
        ..Default::default()
    };

    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ep);
    let ctx = egui::Context::default();

    // Ledger reorder chevrons + Alias save on Main account.
    app.apply_action(UiAction::StartEditAlias(aid1.to_string()));
    app.alias_edit.as_mut().unwrap().nickname = "Primary".into();
    assert!(click_if(&ctx, &mut app, "Save"));
    assert_eq!(app.state.accounts[0].nickname, "Primary");

    // Settings modal + Open Rules Add -> empty Save error -> Browse -> Save, plus Vault Show/Export/Import.
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    app.vault_draft.password = "Abcdef12!".into();
    app.vault_draft.confirm_password = "Abcdef12!".into();
    app.apply_action(UiAction::StartAddOpenWithRule);
    app.apply_action(UiAction::SaveOpenWithRule);
    assert!(app.open_rule_draft.error.is_some());
    app.apply_action(UiAction::BrowseOpenWithRuleApp);
    assert_eq!(app.open_rule_draft.app_command, r"C:\Tools\vlc.exe");
    app.open_rule_draft.pattern = "mp4".into();
    app.apply_action(UiAction::SaveOpenWithRule);
    assert!(!app.state.settings.open_with_rules.is_empty());
    app.apply_action(UiAction::ToggleVaultSecret);
    app.apply_action(UiAction::ExportVault);
    app.apply_action(UiAction::ImportVault);
    app.settings_open = false;

    // Explorer Upload dropdown -> Upload Files & Upload Folder.
    app.apply_action(UiAction::OpenExplorer(aid1.to_string()));
    wait_entry(&ctx, &mut app, aid1, "clip.mp4");
    assert!(click_if(&ctx, &mut app, "Upload"));
    assert!(click_if(&ctx, &mut app, "Upload Files"));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));
    assert!(click_if(&ctx, &mut app, "Upload"));
    assert!(click_if(&ctx, &mut app, "Upload Folder"));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));

    // Select video clip.mp4 -> Copy Stream URL, Sync dropdown (Sync Down, Sync Up, Sync Up & Clear Local), Copy -> Paste Copy.
    let f_vid = eid(&app, aid1, "clip.mp4");
    app.apply_action(UiAction::ExplorerSelectEntry(Some(f_vid)));
    let _ = send_frame(&ctx, &mut app, Vec::new());
    let _ = click_if(&ctx, &mut app, "Copy Stream URL");
    for label in ["Sync Down", "Sync Up", "Sync Up & Clear Local"] {
        assert!(click_if(&ctx, &mut app, "Sync"));
        let _ = click_if(&ctx, &mut app, label);
        let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));
    }
    assert!(click_if(&ctx, &mut app, "Copy"));
    assert!(click_if(&ctx, &mut app, "Paste Copy Here"));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));

    // Switch to Grid view and exercise quick double-click detection + empty area click.
    app.apply_action(UiAction::ExplorerSetViewMode(ExplorerViewMode::Grid));
    let _ = send_frame(&ctx, &mut app, Vec::new());
    let shapes = send_frame(&ctx, &mut app, Vec::new()).shapes;
    let pos = text_pos(&shapes, "readme.txt");
    click_at(&ctx, &mut app, pos, PointerButton::Primary, Modifiers::NONE);
    click_at(&ctx, &mut app, pos, PointerButton::Primary, Modifiers::NONE);
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid1));

    drive_h.abort();
    drop(app);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dashboard_drawer_toolbar_and_cli_extra_branches() {
    use opendrive_app::design::formatters::ConnectAuthMode;
    use opendrive_app::runtime::cli::list_root_with_base;
    use opendrive_store::{Node, NodeState};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let rt = tokio::runtime::Runtime::new().unwrap();
    // 1. list_root_with_base second-request (list_files) error branch.
    let (bad_list_url, bad_list_h) = rt.block_on(async {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let seq = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h = tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                let n = seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf).await;
                let resp = if n == 0 {
                    let b = r#"{"user":{"displayName":"A","emailAddress":"a@b.c"},"storageQuota":{"usage":"0","limit":"1"}}"#;
                    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len())
                } else {
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".into()
                };
                let _ = s.write_all(resp.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), h)
    });
    assert!(rt
        .block_on(list_root_with_base("t", &bad_list_url))
        .is_err());
    bad_list_h.abort();

    // 2. explorer_open_item local-mount fallback + Dashboard/Drawer/Toolbar UI coverage.
    let drive = seed_drive(
        &[
            (
                "sub1",
                "Sub",
                "application/vnd.google-apps.folder",
                "root",
                &[],
            ),
            ("f1", "readme.txt", "text/plain", "root", b"abcd"),
        ],
        100,
        500,
    );
    let (addr, drive_h) = rt.block_on(FakeDriveServer::start(drive));
    let tmp2 = std::env::temp_dir().join(format!("od-ui2-mnt-{}", uuid::Uuid::new_v4()));
    let (bridge, _rx) = Bridge::new(Settings::default());
    let bridge = bridge.with_mount_manager(Arc::new(
        opendrive_app::runtime::mount_manager::MountManager::new_null()
            .with_mount_root(tmp2.clone()),
    ));
    let aid = add_acct(&bridge, bridge.engine(), "Main", addr, 100, 500);
    let snap = bridge.state_snapshot();
    let local_name = format!("cov-local-{}.txt", uuid::Uuid::new_v4());
    let acct_dir = std::path::PathBuf::from(&snap.mount_root).join(&snap.accounts[0].folder_name);
    let _ = std::fs::create_dir_all(&acct_dir);
    let local_file = acct_dir.join(&local_name);
    std::fs::write(&local_file, b"data").unwrap();
    let root_n = bridge.explorer_root_node(aid).unwrap();
    let local_nid = {
        let st = bridge.engine().store().lock();
        let next = st.next_id().unwrap();
        st.upsert_node(&Node {
            id: next,
            account_id: aid,
            drive_id: "loc-1".into(),
            parent_id: Some(root_n as i64),
            name: local_name.clone(),
            display_name: local_name,
            kind: "file".into(),
            mime: Some("text/plain".into()),
            size: 4,
            md5: None,
            modified_ms: 0,
            head_revision: None,
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Cached,
            pinned: false,
            last_access_ms: 0,
        })
        .unwrap();
        next as u64
    };
    let opened = bridge.explorer_open_item(aid, local_nid, true).unwrap();
    assert_eq!(opened.target, local_file.to_string_lossy());
    let _ = std::fs::remove_dir_all(&tmp2);

    let ep = AuthEndpoints {
        browser_opener: Arc::new(|_| Ok(())),
        folder_opener: Arc::new(|_| Ok(())),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, ep);
    let ctx = egui::Context::default();
    let _ = click_if(&ctx, &mut app, "Read-Write");
    let _ = click_if(&ctx, &mut app, "Alias");
    let _ = click_if(&ctx, &mut app, "Alias");
    let _ = click_if(&ctx, &mut app, "Sync On");
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    let _ = click_if(&ctx, &mut app, "Purge Unpinned Cache");
    let _ = click_if(&ctx, &mut app, "Done");

    app.apply_action(UiAction::ToggleDrawer(Some(true)));
    app.draft.client_id = "cid".into();
    app.draft.client_secret = "csec".into();
    let _ = click_if(&ctx, &mut app, "Show");
    app.apply_action(UiAction::StartBrowserOAuth {
        open_browser: false,
        copy_link: false,
    });
    let _ = wait_until(&ctx, &mut app, |x| {
        x.auth_session
            .as_ref()
            .is_some_and(|s| s.auth_url.is_some())
    });
    let _ = click_if(&ctx, &mut app, "Copy Link");
    let _ = click_if(&ctx, &mut app, "Open Browser");
    let _ = click_if(&ctx, &mut app, "Cancel Sign-In");
    app.apply_action(UiAction::CancelAuth);
    app.draft.auth_mode = ConnectAuthMode::ServiceAccount;
    let _ = click_if(&ctx, &mut app, "Connect Service Account");
    app.drawer_open = false;

    app.apply_action(UiAction::OpenExplorer(aid.to_string()));
    wait_entry(&ctx, &mut app, aid, "Sub");
    let sub_id = eid(&app, aid, "Sub");
    app.apply_action(UiAction::ExplorerNavigateNode(sub_id));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid));
    assert!(click_if(&ctx, &mut app, "My Drive"));
    let _ = wait_until(&ctx, &mut app, |x| !x.explorer.is_busy(aid));
    let _ = click_if(&ctx, &mut app, "Search folder...");
    let _ = send_frame(&ctx, &mut app, vec![egui::Event::Text("read".into())]);
    let _ = click_if(&ctx, &mut app, "Remove");
    app.apply_action(UiAction::OpenExplorer(String::new()));
    let _ = send_frame(&ctx, &mut app, Vec::new());

    drive_h.abort();
}
