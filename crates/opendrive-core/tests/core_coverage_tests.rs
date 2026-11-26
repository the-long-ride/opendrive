//! Coverage tests for remaining `opendrive-core` controller, manager, and open_rules branches.

use opendrive_core::{
    accounts_path, classify_media_kind, credentials_path, load_accounts, save_accounts,
    settings_path, validate_open_with_rule, Account, AccountManager, AccountStatus, AppCommand,
    AppController, AppEvent, CoreError, MediaKind, OpenWithRule, Settings,
};
use uuid::Uuid;

#[test]
fn controller_and_manager_error_and_noop_branches() {
    let mut ctrl = AppController::new(Settings::default());
    let missing = Uuid::new_v4();
    assert!(matches!(
        ctrl.handle(AppCommand::MoveAccount {
            id: missing,
            delta: 1
        })[0],
        AppEvent::Error(_)
    ));

    ctrl.handle(AppCommand::AddAccount {
        nickname: "Alpha".into(),
    });
    let id = ctrl.accounts.list_accounts()[0].id;
    let ev = ctrl.handle(AppCommand::RenameAccount {
        id,
        nickname: "Beta".into(),
        folder_name: Some(".".into()),
    });
    match &ev[0] {
        AppEvent::AccountUpdated(a) => assert_eq!(a.folder_name, "Alpha"),
        other => panic!("unexpected {other:?}"),
    }

    assert!(matches!(
        ctrl.accounts.move_account(missing, 1),
        Err(CoreError::AccountNotFound(_))
    ));
    assert!(!ctrl.accounts.move_account(id, 0).unwrap());

    let dir = std::env::temp_dir().join(format!("od-core-cov-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    save_accounts(&dir, &[]).unwrap();
    std::fs::write(credentials_path(&dir), "{bad-json").unwrap();
    let mut mgr = AccountManager::new();
    assert!(mgr.load_from(&dir).is_err());
    assert!(load_accounts(&dir).is_ok());
    assert!(accounts_path(&dir).ends_with("accounts.json"));
    assert!(settings_path(&dir).ends_with("settings.json"));

    // Corrupt credentials dir blocks save_to at save_credentials step.
    std::fs::remove_file(credentials_path(&dir)).unwrap();
    std::fs::create_dir_all(credentials_path(&dir)).unwrap();
    assert!(mgr.save_to(&dir).is_err());
    assert!(opendrive_core::load_credentials(&dir).is_err());
    std::fs::remove_file(accounts_path(&dir)).unwrap();
    std::fs::create_dir_all(accounts_path(&dir)).unwrap();
    assert!(load_accounts(&dir).is_err());
    assert!(save_accounts(&dir, &[]).is_err());
    std::fs::create_dir_all(settings_path(&dir)).unwrap();
    assert!(opendrive_core::load_settings(&dir).is_err());
    assert!(opendrive_core::save_settings(&dir, &Settings::default()).is_err());

    let _ = Account {
        id,
        nickname: String::new(),
        email: String::new(),
        credential_set_id: Uuid::nil(),
        folder_name: String::new(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    };
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn open_rules_and_media_kind_edge_branches() {
    assert_eq!(
        classify_media_kind("app.json", Some("application/json")),
        MediaKind::CodeText
    );
    assert_eq!(
        classify_media_kind("notes.txt", Some("text/plain")),
        MediaKind::CodeText
    );
    let existing = vec![OpenWithRule {
        pattern: "   ".into(),
        app_command: "ignored".into(),
    }];
    let valid = validate_open_with_rule("mp4", "vlc", &existing, None).unwrap();
    assert_eq!(valid.pattern, "mp4");
}
