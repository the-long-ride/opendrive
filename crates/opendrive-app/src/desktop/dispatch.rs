//! Central UiAction dispatcher for ControlCenterApp.

use super::{ControlCenterApp, UiAction};

impl ControlCenterApp {
    pub fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::ToggleDrawer(force) => self.toggle_drawer(force),
            UiAction::ToggleSettings(force) => self.toggle_settings(force),
            UiAction::ToggleGuide(force) => self.toggle_guide(force),
            UiAction::OpenCloudConsole => self.open_cloud_console(),
            UiAction::ToggleTheme => self.toggle_theme(),
            UiAction::SetAuthMode(mode) => self.set_auth_mode(mode),
            UiAction::ToggleSecretVisibility => self.draft.show_secret = !self.draft.show_secret,
            UiAction::StartBrowserOAuth {
                open_browser,
                copy_link,
            } => self.start_browser_oauth_flow(open_browser, copy_link),
            UiAction::CopyAuthLink => self.copy_auth_link(),
            UiAction::OpenAuthBrowser => self.open_auth_browser(),
            UiAction::StartServiceAccount => self.start_service_account_flow(),
            UiAction::ReauthAccount {
                id,
                open_browser,
                copy_link,
            } => self.start_account_reauth(&id, open_browser, copy_link),
            UiAction::OpenReauthCredentials(id) => self.open_reauth_credentials(&id),
            UiAction::CancelAuth => self.cancel_auth(),
            UiAction::AccountAction { action, id } => {
                self.run_account_action(action, id.as_deref())
            }
            UiAction::StartEditAlias(id) => self.start_edit_alias(&id),
            UiAction::SaveAlias => self.save_edit_alias(),
            UiAction::CancelAlias => self.alias_edit = None,
            UiAction::ToggleVaultSecret => {
                self.vault_draft.show_password = !self.vault_draft.show_password
            }
            UiAction::ExportVault => self.export_vault_flow(),
            UiAction::ImportVault => self.import_vault_flow(),
            UiAction::OpenVaultBackup => self.open_vault_backup(),
            UiAction::SetBandwidth(limit) => self.set_bandwidth(limit),
            UiAction::SetCacheMax(max_bytes) => self.set_cache_max(max_bytes),
            UiAction::SetDocsMode(mode) => self.set_docs_mode(mode),
            UiAction::ToggleAutostart => self.toggle_autostart(),
            UiAction::ToggleVirtualDrive => self.toggle_virtual_drive(),
            UiAction::ToggleCheckUpdates => self.toggle_check_updates(),
            UiAction::ToggleExplorerFolderCache => self.toggle_explorer_folder_cache(),
            UiAction::CheckReleasesNow => self.start_release_check(true),
            UiAction::OpenReleasePage(url) => self.open_release_page(&url),
            UiAction::IgnoreReleaseVersion(tag) => self.ignore_release_version(&tag),
            UiAction::DismissReleaseDialog => self.available_release = None,
            UiAction::OpenExternalUrl(url) => self.open_external_url(url),
            UiAction::OpenMountRoot => self.open_mount_root(),
            UiAction::OpenAccountFolder(path) => self.open_account_folder(&path),
            UiAction::PurgeCache => self.purge_cache(),
            UiAction::Refresh => self.refresh(),
            UiAction::DismissNotice => self.dismiss_notice(),
            util @ (UiAction::ToggleTerminal(_)
            | UiAction::SetTerminalTab(_)
            | UiAction::CopyLogs
            | UiAction::ClearLogs
            | UiAction::ToggleSupportModal(_)
            | UiAction::CopySupportPrompt
            | UiAction::SelectUiFont(_)
            | UiAction::ResetUiFont
            | UiAction::ImportUiFont
            | UiAction::BrowseUiFont) => self.handle_utility_action(util),
            exp @ (UiAction::OpenExplorer(_)
            | UiAction::CloseExplorer
            | UiAction::ExplorerSelectAccount(_)
            | UiAction::ExplorerSwitchScope(_)
            | UiAction::ExplorerNavigateNode(_)
            | UiAction::ExplorerNavigateBreadcrumb(_)
            | UiAction::ExplorerRefresh
            | UiAction::ExplorerSelectEntry(_)
            | UiAction::ExplorerToggleSelect(_)
            | UiAction::ExplorerSelectRange { .. }
            | UiAction::ExplorerSelectAll
            | UiAction::ExplorerSetViewMode(_)
            | UiAction::ExplorerToggleSort(_)
            | UiAction::ExplorerSyncDown(_)
            | UiAction::ExplorerSyncUpItems(_)
            | UiAction::ExplorerSyncUpFiles
            | UiAction::ExplorerSyncUpFolder
            | UiAction::ExplorerSyncUpAndClear(_)
            | UiAction::ExplorerOpenItem(_)
            | UiAction::ExplorerCopyStreamUrl(_)
            | UiAction::ExplorerStartRename(_)
            | UiAction::ExplorerSaveRename
            | UiAction::ExplorerRequestDelete(_)
            | UiAction::ExplorerConfirmDelete
            | UiAction::ExplorerDismissModal
            | UiAction::ExplorerSetClipboard { .. }
            | UiAction::ExplorerClearClipboard
            | UiAction::ExplorerPasteClipboard(_)) => self.handle_explorer_action(exp),
            rule @ (UiAction::StartAddOpenWithRule
            | UiAction::StartEditOpenWithRule(_)
            | UiAction::SaveOpenWithRule
            | UiAction::CancelOpenWithRuleEdit
            | UiAction::RemoveOpenWithRule(_)
            | UiAction::BrowseOpenWithRuleApp) => self.handle_open_rule_action(rule),
            win @ (UiAction::WindowMinimize
            | UiAction::WindowToggleMaximize
            | UiAction::WindowRequestClose
            | UiAction::WindowShowFromTray
            | UiAction::WindowForceQuit) => self.handle_window_action(win),
        }
    }
}
