//! Integration tests for `opendrive-core` domain model, IO, and controller.

use opendrive_core::{
    accounts_path, credentials_path, default_account_alias, load_accounts, load_credentials,
    load_settings, sanitize_folder_segment, save_accounts, save_credentials, save_settings,
    settings_path, Account, AccountManager, AccountStatus, AppCommand, AppController, AppEvent,
    CoreError, CredentialSet, DocsMode, Settings,
};
use uuid::Uuid;

#[test]
fn account_manager_and_io_roundtrip() {
    let dir = std::env::temp_dir().join(format!("opendrive-core-test-{}", Uuid::new_v4()));
    assert!(load_accounts(&dir).unwrap().is_empty());
    assert!(load_credentials(&dir).unwrap().is_empty());
    assert_eq!(load_settings(&dir).unwrap(), Settings::default());

    let cred_id = Uuid::new_v4();
    let acct_id = Uuid::new_v4();
    let mut mgr = AccountManager::new();
    mgr.add_credential(CredentialSet {
        id: cred_id,
        label: "Personal".into(),
        client_id: "cid-1".into(),
    });
    assert_eq!(mgr.list_credentials().len(), 1);
    assert_eq!(mgr.get_credential(cred_id).unwrap().label, "Personal");
    assert!(matches!(
        mgr.get_credential(Uuid::new_v4()),
        Err(CoreError::CredentialNotFound(_))
    ));

    mgr.add_account(Account {
        id: acct_id,
        nickname: "Work".into(),
        email: "w@example.com".into(),
        credential_set_id: cred_id,
        folder_name: "WorkDrive".into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 1024,
        storage_quota: 4096,
        avatar_url: Some("https://example.com/a.png".into()),
    });
    // Legacy accounts.json without avatar_url / storage fields deserializes cleanly.
    let legacy_json = r#"[{"id":"11111111-1111-1111-1111-111111111111","nickname":"Old","email":"o@x.com","credential_set_id":"00000000-0000-0000-0000-000000000000","folder_name":"Old","read_only":false,"enabled":true,"status":"Connected"}]"#;
    let parsed_legacy: Vec<Account> = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(parsed_legacy[0].avatar_url, None);
    mgr.save_to(&dir).unwrap();
    assert!(accounts_path(&dir).exists());
    assert!(credentials_path(&dir).exists());

    let mut loaded = AccountManager::new();
    loaded.load_from(&dir).unwrap();
    assert_eq!(loaded.list_accounts(), mgr.list_accounts());
    assert_eq!(loaded.list_credentials(), mgr.list_credentials());
    assert_eq!(loaded.get_account(acct_id).unwrap().nickname, "Work");

    loaded.remove_account(acct_id).unwrap();
    assert!(matches!(
        loaded.remove_account(acct_id),
        Err(CoreError::AccountNotFound(_))
    ));

    let settings = Settings {
        root_path: "/mnt/od".into(),
        cache_dir: "/tmp/cache".into(),
        cache_max_size: 4096,
        bandwidth_limit: Some(1024),
        autostart: true,
        check_updates: true,
        docs_mode: DocsMode::ExportOffice,
        virtual_drive_enabled: true,
        virtual_drive_letter: "Z".into(),
        ignored_release_version: Some("v0.2.0".into()),
        open_with_rules: Vec::new(),
        explorer_folder_cache: true,
        ui_font_family: Some("Inter".into()),
    };
    save_settings(&dir, &settings).unwrap();
    assert!(settings_path(&dir).exists());
    assert_eq!(load_settings(&dir).unwrap(), settings);

    save_accounts(&dir, &[]).unwrap();
    save_credentials(&dir, &[]).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn io_errors_on_corrupt_json_and_file_as_dir() {
    let base = std::env::temp_dir().join(format!("opendrive-core-err-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();

    std::fs::write(accounts_path(&base), "{bad").unwrap();
    std::fs::write(credentials_path(&base), "{bad").unwrap();
    std::fs::write(settings_path(&base), "{bad").unwrap();
    assert!(matches!(load_accounts(&base), Err(CoreError::Other(_))));
    assert!(matches!(load_credentials(&base), Err(CoreError::Other(_))));
    assert!(matches!(load_settings(&base), Err(CoreError::Other(_))));

    let file_not_dir = base.join("blocker");
    std::fs::write(&file_not_dir, "x").unwrap();
    assert!(matches!(
        save_accounts(&file_not_dir, &[]),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        save_credentials(&file_not_dir, &[]),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        save_settings(&file_not_dir, &Settings::default()),
        Err(CoreError::Other(_))
    ));
    let mut mgr = AccountManager::new();
    assert!(matches!(mgr.load_from(&base), Err(CoreError::Other(_))));
    assert!(matches!(
        mgr.save_to(&file_not_dir),
        Err(CoreError::Other(_))
    ));

    let dir_entries = base.join("dir_entries");
    std::fs::create_dir_all(accounts_path(&dir_entries)).unwrap();
    std::fs::create_dir_all(credentials_path(&dir_entries)).unwrap();
    std::fs::create_dir_all(settings_path(&dir_entries)).unwrap();
    assert!(matches!(
        load_accounts(&dir_entries),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        load_credentials(&dir_entries),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        load_settings(&dir_entries),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        save_accounts(&dir_entries, &[]),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        save_credentials(&dir_entries, &[]),
        Err(CoreError::Other(_))
    ));
    assert!(matches!(
        save_settings(&dir_entries, &Settings::default()),
        Err(CoreError::Other(_))
    ));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn app_controller_handles_all_commands() {
    let mut ctrl = AppController::new(Settings::default());
    let ev = ctrl.handle(AppCommand::AddAccount {
        nickname: "Alpha".into(),
    });
    let id = match &ev[0] {
        AppEvent::AccountUpdated(a) => {
            assert_eq!(a.status, AccountStatus::NeedsReauth);
            a.id
        }
        other => panic!("unexpected event: {other:?}"),
    };

    let ev = ctrl.handle(AppCommand::SetReadOnly {
        id,
        read_only: true,
    });
    assert!(matches!(&ev[0], AppEvent::AccountUpdated(a) if a.read_only));

    let ev = ctrl.handle(AppCommand::PauseAccount { id });
    assert!(matches!(
        &ev[0],
        AppEvent::AccountUpdated(a) if a.status == AccountStatus::Paused && !a.enabled
    ));

    let ev = ctrl.handle(AppCommand::ResumeAccount { id });
    assert!(matches!(
        &ev[0],
        AppEvent::AccountUpdated(a) if a.status == AccountStatus::Connected && a.enabled
    ));

    let _ = ctrl.handle(AppCommand::AddAccount {
        nickname: "Beta".into(),
    });
    assert_eq!(ctrl.handle(AppCommand::PauseAll).len(), 2);
    assert_eq!(ctrl.handle(AppCommand::ResumeAll).len(), 2);

    let new_settings = Settings {
        bandwidth_limit: Some(5_000_000),
        docs_mode: DocsMode::ExportOffice,
        ..Settings::default()
    };
    assert!(matches!(
        ctrl.handle(AppCommand::UpdateSettings(new_settings.clone()))
            .as_slice(),
        [AppEvent::Ready]
    ));
    assert_eq!(ctrl.settings, new_settings);

    assert!(matches!(
        ctrl.handle(AppCommand::OpenFolder).as_slice(),
        [AppEvent::Ready]
    ));
    assert!(matches!(
        ctrl.handle(AppCommand::Quit).as_slice(),
        [AppEvent::Ready]
    ));
    assert!(matches!(
        ctrl.handle(AppCommand::RemoveAccount { id }).as_slice(),
        [AppEvent::Ready]
    ));
    assert!(matches!(
        ctrl.handle(AppCommand::PauseAccount { id }).as_slice(),
        [AppEvent::Error(_)]
    ));
    assert!(matches!(
        ctrl.handle(AppCommand::RemoveAccount { id }).as_slice(),
        [AppEvent::Error(_)]
    ));
}

#[test]
fn settings_backwards_compat_defaults_virtual_drive_fields() {
    let dir = std::env::temp_dir().join(format!("opendrive-core-compat-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let legacy_json = r#"{
        "root_path": "/mnt/legacy",
        "cache_dir": "/tmp/legacy-cache",
        "cache_max_size": 2048,
        "bandwidth_limit": null,
        "autostart": false,
        "check_updates": true,
        "docs_mode": "Link"
    }"#;
    std::fs::write(settings_path(&dir), legacy_json).unwrap();
    let loaded = load_settings(&dir).unwrap();
    assert_eq!(loaded.root_path, "/mnt/legacy");
    assert!(!loaded.virtual_drive_enabled);
    assert_eq!(loaded.virtual_drive_letter, "O");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn alias_defaults_and_rename_account_command_work() {
    assert_eq!(
        default_account_alias("", "alice.smith@gmail.com"),
        "alice.smith"
    );
    assert_eq!(default_account_alias("  Work  ", "alice@gmail.com"), "Work");
    assert_eq!(default_account_alias("", ""), "Drive");
    assert_eq!(
        sanitize_folder_segment("../bad:name/"),
        Some("_bad_name_".into())
    );
    assert_eq!(sanitize_folder_segment(".."), None);

    let mut ctrl = AppController::new(Settings::default());
    let acct_id = Uuid::new_v4();
    ctrl.accounts.add_account(Account {
        id: acct_id,
        nickname: "OldAlias".into(),
        email: "marshall@example.com".into(),
        credential_set_id: Uuid::nil(),
        folder_name: "Marshall".into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    });

    // Blank alias falls back to local-part before @; None folder keeps Marshall.
    ctrl.handle(AppCommand::RenameAccount {
        id: acct_id,
        nickname: "   ".into(),
        folder_name: None,
    });
    let updated = ctrl.accounts.get_account(acct_id).unwrap();
    assert_eq!(updated.nickname, "marshall");
    assert_eq!(updated.folder_name, "Marshall");

    // Explicit alias + folder rename.
    ctrl.handle(AppCommand::RenameAccount {
        id: acct_id,
        nickname: "Team Drive".into(),
        folder_name: Some("TeamDrive/Ops".into()),
    });
    let updated = ctrl.accounts.get_account(acct_id).unwrap();
    assert_eq!(updated.nickname, "Team Drive");
    assert_eq!(updated.folder_name, "TeamDrive_Ops");
}
