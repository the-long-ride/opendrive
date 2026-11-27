//! Full-viewport Google Drive-inspired Cloud Explorer split layout and per-account session state.

mod cache;
pub mod file_ops_modal;
pub mod file_ops_ui;
pub mod items;
mod items_grid;
pub mod jobs;
pub mod list_header;
pub mod selection;
pub mod sidebar;
pub mod toolbar;
mod toolbar_actions;

pub(crate) use self::file_ops_modal::{handle_explorer_file_op_action, render_explorer_modal};
pub use self::file_ops_modal::{ExplorerClipboard, ExplorerModal};
use self::items::render_items_pane;
pub use self::list_header::ExplorerSortColumn;
use self::selection::handle_explorer_shortcuts;
use self::sidebar::render_explorer_sidebar;
use self::toolbar::render_explorer_toolbar;
use crate::design::theme::ThemePalette;
use crate::design::widgets::IconKind;
use crate::desktop::{ControlCenterApp, UiAction};
use crate::runtime::explorer_ops::ExplorerEntry;
use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, Stroke, Vec2};
use opendrive_core::MediaKind;
use opendrive_sync::NodeId;
use std::collections::HashMap;
use uuid::Uuid;

pub(super) fn media_icon_and_color(
    pal: &ThemePalette,
    entry: &ExplorerEntry,
) -> (IconKind, Color32) {
    match entry.media_kind {
        MediaKind::Folder => (IconKind::Folder, pal.accent_blue),
        MediaKind::Image => (IconKind::Image, pal.pop_pink),
        MediaKind::Video => (IconKind::Video, pal.accent_coral),
        MediaKind::Pdf => (IconKind::Pdf, pal.accent_coral),
        MediaKind::Audio => (IconKind::Audio, pal.pop_cyan),
        MediaKind::NativeDoc => (IconKind::DocSheet, pal.ok_fg),
        MediaKind::Archive => (IconKind::Folder, pal.warn_fg),
        MediaKind::CodeText | MediaKind::Other => (IconKind::FileText, pal.text_muted),
    }
}

pub(super) fn item_frame(pal: &ThemePalette, selected: bool, radius: u8, margin: Margin) -> Frame {
    let (fill, stroke) = if selected {
        (pal.accent_blue_tint, Stroke::new(1.5, pal.accent_blue))
    } else {
        (pal.card_alt, Stroke::new(1.5, pal.card_border))
    };
    Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(radius))
        .inner_margin(margin)
}

pub(super) fn paint_entry_icon(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    entry: &ExplorerEntry,
    sz: f32,
) {
    let (icon, col) = media_icon_and_color(pal, entry);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(sz), egui::Sense::hover());
    crate::design::widgets::paint_icon(ui.painter(), rect, icon, col);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerScope {
    #[default]
    MyDrive,
    SharedWithMe,
}

impl ExplorerScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::MyDrive => "My Drive",
            Self::SharedWithMe => "Shared with me",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Debug, Clone, Default)]
