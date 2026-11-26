//! Explorer items pane: Google Drive-inspired List & Grid views, vector media icons, and context menus.

use super::file_ops_modal::ExplorerClipboard;
use super::file_ops_ui::{paste_label_for, render_entry_context_menu, render_sync_state_pill};
use super::items_grid::{consume_empty_area_click, render_empty_items_state};
use super::list_header::{
    paint_theme_checkbox, render_list_header, row_checkbox_rect, CheckboxState, ListCols,
};
use super::selection::SelectionRef;
use super::{media_icon_and_color, AccountExplorerSession, ExplorerViewMode};
use crate::design::formatters::{format_bytes, format_modified_date};
use crate::design::theme::ThemePalette;
use crate::design::widgets::paint_icon;
use crate::desktop::UiAction;
use crate::runtime::explorer_ops::ExplorerEntry;
use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, Id, Layout, Pos2, Rect, RichText, ScrollArea, Sense,
    Shape, Stroke, StrokeKind, Vec2,
};
use eframe::epaint::RectShape;
use opendrive_core::Account;
use opendrive_sync::NodeId;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) struct ItemInteractCfg {
    pub can_write: bool,
    pub paste_label: Option<&'static str>,
    pub in_checkbox: bool,
}

pub(super) fn item_size_label(entry: &ExplorerEntry) -> String {
    if entry.is_folder {
        "\u{2014}".into()
    } else {
        format_bytes(entry.size)
    }
}

