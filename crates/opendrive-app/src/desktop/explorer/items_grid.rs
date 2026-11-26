//! Explorer items grid view cards and interactions.

use super::file_ops_ui::render_sync_state_pill;
use super::items::{
    activate_entry_action, attach_item_interactions, item_size_label, item_style, ItemInteractCfg,
    LastItemClick,
};
use super::paint_entry_icon;
use super::selection::SelectionRef;
use crate::design::formatters::format_modified_date;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{menu_pill_button, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::explorer_ops::ExplorerEntry;
use eframe::egui::{
    self, Align, CornerRadius, Id, Layout, Pos2, Rect, RichText, ScrollArea, Sense, Shape,
    StrokeKind, Vec2,
};
use eframe::epaint::RectShape;
use uuid::Uuid;

pub(super) fn finalize_item_clicks(
    ctx: &egui::Context,
    acct_id: Option<Uuid>,
    entries: &[&ExplorerEntry],
    action_start: usize,
    last_click: &mut LastItemClick,
    actions: &mut Vec<UiAction>,
) {
    let Some(aid) = acct_id else { return };
    let (now, released_pos, mods) = ctx.input(|i| {
        let rel = i.events.iter().rev().find_map(|ev| match ev {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } => Some(*pos),
            _ => None,
        });
        (i.time, rel, i.modifiers)
    });
    let opened = actions[action_start..].iter().any(|a| {
        matches!(
            a,
            UiAction::ExplorerNavigateNode(_) | UiAction::ExplorerOpenItem(_)
        )
    });
    if opened
        || actions[action_start..].iter().any(|a| {
            matches!(
                a,
                UiAction::ExplorerToggleSelect(_) | UiAction::ExplorerSelectRange { .. }
            )
        })
    {
        *last_click = None;
        return;
    }
    let new_sel = actions[action_start..].iter().find_map(|a| match a {
        UiAction::ExplorerSelectEntry(Some(id)) => Some(*id),
        _ => None,
    });
    if mods.any() {
        if new_sel.is_some() {
            *last_click = None;
        }
        return;
    }
    if let (Some((prev_aid, prev_nid, prev_pos, prev_t)), Some(pos)) = (*last_click, released_pos) {
        let same_spot = prev_aid == aid
            && now >= prev_t
            && (now - prev_t) <= 0.35
            && (pos - prev_pos).length() <= 6.0;
        if same_spot && new_sel.is_none_or(|id| id == prev_nid) {
            if let Some(entry) = entries.iter().find(|e| e.id == prev_nid) {
                actions.truncate(action_start);
                actions.push(UiAction::ExplorerSelectEntry(Some(prev_nid)));
                actions.push(activate_entry_action(entry));
                *last_click = None;
                return;
            }
        }
    }
    if let (Some(nid), Some(pos)) = (new_sel, released_pos) {
        *last_click = Some((aid, nid, pos, now));
    }
}