pub struct AccountExplorerSession {
    pub scope: ExplorerScope,
    pub breadcrumbs: Vec<(NodeId, String)>,
    pub entries: Vec<ExplorerEntry>,
    pub folder_cache: HashMap<NodeId, Vec<ExplorerEntry>>,
    pub cache_epoch: u64,
    pub selected_id: Option<NodeId>,
    pub anchor_id: Option<NodeId>,
    pub selected_ids: Vec<NodeId>,
    pub search_query: String,
    pub sort_col: ExplorerSortColumn,
    pub sort_desc: bool,
    pub loaded: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ExplorerState {
    pub open: bool,
    pub active_account_id: Option<Uuid>,
    pub view_mode: ExplorerViewMode,
    pub sessions: HashMap<Uuid, AccountExplorerSession>,
    pub busy_accounts: HashMap<Uuid, usize>,
    pub clipboard: Option<ExplorerClipboard>,
    pub modal: Option<ExplorerModal>,
    pub(crate) last_item_click: Option<(Uuid, NodeId, egui::Pos2, f64)>,
}

fn render_bento_pane(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    card_rect: egui::Rect,
    shadow_color: Color32,
    pad: f32,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    let radius = egui::CornerRadius::same(12);
    let p = ui.painter();
    p.rect_filled(
        card_rect.translate(Vec2::new(3.5, 3.5)),
        radius,
        shadow_color,
    );
    p.rect(
        card_rect,
        radius,
        pal.card_bg,
        Stroke::new(1.5, pal.tactile_ink),
        egui::StrokeKind::Inside,
    );
    let inner = card_rect.shrink(pad);
    let builder = egui::UiBuilder::new()
        .max_rect(inner)
        .layout(Layout::top_down(Align::Min));
    let mut child = ui.new_child(builder);
    child.set_clip_rect(inner);
    add_body(&mut child);
}

fn render_main_pane(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    app: &mut ControlCenterApp,
    actions: &mut Vec<UiAction>,
) {
    let active_id = app.explorer.active_account_id;
    let active_acct =
        active_id.and_then(|id| app.state.accounts.iter().find(|a| a.id == id).cloned());
    let is_busy = active_id.is_some_and(|id| app.explorer.is_busy(id));
    let can_write = active_acct.as_ref().is_some_and(|a| !a.read_only);
    let overlay_open = app.settings_open
        || app.guide_open
        || app.drawer_open
        || app.support_open
        || app.available_release.is_some()
        || app.explorer.modal.is_some();
    handle_explorer_shortcuts(ui.ctx(), &app.explorer, can_write, overlay_open, actions);
    let view_mode = app.explorer.view_mode;
    let clipboard = app.explorer.clipboard.clone();
    let mut session = active_id.and_then(|id| app.explorer.sessions.get_mut(&id));
    render_explorer_toolbar(
        ui,
        pal,
        active_acct.as_ref(),
        session.as_deref_mut(),
        clipboard.as_ref(),
        (view_mode, is_busy),
        actions,
    );
    ui.add_space(6.0);
    let div_rect = ui.allocate_space(Vec2::new(ui.available_width(), 1.5)).1;
    ui.painter().rect_filled(div_rect, 0.0, pal.card_border);
    ui.add_space(6.0);
    render_items_pane(
        ui,
        pal,
        active_acct.as_ref(),
        session.as_deref(),
        clipboard.as_ref(),
        (view_mode, active_id, &mut app.explorer.last_item_click),
        actions,
    );
}

pub fn render_explorer_panel(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    app: &mut ControlCenterApp,
    actions: &mut Vec<UiAction>,
) {
    let frame = Frame::new()
        .fill(pal.canvas_bg)
        .inner_margin(Margin::symmetric(18, 16));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        crate::desktop::bento::paint_bento_dots(ui.painter(), ui.max_rect(), pal);
        let avail = ui.available_size();
        if avail.x < 720.0 {
            let slot = ui.painter().add(egui::Shape::Noop);
            let card = Frame::new()
                .fill(pal.card_bg)
                .stroke(Stroke::new(1.5, pal.tactile_ink))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(Margin::same(12));
            let top = card.show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                render_explorer_sidebar(ui, pal, app, true, actions);
            });
            ui.painter().set(
                slot,
                egui::Shape::rect_filled(
                    top.response.rect.translate(Vec2::splat(3.5)),
                    egui::CornerRadius::same(12),
                    pal.pop_pink,
                ),
            );
            ui.add_space(12.0);
            let rem = ui.available_rect_before_wrap();
            let main_rect = egui::Rect::from_min_size(
                rem.min,
                Vec2::new(
                    (rem.width() - 4.0).max(200.0),
                    (rem.height() - 4.0).max(200.0),
                ),
            );
            ui.allocate_rect(main_rect, egui::Sense::hover());
            render_bento_pane(ui, pal, main_rect, pal.pop_cyan, 14.0, |ui| {
                render_main_pane(ui, pal, app, actions)
            });
        } else {
            let g = ui.available_rect_before_wrap();
            let card_h = (g.height() - 4.0).max(200.0);
            let side_w = (g.width() * 0.28).clamp(276.0, 300.0);
            let main_w = (g.width() - side_w - 20.0).max(220.0);
            ui.allocate_rect(g, egui::Sense::hover());
            render_bento_pane(
                ui,
                pal,
                egui::Rect::from_min_size(g.min, Vec2::new(side_w, card_h)),
                pal.pop_pink,
                14.0,
                |ui| render_explorer_sidebar(ui, pal, app, false, actions),
            );
            render_bento_pane(
                ui,
                pal,
                egui::Rect::from_min_size(
                    egui::Pos2::new(g.min.x + side_w + 16.0, g.min.y),
                    Vec2::new(main_w, card_h),
                ),
                pal.pop_cyan,
                14.0,
                |ui| render_main_pane(ui, pal, app, actions),
            );
        }
    });
}