fn cell_ui(
    ui: &mut egui::Ui,
    x: (f32, f32),
    row: Rect,
    right_align: bool,
    f: impl FnOnce(&mut egui::Ui),
) {
    let rect = Rect::from_min_max(Pos2::new(x.0, row.top()), Pos2::new(x.1, row.bottom()));
    let clip = ui.clip_rect().intersect(rect);
    if !clip.is_positive() {
        return;
    }
    let layout = if right_align {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    child.style_mut().interaction.selectable_labels = false;
    child.spacing_mut().interact_size.y = 24.0;
    child.set_clip_rect(clip);
    f(&mut child);
}

pub(super) fn item_style(
    pal: &ThemePalette,
    selected: bool,
    hovered: bool,
) -> (egui::Color32, Stroke) {
    if selected {
        (pal.accent_blue_tint, Stroke::new(1.5, pal.accent_blue))
    } else if hovered {
        (pal.pill_neutral, Stroke::new(1.5, pal.pop_cyan))
    } else {
        (pal.card_alt, Stroke::new(1.5, pal.card_border))
    }
}

pub(super) fn activate_entry_action(entry: &ExplorerEntry) -> UiAction {
    if entry.is_folder {
        UiAction::ExplorerNavigateNode(entry.id)
    } else {
        UiAction::ExplorerOpenItem(entry.id)
    }
}

pub(super) fn attach_item_interactions(
    pal: &ThemePalette,
    resp: egui::Response,
    cfg: ItemInteractCfg,
    entry: &ExplorerEntry,
    sel: SelectionRef<'_>,
    actions: &mut Vec<UiAction>,
) {
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let mods = resp.ctx.input(|i| i.modifiers);
    if resp.double_clicked() && !cfg.in_checkbox {
        actions.push(UiAction::ExplorerSelectEntry(Some(entry.id)));
        actions.push(activate_entry_action(entry));
    } else if resp.clicked() {
        if mods.shift {
            actions.push(UiAction::ExplorerSelectRange {
                node_id: entry.id,
                additive: mods.command || mods.ctrl || cfg.in_checkbox,
            });
        } else if mods.command || mods.ctrl || cfg.in_checkbox {
            actions.push(UiAction::ExplorerToggleSelect(entry.id));
        } else {
            actions.push(UiAction::ExplorerSelectEntry(Some(entry.id)));
        }
    }
    if resp.secondary_clicked() && !sel.is_selected(entry.id) {
        actions.push(UiAction::ExplorerSelectEntry(Some(entry.id)));
    }
    resp.context_menu(|ui| {
        render_entry_context_menu(ui, pal, entry, sel, cfg.can_write, cfg.paste_label, actions);
    });
}

fn render_list_view(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    entries: &[&ExplorerEntry],
    sel: SelectionRef<'_>,
    can_write: bool,
    paste_label: Option<&'static str>,
    actions: &mut Vec<UiAction>,
) {
    ui.style_mut().interaction.selectable_labels = false;
    let row_w = (ui.available_width() - 10.0).max(200.0);
    render_list_header(
        ui,
        pal,
        row_w,
        entries.len(),
        sel.selected_ids.len(),
        (sel.sort_col, sel.sort_desc),
        actions,
    );
    ui.add_space(4.0);
    ScrollArea::vertical()
        .id_salt(Id::unique("explorer_list_scroll"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.spacing_mut().item_spacing.y = 4.0;
            for entry in entries {
                let (r, _) = ui.allocate_exact_size(Vec2::new(row_w, 36.0), Sense::hover());
                let bg_slot = ui.painter().add(Shape::Noop);
                let cols = ListCols::compute(r.left(), row_w);
                let cb_rect = row_checkbox_rect(&cols, r.center().y);
                let ptr_pos = ui
                    .ctx()
                    .input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
                let in_checkbox = ptr_pos.is_some_and(|p| cb_rect.expand(5.0).contains(p));
                let visible = ui.clip_rect().intersects(r);
                if visible {
                    cell_ui(ui, (cols.name.0 + 26.0, cols.name.1), r, false, |ui| {
                        let txt = RichText::new(&entry.name)
                            .size(13.5)
                            .strong()
                            .color(pal.text_primary);
                        ui.add(egui::Label::new(txt).truncate().selectable(false));
                    });
                    if let Some(m) = cols.modified {
                        cell_ui(ui, m, r, false, |ui| {
                            let dt = RichText::new(format_modified_date(entry.modified_ms))
                                .size(12.0)
                                .monospace()
                                .color(pal.text_muted);
                            ui.add(egui::Label::new(dt).selectable(false));
                        });
                    }
                    cell_ui(ui, cols.size, r, true, |ui| {
                        let sz = RichText::new(item_size_label(entry))
                            .size(12.0)
                            .monospace()
                            .color(pal.text_muted);
                        ui.add(egui::Label::new(sz).selectable(false));
                    });
                }
                let resp = ui.interact(
                    r,
                    Id::unique(("explorer_list_row", entry.id)),
                    Sense::click(),
                );
                if visible {
                    let selected = sel.is_selected(entry.id);
                    let (fill, stroke) = item_style(pal, selected, resp.hovered());
                    let rs =
                        RectShape::new(r, CornerRadius::same(10), fill, stroke, StrokeKind::Inside);
                    ui.painter().set(bg_slot, Shape::Rect(rs));
                    let cb_st = if selected {
                        CheckboxState::Checked
                    } else {
                        CheckboxState::Unchecked
                    };
                    paint_theme_checkbox(
                        ui.painter(),
                        cb_rect,
                        cb_st,
                        pal,
                        resp.hovered() && in_checkbox,
                    );
                    let (icon, col) = media_icon_and_color(pal, entry);
                    let ir = Rect::from_min_size(
                        Pos2::new(cols.name.0, r.center().y - 9.0),
                        Vec2::splat(18.0),
                    );
                    paint_icon(ui.painter(), ir, icon, col);
                    cell_ui(ui, cols.state, r, false, |ui| {
                        render_sync_state_pill(ui, pal, entry)
                    });
                }
                let cfg = ItemInteractCfg {
                    can_write,
                    paste_label,
                    in_checkbox,
                };
                attach_item_interactions(pal, resp, cfg, entry, sel, actions);
            }
            consume_empty_area_click(ui, pal, !sel.selected_ids.is_empty(), paste_label, actions);
        });
}

pub type LastItemClick = Option<(Uuid, NodeId, Pos2, f64)>;

pub fn render_items_pane(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    active_acct: Option<&Account>,
    session: Option<&AccountExplorerSession>,
    clipboard: Option<&ExplorerClipboard>,
    (view_mode, active_id, last_click): (ExplorerViewMode, Option<Uuid>, &mut LastItemClick),
    actions: &mut Vec<UiAction>,
) {
    if active_acct.is_none() {
        ui.label(
            RichText::new("Select or connect an account to explore files.").color(pal.text_muted),
        );
        return;
    }
    let Some(s) = session else {
        ui.label(RichText::new("Loading Drive folder...").color(pal.text_muted));
        return;
    };
    let filtered = super::selection::visible_sorted_entries(s);
    let sel_ids = s.effective_selected_ids();
    let sel = SelectionRef {
        entries: s.entries.as_slice(),
        selected_ids: sel_ids.as_slice(),
        sort_col: s.sort_col,
        sort_desc: s.sort_desc,
    };
    let can_write = active_acct.is_some_and(|a| !a.read_only);
    let paste_label = paste_label_for(clipboard, active_acct.map(|a| a.id), can_write);
    if filtered.is_empty() {
        render_empty_items_state(
            ui,
            pal,
            s.entries.is_empty(),
            !sel_ids.is_empty(),
            paste_label,
            actions,
        );
        return;
    }
    let action_start = actions.len();
    match view_mode {
        ExplorerViewMode::List => {
            render_list_view(ui, pal, &filtered, sel, can_write, paste_label, actions)
        }
        ExplorerViewMode::Grid => super::items_grid::render_grid_view(
            ui,
            pal,
            &filtered,
            sel,
            can_write,
            paste_label,
            actions,
        ),
    }
    super::items_grid::finalize_item_clicks(
        ui.ctx(),
        active_id,
        &filtered,
        action_start,
        last_click,
        actions,
    );
}
