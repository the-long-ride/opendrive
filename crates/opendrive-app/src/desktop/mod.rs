//! Pure-Rust Control Center application state, actions, custom chrome, Bento Grid, Explorer, and tray bridge.

mod actions;
mod auth_handlers;
mod auth_reauth;
pub mod bento;
mod dispatch;
pub mod drawer;
pub mod explorer;
mod explorer_actions;
mod font_actions;
pub mod ledger;
pub mod modals;
pub mod panels;
mod release_actions;
pub mod resize;
mod settings_actions;
pub mod terminal;
pub mod titlebar;

pub use self::actions::{NoticeTone, UiAction, UiNotice};
use self::explorer::jobs::ExplorerJobEvent;
pub use self::explorer::{
    AccountExplorerSession, ExplorerClipboard, ExplorerModal, ExplorerSortColumn, ExplorerState,
    ExplorerViewMode,
};
pub use self::modals::font_ui::FontPickerDraft;
pub use self::modals::guide::GuideUiState;
pub use self::modals::open_rules_ui::OpenRuleDraft;
use self::panels::{
    handle_keyboard_zoom, render_overlays, render_workspace_panel, tick_notice_timeout,
};
use self::resize::handle_frameless_resize;
use self::terminal::render_terminal_panel;
pub use self::terminal::{LogFilterLevel, TerminalTab, TerminalViewState};
use self::titlebar::render_custom_titlebar;
use crate::design::formatters::NewAccountDraft;
use crate::design::theme::{palette, ThemeMode};
use crate::runtime::auth_runner::{AuthEndpoints, AuthSession};
use crate::runtime::bridge::{AppStateResponse, Bridge};
use crate::runtime::bridge_types::{AliasEditDraft, VaultBackupDraft};
use crate::runtime::font_import::FontImportHandle;
use crate::runtime::fonts::DiscoveredFont;
use crate::runtime::release_check::{ReleaseCheckHandle, ReleaseInfo};
use crate::shell::tray::{TrayEventQueue, TraySummaryUpdater};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Duration;
use uuid::Uuid;

pub struct ControlCenterApp {
    pub(crate) bridge: Bridge,
    pub state: AppStateResponse,
    pub theme_mode: ThemeMode,
    pub drawer_open: bool,
    pub settings_open: bool,
    pub guide_open: bool,
    pub terminal_open: bool,
    pub support_open: bool,
    pub guide_state: GuideUiState,
    pub terminal: TerminalViewState,
    pub tray_active: bool,
    pub hidden_to_tray: bool,
    pub quit_requested: bool,
    pub draft: NewAccountDraft,
    pub alias_edit: Option<AliasEditDraft>,
    pub vault_draft: VaultBackupDraft,
    pub open_rule_draft: OpenRuleDraft,
    pub available_fonts: Vec<DiscoveredFont>,
    pub(crate) fonts_loaded: bool,
    pub font_draft: FontPickerDraft,
    pub(crate) font_import: Option<FontImportHandle>,
    pub(crate) applied_font: Option<Option<String>>,
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
    pub explorer: ExplorerState,
    pub(crate) explorer_tx: mpsc::Sender<ExplorerJobEvent>,
    pub(crate) explorer_rx: mpsc::Receiver<ExplorerJobEvent>,
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
        let (explorer_tx, explorer_rx) = mpsc::channel();
        Self {
            bridge,
            state,
            theme_mode: ThemeMode::default(),
            drawer_open: false,
            settings_open: false,
            guide_open: false,
            terminal_open: false,
            support_open: false,
            guide_state: GuideUiState::default(),
            terminal: TerminalViewState::default(),
            tray_active: false,
            hidden_to_tray: false,
            quit_requested: false,
            draft: NewAccountDraft::default(),
            alias_edit: None,
            vault_draft: VaultBackupDraft::default(),
            open_rule_draft: OpenRuleDraft::default(),
            available_fonts: Vec::new(),
            fonts_loaded: false,
            font_draft: FontPickerDraft::default(),
            font_import: None,
            applied_font: None,
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
            explorer: ExplorerState::default(),
            explorer_tx,
            explorer_rx,
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

    pub fn render_root(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.repaint_ctx = Some(ctx.clone());
        handle_frameless_resize(&ctx);
        handle_keyboard_zoom(&ctx);
        self.handle_dropped_files(&ctx);
        self.handle_explorer_drop(&ctx);
        self.poll_auth();
        self.poll_explorer_jobs();
        self.maybe_check_releases_on_startup();
        self.poll_release_check();
        self.poll_font_import();
        self.poll_bridge_and_avatars(&ctx);

        let mut actions = Vec::new();
        self.drain_tray_and_close(&ctx, &mut actions);
        self.sync_tray_summary();
        tick_notice_timeout(&mut self.notice, &mut self.notice_expires_at, &ctx);
        let pal = palette(self.theme_mode);
        let top_y = render_custom_titlebar(
            ui,
            &pal,
            self.theme_mode,
            &self.state,
            self.terminal_open,
            self.support_open,
            &mut actions,
        );
        if self.terminal_open {
            render_terminal_panel(ui, &pal, self, &mut actions);
        } else if self.explorer.open {
            self::explorer::render_explorer_panel(ui, &pal, self, &mut actions);
        } else {
            render_workspace_panel(ui, &pal, self, &mut actions);
        }
        render_overlays(&ctx, &pal, top_y, self, &mut actions);
        for action in actions {
            self.apply_action(action);
        }
        if let Some(deadline) = self.notice_expires_at {
            ctx.request_repaint_after(
                deadline.saturating_duration_since(std::time::Instant::now()),
            );
        }
        if self.auth_session.is_some()
            || self.release_checking
            || self.font_draft.importing
            || !self.explorer.busy_accounts.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.terminal_open {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

impl eframe::App for ControlCenterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_root(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_session();
    }
}
