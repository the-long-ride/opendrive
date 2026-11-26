//! Action enum and status/error notice types for the Control Center desktop UI.

use super::explorer::{ExplorerSortColumn, ExplorerViewMode};
use crate::design::formatters::ConnectAuthMode;
use opendrive_core::DocsMode;
use opendrive_sync::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeTone {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiNotice {
    pub tone: NoticeTone,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    ToggleDrawer(Option<bool>),
    ToggleSettings(Option<bool>),
    ToggleGuide(Option<bool>),
    OpenCloudConsole,
    ToggleTheme,
    SetAuthMode(ConnectAuthMode),
    ToggleSecretVisibility,
    StartBrowserOAuth {
        open_browser: bool,
        copy_link: bool,
    },
    CopyAuthLink,
    OpenAuthBrowser,
    StartServiceAccount,
    ReauthAccount {
        id: String,
        open_browser: bool,
        copy_link: bool,
    },
    CancelAuth,
    AccountAction {
        action: &'static str,
        id: Option<String>,
    },
    StartEditAlias(String),
    SaveAlias,
    CancelAlias,
    ToggleVaultSecret,
    ExportVault,
    ImportVault,
    OpenVaultBackup,
    SetBandwidth(Option<u64>),
    SetCacheMax(u64),
    SetDocsMode(DocsMode),
    ToggleAutostart,
    ToggleVirtualDrive,
    ToggleCheckUpdates,
    ToggleExplorerFolderCache,
    CheckReleasesNow,
    OpenReleasePage(String),
    IgnoreReleaseVersion(String),
    DismissReleaseDialog,
    OpenExternalUrl(&'static str),
    OpenMountRoot,
    OpenAccountFolder(String),
    PurgeCache,
    Refresh,
    DismissNotice,
    OpenExplorer(String),
    CloseExplorer,
    ExplorerSelectAccount(String),
    ExplorerNavigateNode(NodeId),
    ExplorerNavigateBreadcrumb(usize),
    ExplorerRefresh,
    ExplorerSelectEntry(Option<NodeId>),
    ExplorerToggleSelect(NodeId),
    ExplorerSelectRange {
        node_id: NodeId,
        additive: bool,
    },
    ExplorerSelectAll,
    ExplorerSetViewMode(ExplorerViewMode),
    ExplorerToggleSort(ExplorerSortColumn),
    ExplorerSyncDown(Vec<NodeId>),
    ExplorerSyncUpItems(Vec<NodeId>),
    ExplorerSyncUpFiles,
    ExplorerSyncUpFolder,
    ExplorerSyncUpAndClear(Vec<NodeId>),
    ExplorerOpenItem(NodeId),
    ExplorerCopyStreamUrl(NodeId),
    ExplorerStartRename(NodeId),
    ExplorerSaveRename,
    ExplorerRequestDelete(Vec<NodeId>),
    ExplorerConfirmDelete,
    ExplorerDismissModal,
    ExplorerSetClipboard {
        node_ids: Vec<NodeId>,
        cut: bool,
    },
    ExplorerClearClipboard,
    ExplorerPasteClipboard(Option<NodeId>),
    StartAddOpenWithRule,
    StartEditOpenWithRule(usize),
    SaveOpenWithRule,
    CancelOpenWithRuleEdit,
    RemoveOpenWithRule(usize),
    BrowseOpenWithRuleApp,
    WindowMinimize,
    WindowToggleMaximize,
    WindowRequestClose,
    WindowShowFromTray,
    WindowForceQuit,
}

#[cfg(test)]
mod tests {
    use super::UiAction;
    use crate::desktop::explorer::jobs::ExplorerJobEvent;
    use crate::desktop::ControlCenterApp;
    use crate::runtime::{
        auth_runner::AuthEndpoints, bridge::Bridge, explorer_batch::BatchOutcome,
        explorer_ops::ResolvedOpenTarget,
    };
    use opendrive_core::Settings;
    use uuid::Uuid;

    #[test]
    fn poll_jobs_handles_none_folders_and_error_outcomes() {
        let ep =
            AuthEndpoints {
                item_launcher: std::sync::Arc::new(|_, t| {
                    if t == "bad" {
                        Err("e".into())
                    } else {
                        Ok(())
                    }
                }),
                ..Default::default()
            };
        let mut app = ControlCenterApp::with_auth_endpoints(Bridge::new(Settings::default()).0, ep);
        let aid = Uuid::new_v4();
        let evs = [
            ExplorerJobEvent::Listed {
                account_id: aid,
                folder_id: 1,
                entries: vec![],
                epoch: 0,
            },
            ExplorerJobEvent::Mutated {
                account_id: aid,
                folder_id: None,
                message: "m".into(),
            },
            ExplorerJobEvent::BatchFinished {
                account_id: aid,
                folder_id: None,
                outcome: BatchOutcome {
                    succeeded_ids: vec![1],
                    message: "ok".into(),
                    error: None,
                },
                deleted_ids: vec![1],
                moved_ids: vec![2],
            },
            ExplorerJobEvent::BatchFinished {
                account_id: aid,
                folder_id: None,
                outcome: BatchOutcome {
                    succeeded_ids: vec![],
                    message: "0".into(),
                    error: Some("err".into()),
                },
                deleted_ids: vec![1],
                moved_ids: vec![2],
            },
            ExplorerJobEvent::Opened {
                account_id: aid,
                resolved: ResolvedOpenTarget {
                    app_command: None,
                    target: "bad".into(),
                    is_stream_url: false,
                    stream_url: None,
                },
            },
            ExplorerJobEvent::Opened {
                account_id: aid,
                resolved: ResolvedOpenTarget {
                    app_command: None,
                    target: "ok".into(),
                    is_stream_url: true,
                    stream_url: None,
                },
            },
            ExplorerJobEvent::Failed {
                account_id: aid,
                error: "boom".into(),
            },
        ];
        for ev in evs {
            let _ = app.explorer_tx.send(ev);
        }
        app.poll_explorer_jobs();
        assert!(app.notice.is_some());

        app.apply_action(UiAction::StartAddOpenWithRule);
        app.open_rule_draft.pattern = "mp4".into();
        app.open_rule_draft.app_command = "vlc".into();
        app.apply_action(UiAction::SaveOpenWithRule);
        app.apply_action(UiAction::StartAddOpenWithRule);
        app.open_rule_draft.pattern = "mkv".into();
        app.open_rule_draft.app_command = "mpv".into();
        app.apply_action(UiAction::SaveOpenWithRule);
        app.apply_action(UiAction::StartEditOpenWithRule(99));
        app.apply_action(UiAction::StartEditOpenWithRule(1));
        app.apply_action(UiAction::RemoveOpenWithRule(0));
        assert_eq!(app.open_rule_draft.editing_index, Some(0));
        app.apply_action(UiAction::SaveOpenWithRule);
        app.apply_action(UiAction::StartEditOpenWithRule(0));
        app.apply_action(UiAction::RemoveOpenWithRule(0));
        assert!(app.open_rule_draft.editing_index.is_none());
        app.apply_action(UiAction::RemoveOpenWithRule(99));
        app.apply_action(UiAction::StartAddOpenWithRule);
        app.apply_action(UiAction::CancelOpenWithRuleEdit);
        assert!(!app.open_rule_draft.editor_open);
    }
}
