//! Pure-Rust Control Center application state, actions, custom chrome, Bento Grid, and tray bridge.

mod auth_handlers;
pub mod bento;
pub mod drawer;
pub mod ledger;
pub mod modals;
pub mod panels;
mod release_actions;
pub mod resize;
mod settings_actions;
pub mod titlebar;

use crate::design::formatters::{ConnectAuthMode, NewAccountDraft};
use crate::design::theme::{palette, ThemeMode};
use crate::desktop::modals::guide::render_cloud_guide_modal;
use crate::desktop::modals::release::render_release_modal;
use crate::desktop::modals::settings::render_settings_modal;
use crate::desktop::panels::{
    handle_keyboard_zoom, render_notice_toast, render_workspace_panel, tick_notice_timeout,
};
use crate::desktop::resize::handle_frameless_resize;
use crate::desktop::titlebar::render_custom_titlebar;
use crate::runtime::auth_runner::{AuthEndpoints, AuthSession};
use crate::runtime::bridge::{AppStateResponse, Bridge};
use crate::runtime::bridge_types::{AliasEditDraft, VaultBackupDraft};
use crate::runtime::release_check::{ReleaseCheckHandle, ReleaseInfo};
use crate::shell::tray::{TrayEventQueue, TraySummaryUpdater};
use eframe::egui;
use opendrive_core::DocsMode;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

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
    WindowMinimize,
    WindowToggleMaximize,
    WindowRequestClose,
    WindowShowFromTray,
    WindowForceQuit,
}

pub struct ControlCenterApp {
    pub(crate) bridge: Bridge,
    pub state: AppStateResponse,
    pub theme_mode: ThemeMode,
    pub drawer_open: bool,
    pub settings_open: bool,
    pub guide_open: bool,
    pub tray_active: bool,
    pub hidden_to_tray: bool,
    pub quit_requested: bool,
    pub draft: NewAccountDraft,
    pub alias_edit: Option<AliasEditDraft>,
    pub vault_draft: VaultBackupDraft,
    pub form_error: Option<String>,
    pub notice: Option<UiNotice>,
    pub notice_expires_at: Option<std::time::Instant>,
    pub auth_session: Option<AuthSession>,
    pub reauth_account_id: Option<Uuid>,
    pub(crate) auth_endpoints: AuthEndpoints,
    pub(crate) pending_clipboard: Option<String>,
    pub(crate) repaint_ctx: Option<egui::Context>,
    pub(crate) applied_theme: Option<ThemeMode>,
    tray_events: Option<TrayEventQueue>,
    pub(crate) tray_summary_updater: Option<TraySummaryUpdater>,
    pub(crate) last_tray_summary: String,
    pub(crate) avatar_textures: HashMap<Uuid, egui::TextureHandle>,
    pub(crate) avatar_attempted: HashSet<Uuid>,
    pub available_release: Option<ReleaseInfo>,
    pub(crate) release_check: Option<ReleaseCheckHandle>,
    pub release_checking: bool,
    pub(crate) startup_release_checked: bool,
}

impl ControlCenterApp {
    pub fn new(bridge: Bridge) -> Self {
        Self::with_auth_endpoints(bridge, AuthEndpoints::default())
    }

    pub fn with_auth_endpoints(bridge: Bridge, auth_endpoints: AuthEndpoints) -> Self {
        let state = bridge.state_snapshot();
        if !state.accounts.is_empty() || state.settings.virtual_drive_enabled {
            bridge.spawn_mount_sync(None);
        }
        let (client_id, client_secret) = bridge.saved_oauth_credentials();
        Self {
            bridge,
            state,
            theme_mode: ThemeMode::default(),
            drawer_open: false,
            settings_open: false,
            guide_open: false,
            tray_active: false,
            hidden_to_tray: false,
            quit_requested: false,
            draft: NewAccountDraft {
                client_id,
                client_secret,
                ..Default::default()
            },
            alias_edit: None,
            vault_draft: VaultBackupDraft::default(),
            form_error: None,
            notice: None,
            notice_expires_at: None,
            auth_session: None,
            reauth_account_id: None,
            auth_endpoints,
            pending_clipboard: None,
            repaint_ctx: None,
            applied_theme: None,
            tray_events: None,
            tray_summary_updater: None,
            last_tray_summary: String::new(),
            avatar_textures: HashMap::new(),
            avatar_attempted: HashSet::new(),
            available_release: None,
            release_check: None,
            release_checking: false,
            startup_release_checked: false,
        }
    }

    pub fn with_tray_events(mut self, queue: TrayEventQueue, active: bool) -> Self {
        self.tray_events = Some(queue);
        self.tray_active = active;
        self
    }

    pub fn with_tray_summary_updater(mut self, updater: TraySummaryUpdater) -> Self {
        self.tray_summary_updater = Some(updater);
        self.sync_tray_summary();
        self
    }

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
            win @ (UiAction::WindowMinimize
            | UiAction::WindowToggleMaximize
            | UiAction::WindowRequestClose
            | UiAction::WindowShowFromTray
            | UiAction::WindowForceQuit) => self.handle_window_action(win),
        }
    }

    pub fn render_root(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.repaint_ctx = Some(ctx.clone());
        handle_frameless_resize(&ctx);
        handle_keyboard_zoom(&ctx);
        self.handle_dropped_files(&ctx);
        self.poll_auth();
        self.maybe_check_releases_on_startup();
        self.poll_release_check();
        self.poll_bridge_and_avatars(&ctx);

        let mut actions = Vec::new();
        self.drain_tray_and_close(&ctx, &mut actions);
        self.sync_tray_summary();
        tick_notice_timeout(&mut self.notice, &mut self.notice_expires_at, &ctx);
        let pal = palette(self.theme_mode);
        let top_y = render_custom_titlebar(ui, &pal, self.theme_mode, &self.state, &mut actions);
        render_workspace_panel(ui, &pal, self, &mut actions);
        if self.settings_open {
            render_settings_modal(
                &ctx,
                &pal,
                top_y,
                &self.state,
                &mut self.vault_draft,
                self.release_checking,
                &mut actions,
            );
        }
        if self.guide_open {
            render_cloud_guide_modal(&ctx, &pal, top_y, &mut actions);
        }
        if let Some(release) = self.available_release.as_ref() {
            render_release_modal(&ctx, &pal, top_y, release, &mut actions);
        }
        render_notice_toast(&ctx, &pal, self.notice.as_ref(), &mut actions);
        for action in actions {
            self.apply_action(action);
        }
        if let Some(deadline) = self.notice_expires_at {
            ctx.request_repaint_after(
                deadline.saturating_duration_since(std::time::Instant::now()),
            );
        }
        if self.auth_session.is_some() || self.release_checking {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl eframe::App for ControlCenterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_root(ui);
    }
}
