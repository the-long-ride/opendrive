//! Explorer header toolbar: breadcrumbs, search box, List/Grid toggle, refresh, and responsive row layout.

use super::file_ops_modal::ExplorerClipboard;
use super::selection::ToolbarSelection;
use super::toolbar_actions::{
    render_action_controls, render_upload_menu, stable_action_controls_width,
};
use super::{AccountExplorerSession, ExplorerViewMode};
use crate::design::formatters::StatusTone;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    paint_icon, pill_button, pill_button_width, status_pill, ButtonVariant, IconKind,
};
use crate::desktop::UiAction;
use eframe::egui::{
    self, Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Sense, Shape, Stroke,
    StrokeKind, TextEdit, Vec2,
};
use eframe::epaint::RectShape;
use opendrive_core::Account;
use opendrive_sync::NodeId;

fn crumb_label(s: &str, max_c: usize) -> String {
    if s.chars().count() <= max_c {
        s.to_owned()
    } else {
        format!("{}\u{2026}", s.chars().take(max_c).collect::<String>())
    }
}

fn render_breadcrumbs(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    crumbs: &[(NodeId, String)],
    actions: &mut Vec<UiAction>,
) {
    if crumbs.is_empty() {
        let rt = RichText::new("My Drive")
            .size(15.0)
            .strong()
            .color(pal.text_primary);
        ui.label(rt);
        return;
    }
    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
    let (last, max_c) = (crumbs.len() - 1, if crumbs.len() > 1 { 24 } else { 32 });
    for (idx, (_, label)) in crumbs.iter().enumerate() {
        if idx > 0 {
            let (cr, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            paint_icon(ui.painter(), cr, IconKind::ChevronRight, pal.text_muted);
        }
        let v = if idx == last {
            ButtonVariant::TactilePill
        } else {
            ButtonVariant::Default
        };
        if pill_button(ui, pal, None, &crumb_label(label, max_c), v, true, true) {
            actions.push(UiAction::ExplorerNavigateBreadcrumb(idx));
        }
    }
}

fn render_search_box(ui: &mut egui::Ui, pal: &ThemePalette, query: &mut String) -> bool {
    let slot = ui.painter().add(Shape::Noop);
    let (rect, box_resp) = ui.allocate_exact_size(Vec2::new(188.0, 28.0), Sense::click());
    let bld = egui::UiBuilder::new()
        .max_rect(rect.shrink2(Vec2::new(10.0, 4.0)))
        .layout(Layout::left_to_right(Align::Center));
    let mut child = ui.new_child(bld);
    child.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
    child.spacing_mut().interact_size.y = 20.0;
    let (sr, _) = child.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    paint_icon(child.painter(), sr, IconKind::Search, pal.text_muted);
    let ed = TextEdit::singleline(query)
        .frame(Frame::NONE)
        .align(Align2::LEFT_CENTER)
        .hint_text("Search folder...")
        .margin(Margin::symmetric(2, 2));
    let out = child.add_sized([child.available_width().max(40.0), 20.0], ed);
    if box_resp.clicked() {
        out.request_focus();
    }
    let border = if out.has_focus() {
        pal.accent_blue
    } else {
        pal.card_border
    };
    let rs = RectShape::new(
        rect,
        CornerRadius::same(8),
        pal.card_alt,
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    ui.painter().set(slot, Shape::Rect(rs));
    out.changed()
}

#[derive(Clone, Copy)]
enum TopRightCtrl {
    Busy,
    Search,
    List,
    Grid,
    Upload,
    Refresh,
}

fn top_right_controls_width(
    ui: &egui::Ui,
    pal: &ThemePalette,
    has_session: bool,
    is_busy: bool,
) -> f32 {
    use IconKind as I;
    let pw = |ic, lbl: &str, chev| pill_button_width(ui, pal, Some(ic), lbl, true, chev);
    let mut count = 4usize;
    let mut w = pw(I::List, "", false)
        + pw(I::Grid, "", false)
        + pw(I::CloudUpload, "Upload", true)
        + pw(I::Refresh, "", false);
    if has_session {
        w += 188.0;
        count += 1;
    }
    if is_busy {
        w += 28.0;
        count += 1;
    }
    w + (count.saturating_sub(1) as f32 * 8.0)
}

fn render_top_right_controls(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    mut session: Option<&mut AccountExplorerSession>,
    flags: (ExplorerViewMode, bool, bool, bool, bool),
    actions: &mut Vec<UiAction>,
) {
    use ExplorerViewMode as M;
    use IconKind as I;
    use TopRightCtrl as C;
    use UiAction as A;
    let (view_mode, has_acct, up_ok, is_busy, rtl) = flags;
    let def = ButtonVariant::Default;
    let tv = |m: M| ButtonVariant::Toggle {
        selected: view_mode == m,
    };
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
    let order = if rtl {
        [C::Refresh, C::Upload, C::Grid, C::List, C::Search, C::Busy]
    } else {
        [C::Search, C::List, C::Grid, C::Upload, C::Refresh, C::Busy]
    };
    for ctrl in order {
        match ctrl {
            C::Busy if is_busy => status_pill(ui, pal, "Syncing...", StatusTone::Standby),
            C::Search => {
                if let Some(s) = session.as_deref_mut() {
                    if render_search_box(ui, pal, &mut s.search_query) {
                        s.prune_to_visible();
                    }
                }
            }
            C::List if pill_button(ui, pal, Some(I::List), "", tv(M::List), true, true) => {
                actions.push(A::ExplorerSetViewMode(M::List));
            }
            C::Grid if pill_button(ui, pal, Some(I::Grid), "", tv(M::Grid), true, true) => {
                actions.push(A::ExplorerSetViewMode(M::Grid));
            }
            C::Upload => render_upload_menu(ui, pal, up_ok, actions),
            C::Refresh if pill_button(ui, pal, Some(I::Refresh), "", def, true, has_acct) => {
                actions.push(A::ExplorerRefresh);
            }
            _ => {}
        }
    }
}

pub fn render_explorer_toolbar(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    active_acct: Option<&Account>,
    session: Option<&mut AccountExplorerSession>,
    clipboard: Option<&ExplorerClipboard>,
    (view_mode, is_busy): (ExplorerViewMode, bool),
    actions: &mut Vec<UiAction>,
) {
    let has_acct = active_acct.is_some();
    let up_ok = active_acct.is_some_and(|a| !a.read_only);
    let tsel = ToolbarSelection::from_session(session.as_deref());
    let crumbs = session
        .as_deref()
        .map_or_else(Vec::new, |s| s.breadcrumbs.clone());

    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 28.0;
        ui.horizontal_wrapped(|ui| {
            ui.set_min_height(28.0);
            render_breadcrumbs(ui, pal, &crumbs, actions);
        });
        ui.add_space(8.0);

        let search_w = top_right_controls_width(ui, pal, session.is_some(), is_busy);
        let actions_w = stable_action_controls_width(ui, pal, active_acct, &tsel, clipboard);
        if ui.available_width() >= actions_w + search_w + 16.0 {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                render_action_controls(ui, pal, active_acct, &tsel, clipboard, actions);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    render_top_right_controls(
                        ui,
                        pal,
                        session,
                        (view_mode, has_acct, up_ok, is_busy, true),
                        actions,
                    );
                });
            });
        } else {
            let draw_top = |ui: &mut egui::Ui,
                            session: Option<&mut AccountExplorerSession>,
                            actions: &mut Vec<UiAction>| {
                render_top_right_controls(
                    ui,
                    pal,
                    session,
                    (view_mode, has_acct, up_ok, is_busy, false),
                    actions,
                );
            };
            if ui.available_width() >= search_w + 4.0 {
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    draw_top(ui, session, actions);
                });
            } else {
                ui.horizontal_wrapped(|ui| draw_top(ui, session, actions));
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                render_action_controls(ui, pal, active_acct, &tsel, clipboard, actions);
            });
        }
    });
}
