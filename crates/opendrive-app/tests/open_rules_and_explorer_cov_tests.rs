//! Coverage tests for Open With rules modal actions and explorer action edge branches.

use opendrive_app::desktop::{ControlCenterApp, ExplorerSortColumn, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::explorer_ops::ExplorerEntry;
use opendrive_core::{Account, AccountStatus, CredentialSet, MediaKind, Settings};
use opendrive_store::NodeState;
use opendrive_sync::{ExplorerSyncStatus, NodeId};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn open_rules_actions_and_explorer_action_edge_branches_work() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let ep_some = AuthEndpoints {
        app_exe_picker: Arc::new(|| Some(PathBuf::from("okular"))),
        upload_files_picker: Arc::new(Vec::new),
        upload_folder_picker: Arc::new(|| None),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep_some);

    app.apply_action(UiAction::StartAddOpenWithRule);
    app.apply_action(UiAction::SaveOpenWithRule);
    assert!(app.open_rule_draft.error.is_some());
    for (pat, cmd) in [("pdf", "zathura"), ("mp4", "vlc")] {
        app.apply_action(UiAction::StartAddOpenWithRule);
        app.open_rule_draft.pattern = pat.into();
        app.open_rule_draft.app_command = cmd.into();
        app.apply_action(UiAction::SaveOpenWithRule);
    }
    assert_eq!(app.state.settings.open_with_rules.len(), 2);

    for act in [
        UiAction::StartEditOpenWithRule(999),
        UiAction::StartEditOpenWithRule(1),
        UiAction::BrowseOpenWithRuleApp,
    ] {
        app.apply_action(act);
    }
    assert_eq!(app.open_rule_draft.app_command, "okular");
    for act in [
        UiAction::SaveOpenWithRule,
        UiAction::CancelOpenWithRuleEdit,
        UiAction::RemoveOpenWithRule(999),
        UiAction::StartEditOpenWithRule(1),
        UiAction::RemoveOpenWithRule(0),
    ] {
        app.apply_action(act);
    }
    assert_eq!(app.open_rule_draft.editing_index, Some(0));
    app.apply_action(UiAction::RemoveOpenWithRule(0));
    assert!(!app.open_rule_draft.editor_open && app.open_rule_draft.editing_index.is_none());

    let (b_none, _) = Bridge::new(Settings::default());
    let mut app_none = ControlCenterApp::with_auth_endpoints(
        b_none,
        AuthEndpoints {
            app_exe_picker: Arc::new(|| None),
            ..Default::default()
        },
    );
    app_none.apply_action(UiAction::BrowseOpenWithRuleApp);

    app.apply_action(UiAction::OpenExplorer(String::new()));
    assert!(app.explorer.open && app.explorer.active_account_id.is_none());
    app.apply_action(UiAction::OpenExplorer("bad-uuid".into()));
    app.apply_action(UiAction::OpenExplorer(Uuid::new_v4().to_string()));

    let (cid, aid) = (Uuid::new_v4(), Uuid::new_v4());
    {
        let h = bridge.controller_handle();
        let mut g = h.lock().unwrap();
        g.accounts.add_credential(CredentialSet {
            id: cid,
            label: "OAuth".into(),
            client_id: "c1".into(),
        });
        g.accounts.add_account(Account {
            id: aid,
            nickname: "Work".into(),
            email: "w@example.com".into(),
            credential_set_id: cid,
            folder_name: "work".into(),
            read_only: false,
            enabled: true,
            status: AccountStatus::Connected,
            storage_used: 0,
            storage_quota: 0,
            avatar_url: None,
        });
    }
    app.refresh();
    app.apply_action(UiAction::OpenExplorer(String::new()));
    assert_eq!(app.explorer.active_account_id, Some(aid));

    let (fid, file_id): (NodeId, NodeId) = (101, 102);
    let s = app.explorer.sessions.entry(aid).or_default();
    s.loaded = true;
    s.breadcrumbs = vec![(1, "My Drive".into())];
    s.entries = vec![ExplorerEntry {
        id: fid,
        name: "Docs".into(),
        media_kind: MediaKind::Folder,
        is_folder: true,
        is_native_doc: false,
        mime: None,
        size: 0,
        modified_ms: 0,
        state: NodeState::Placeholder,
        sync_status: ExplorerSyncStatus::Cloud,
        pinned: false,
        drive_id: "d-fid".into(),
    }];
    for act in [
        UiAction::ExplorerToggleSort(ExplorerSortColumn::Size),
        UiAction::ExplorerSelectEntry(Some(fid)),
        UiAction::ExplorerToggleSelect(file_id),
        UiAction::ExplorerSelectRange {
            node_id: file_id,
            additive: false,
        },
        UiAction::ExplorerSelectAll,
        UiAction::ExplorerNavigateBreadcrumb(99),
        UiAction::ExplorerNavigateBreadcrumb(0),
        UiAction::ExplorerSyncDown(Vec::new()),
        UiAction::ExplorerSyncUpItems(Vec::new()),
        UiAction::ExplorerSyncUpAndClear(Vec::new()),
        UiAction::ExplorerSyncUpFiles,
        UiAction::ExplorerSyncUpFolder,
        UiAction::ExplorerOpenItem(fid),
        UiAction::ExplorerCopyStreamUrl(file_id),
        UiAction::ToggleExplorerFolderCache,
        UiAction::OpenExplorer(aid.to_string()),
    ] {
        app.apply_action(act);
    }
}