pub(super) fn render_grid_view(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    entries: &[&ExplorerEntry],
    sel: SelectionRef<'_>,
    can_write: bool,
    paste_label: Option<&'static str>,
    actions: &mut Vec<UiAction>,
) {
    ui.style_mut().interaction.selectable_labels = false;
    let cols = ((ui.available_width() + 10.0) / 196.0).floor().max(1.0) as usize;
    ScrollArea::vertical()
        .id_salt(Id::unique("explorer_grid_scroll"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            for row in entries.chunks(cols) {
                ui.columns(cols, |col_uis| {
                    for (idx, entry) in row.iter().enumerate() {
                        let cui = &mut col_uis[idx];
                        cui.style_mut().interaction.selectable_labels = false;
                        let card_w = cui.available_width().max(120.0);
                        let (card_r, _) =
                            cui.allocate_exact_size(Vec2::new(card_w, 106.0), Sense::hover());
                        let bg_slot = cui.painter().add(Shape::Noop);
                        let visible = cui.clip_rect().intersects(card_r);
                        let inner = card_r.shrink2(Vec2::new(12.0, 10.0));
                        let clip = cui.clip_rect().intersect(inner);
                        let badge_rect = Rect::from_min_size(
                            Pos2::new(inner.right() - 28.0, inner.top()),
                            Vec2::new(28.0, 24.0),
                        );
                        if visible && clip.is_positive() {
                            let bld = egui::UiBuilder::new()
                                .max_rect(inner)
                                .layout(Layout::top_down(Align::Min));
                            let mut child = cui.new_child(bld);
                            child.style_mut().interaction.selectable_labels = false;
                            child.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                            child.set_clip_rect(clip);
                            child.horizontal(|ui| {
                                ui.set_min_height(24.0);
                                paint_entry_icon(ui, pal, entry, 24.0);
                            });
                            child.add_space(2.0);
                            let name_t = RichText::new(&entry.name)
                                .size(13.0)
                                .strong()
                                .color(pal.text_primary);
                            child.add(egui::Label::new(name_t).truncate().selectable(false));
                            child.add_space(2.0);
                            let mod_t = RichText::new(format_modified_date(entry.modified_ms))
                                .size(10.5)
                                .monospace()
                                .color(pal.text_muted);
                            let size_t = RichText::new(item_size_label(entry))
                                .size(10.5)
                                .monospace()
                                .color(pal.text_muted);
                            if card_w >= 190.0 {
                                child.horizontal(|ui| {
                                    ui.add(egui::Label::new(mod_t).selectable(false));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add(egui::Label::new(size_t).selectable(false));
                                    });
                                });
                            } else {
                                child.add(egui::Label::new(mod_t).selectable(false));
                                child.add(egui::Label::new(size_t).selectable(false));
                            }
                        }
                        let resp = cui.interact(
                            card_r,
                            Id::unique(("explorer_grid_card", entry.id)),
                            Sense::click(),
                        );
                        if visible {
                            let (fill, stroke) =
                                item_style(pal, sel.is_selected(entry.id), resp.hovered());
                            let rs = RectShape::new(
                                card_r,
                                CornerRadius::same(10),
                                fill,
                                stroke,
                                StrokeKind::Inside,
                            );
                            cui.painter().set(bg_slot, Shape::Rect(rs));
                            if clip.is_positive() {
                                let bld = egui::UiBuilder::new()
                                    .max_rect(badge_rect)
                                    .layout(Layout::right_to_left(Align::Center));
                                let mut bui = cui.new_child(bld);
                                bui.set_clip_rect(clip);
                                render_sync_state_pill(&mut bui, pal, entry);
                            }
                        }
                        let cfg = ItemInteractCfg {
                            can_write,
                            paste_label,
                            in_checkbox: false,
                        };
                        attach_item_interactions(pal, resp, cfg, entry, sel, actions);
                    }
                    for cui in &mut col_uis[row.len()..] {
                        let gap_r = cui
                            .allocate_exact_size(
                                Vec2::new(cui.available_width().max(1.0), 106.0),
                                Sense::click(),
                            )
                            .1;
                        attach_empty_actions(
                            gap_r,
                            pal,
                            !sel.selected_ids.is_empty(),
                            paste_label,
                            actions,
                        );
                    }
                });
                ui.add_space(10.0);
            }
            consume_empty_area_click(ui, pal, !sel.selected_ids.is_empty(), paste_label, actions);
        });
}

pub(super) fn render_empty_items_state(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    entries_empty: bool,
    has_selection: bool,
    paste_label: Option<&'static str>,
    actions: &mut Vec<UiAction>,
) {
    let msg = if entries_empty {
        "This folder is empty. Drop files here or click Upload."
    } else {
        "No items match your search filter."
    };
    let empty_r = ui
        .allocate_exact_size(ui.available_size().max(Vec2::splat(40.0)), Sense::click())
        .1;
    ui.painter().text(
        empty_r.rect.center(),
        egui::Align2::CENTER_CENTER,
        msg,
        egui::FontId::proportional(13.5),
        pal.text_muted,
    );
    attach_empty_actions(empty_r, pal, has_selection, paste_label, actions);
}

pub(super) fn consume_empty_area_click(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    has_selection: bool,
    paste_label: Option<&'static str>,
    actions: &mut Vec<UiAction>,
) {
    let rem_h = (ui.clip_rect().bottom() - ui.cursor().top() - 2.0).max(0.0);
    if rem_h <= 4.0 {
        return;
    }
    let resp = ui
        .allocate_exact_size(
            Vec2::new(ui.available_width().max(1.0), rem_h),
            Sense::click(),
        )
        .1;
    attach_empty_actions(resp, pal, has_selection, paste_label, actions);
}

fn attach_empty_actions(
    resp: egui::Response,
    pal: &ThemePalette,
    has_selection: bool,
    paste_label: Option<&'static str>,
    actions: &mut Vec<UiAction>,
) {
    if resp.clicked() && has_selection {
        actions.push(UiAction::ExplorerSelectEntry(None));
    }
    if let Some(pl) = paste_label {
        resp.context_menu(|ui| {
            let menu_w = 188.0;
            ui.set_min_width(menu_w);
            ui.set_max_width(menu_w);
            if menu_pill_button(
                ui,
                pal,
                Some(IconKind::Check),
                pl,
                ButtonVariant::TactilePill,
                menu_w,
                true,
            ) {
                actions.push(UiAction::ExplorerPasteClipboard(None));
                ui.close();
            }
        });
    }
}
