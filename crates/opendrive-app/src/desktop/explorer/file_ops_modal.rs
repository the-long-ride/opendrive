//! Centered Rename and Delete confirmation modals for Cloud Explorer items.

use super::selection::filter_mutable_ids;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card, pill_button, singleline_input, BentoLayerAccent, ButtonVariant, IconKind,
};
use crate::desktop::{ControlCenterApp, UiAction};
use eframe::egui::{self, Align, Color32, Id, Layout, Pos2, Rect, Sense, Vec2};
use opendrive_sync::NodeId;
use uuid::Uuid;

fn mutable_selection(
    app: &ControlCenterApp,
    nids: &[NodeId],
) -> Option<(
    Uuid,
    Vec<NodeId>,
    crate::runtime::explorer_ops::ExplorerEntry,
)> {
    let aid = app.explorer.active_account_id?;
    let s = app.explorer.sessions.get(&aid)?;
    let ids = filter_mutable_ids(&s.entries, nids);
    let first = s
        .entries
        .iter()
        .find(|e| Some(&e.id) == ids.first())?
        .clone();
    Some((aid, ids, first))
}

pub(crate) fn handle_explorer_file_op_action(app: &mut ControlCenterApp, action: UiAction) {
    match action {
        UiAction::ExplorerStartRename(nid) => {
            if let Some((aid, _, ent)) = mutable_selection(app, &[nid]) {
                app.explorer.modal = Some(ExplorerModal::Rename {
                    account_id: aid,
                    node_id: nid,
                    draft: ent.name,
                    focus_requested: false,
                });
            }
        }
        UiAction::ExplorerSaveRename => {
            if let Some(ExplorerModal::Rename {
                account_id,
                node_id,
                draft,
                ..
            }) = app.explorer.modal.take()
            {
                let name = draft.trim().to_string();
                if name.is_empty() {
                    app.set_err("Name cannot be empty");
                } else {
                    app.spawn_explorer_rename(account_id, node_id, name);
                }
            }
        }
        UiAction::ExplorerRequestDelete(nids) => {
            if let Some((aid, node_ids, first)) = mutable_selection(app, &nids) {
                app.explorer.modal = Some(ExplorerModal::ConfirmDelete {
                    account_id: aid,
                    node_ids,
                    name: first.name,
                    is_folder: first.is_folder,
                });
            }
        }
        UiAction::ExplorerConfirmDelete => {
            if let Some(ExplorerModal::ConfirmDelete {
                account_id,
                node_ids,
                ..
            }) = app.explorer.modal.take()
            {
                if !node_ids.is_empty() {
                    app.spawn_explorer_delete(account_id, node_ids);
                }
            }
        }
        UiAction::ExplorerDismissModal => app.explorer.modal = None,
        UiAction::ExplorerSetClipboard { node_ids, cut } => {
            if let Some((aid, ids, first)) = mutable_selection(app, &node_ids) {
                let lbl = if ids.len() == 1 {
                    format!("'{}'", first.name)
                } else {
                    format!("{} items", ids.len())
                };
                let verb = if cut { "Ready to move" } else { "Copied" };
                app.set_ok(format!("{verb} {lbl} — open destination and click Paste"));
                app.explorer.clipboard = Some(ExplorerClipboard {
                    account_id: aid,
                    node_ids: ids,
                    name: lbl,
                    is_cut: cut,
                });
            }
        }
        UiAction::ExplorerClearClipboard => app.explorer.clipboard = None,
        UiAction::ExplorerPasteClipboard(dest_override) => {
            let (Some((aid, cur_fid)), Some(clip)) = (
                app.explorer.active_account_and_folder(),
                app.explorer.clipboard.clone(),
            ) else {
                return;
            };
            if clip.account_id == aid && !clip.node_ids.is_empty() {
                let dest = dest_override.unwrap_or(cur_fid);
                if clip.is_cut {
                    app.spawn_explorer_move(aid, clip.node_ids, dest);
                } else {
                    app.spawn_explorer_copy(aid, clip.node_ids, dest);
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerClipboard {
    pub account_id: Uuid,
    pub node_ids: Vec<NodeId>,
    pub name: String,
    pub is_cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerModal {
    Rename {
        account_id: Uuid,
        node_id: NodeId,
        draft: String,
        focus_requested: bool,
    },
    ConfirmDelete {
        account_id: Uuid,
        node_ids: Vec<NodeId>,
        name: String,
        is_folder: bool,
    },
}

pub(crate) fn render_explorer_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    modal: &mut ExplorerModal,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::ExplorerDismissModal);
        return;
    }
    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = Rect::from_min_max(Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("explorer_modal_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::ExplorerDismissModal);
            }
        });
    let modal_w = (overlay.width() - 64.0).clamp(300.0, 440.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    egui::Area::new(Id::unique("explorer_modal_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_min_width(modal_w);
            ui.set_max_width(modal_w);
            match modal {
                ExplorerModal::Rename {
                    draft,
                    focus_requested,
                    ..
                } => render_rename_card(ui, pal, draft, focus_requested, actions),
                ExplorerModal::ConfirmDelete {
                    node_ids,
                    name,
                    is_folder,
                    ..
                } => render_delete_card(ui, pal, name, *is_folder, node_ids.len(), actions),
            }
        });
}

fn render_rename_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    draft: &mut String,
    focus_requested: &mut bool,
    actions: &mut Vec<UiAction>,
) {
    bento_card(
        ui,
        pal,
        BentoLayerAccent::Dual,
        "Rename Item",
        Some("Updates the item on Google Drive and any local copy."),
        None,
        |ui| {
            let out = ui.add_sized(
                [ui.available_width(), 34.0],
                singleline_input(draft, "Enter new name..."),
            );
            if !*focus_requested {
                out.request_focus();
                *focus_requested = true;
            }
            if out.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                actions.push(UiAction::ExplorerSaveRename);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let ok = !draft.trim().is_empty();
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Check),
                    "Save",
                    ButtonVariant::Primary,
                    true,
                    ok,
                ) {
                    actions.push(UiAction::ExplorerSaveRename);
                }
                modal_cancel_btn(ui, pal, actions);
            });
        },
    );
}

fn modal_cancel_btn(ui: &mut egui::Ui, pal: &ThemePalette, actions: &mut Vec<UiAction>) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if pill_button(
            ui,
            pal,
            Some(IconKind::Close),
            "Cancel",
            ButtonVariant::Default,
            true,
            true,
        ) {
            actions.push(UiAction::ExplorerDismissModal);
        }
    });
}

fn render_delete_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    name: &str,
    is_folder: bool,
    count: usize,
    actions: &mut Vec<UiAction>,
) {
    let title = if count > 1 {
        "Delete Items"
    } else {
        "Delete Item"
    };
    let msg = if count > 1 {
        format!("Move {count} selected items to Google Drive Trash and delete any local replicas or cached copies?")
    } else {
        let kind = if is_folder { "folder" } else { "file" };
        format!("Move {kind} '{name}' to Google Drive Trash and delete any local replica or cached copy?")
    };
    bento_card(
        ui,
        pal,
        BentoLayerAccent::Dual,
        title,
        Some(&msg),
        None,
        |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Trash),
                    "Delete",
                    ButtonVariant::Danger,
                    true,
                    true,
                ) {
                    actions.push(UiAction::ExplorerConfirmDelete);
                }
                modal_cancel_btn(ui, pal, actions);
            });
        },
    );
}
