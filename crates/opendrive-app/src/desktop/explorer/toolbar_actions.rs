//! Compact Cloud Explorer action controls: selection chip, Upload/Sync menus, file ops, and clipboard.

use super::file_ops_modal::ExplorerClipboard;
use super::file_ops_ui::{
    mutable_file_op_items, paste_label_for, render_popup_menu_items, PopupMenuItem,
};
use super::items::activate_entry_action;
use super::selection::ToolbarSelection;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    menu_dropdown_button, pill_button, pill_button_width, vertical_divider, ButtonVariant, IconKind,
};
use crate::desktop::UiAction;
use eframe::egui::{self, Vec2};
use opendrive_core::Account;
use uuid::Uuid;

const ACTION_GAP: f32 = 6.5;

fn toolbar_file_op_specs(
    tsel: &ToolbarSelection,
    can_write: bool,
) -> Vec<(IconKind, &'static str, ButtonVariant, UiAction)> {
    let allow_rename = can_write && !tsel.mutable_ids.is_empty() && tsel.can_rename;
    mutable_file_op_items(&tsel.mutable_ids, allow_rename)
}

fn render_toolbar_file_ops(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    tsel: &ToolbarSelection,
    can_write: bool,
    actions: &mut Vec<UiAction>,
) {
    let mut_ok = can_write && !tsel.mutable_ids.is_empty();
    for (icon, lbl, var, act) in toolbar_file_op_specs(tsel, can_write) {
        if pill_button(ui, pal, Some(icon), lbl, var, true, mut_ok) {
            actions.push(act);
        }
    }
}

fn render_clipboard_paste_controls(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    active_acct_id: Option<Uuid>,
    clipboard: Option<&ExplorerClipboard>,
    can_write: bool,
    actions: &mut Vec<UiAction>,
) {
    if let Some(pl) = paste_label_for(clipboard, active_acct_id, can_write) {
        vertical_divider(ui, pal);
        if pill_button(
            ui,
            pal,
            Some(IconKind::Check),
            pl,
            ButtonVariant::TactilePill,
            true,
            true,
        ) {
            actions.push(UiAction::ExplorerPasteClipboard(None));
        }
        if pill_button(
            ui,
            pal,
            Some(IconKind::Close),
            "",
            ButtonVariant::Default,
            true,
            true,
        ) {
            actions.push(UiAction::ExplorerClearClipboard);
        }
    }
}

pub(super) fn action_controls_width(
    ui: &egui::Ui,
    pal: &ThemePalette,
    active_acct: Option<&Account>,
    tsel: &ToolbarSelection,
    clipboard: Option<&ExplorerClipboard>,
) -> f32 {
    use IconKind as I;
    let can_write = active_acct.is_some_and(|a| !a.read_only);
    let pw = |ic, lbl: &str, chev| pill_button_width(ui, pal, Some(ic), lbl, true, chev);
    let count = tsel.selected_ids.len();
    let mut parts = Vec::new();
    let sel_lbl = format!("{count} selected");
    if count > 0 {
        parts.push(pw(I::Close, &sel_lbl, false));
        parts.push(1.5);
    }
    if tsel.single_entry.is_some() {
        parts.push(pw(I::External, "Open", false));
    }
    parts.push(pw(I::CloudDownload, "Sync", true));
    for (ic, lbl, _, _) in toolbar_file_op_specs(tsel, can_write) {
        parts.push(pw(ic, lbl, false));
    }
    if tsel.is_single_video {
        parts.push(pw(I::Copy, "Copy Stream URL", false));
    }
    if let Some(pl) = paste_label_for(clipboard, active_acct.map(|a| a.id), can_write) {
        parts.push(1.5);
        parts.push(pw(I::Check, pl, false));
        parts.push(pw(I::Close, "", false));
    }
    let gaps = parts.len().saturating_sub(1) as f32 * ACTION_GAP;
    parts.into_iter().sum::<f32>() + gaps
}

/// Reserves steady wide-row width so selecting a single item does not wrap the toolbar mid-double-click.
pub(super) fn stable_action_controls_width(
    ui: &egui::Ui,
    pal: &ThemePalette,
    active_acct: Option<&Account>,
    tsel: &ToolbarSelection,
    clipboard: Option<&ExplorerClipboard>,
) -> f32 {
    use IconKind as I;
    let actual = action_controls_width(ui, pal, active_acct, tsel, clipboard);
    let can_write = active_acct.is_some_and(|a| !a.read_only);
    let pw = |ic, lbl: &str, chev| pill_button_width(ui, pal, Some(ic), lbl, true, chev);
    let mut parts = vec![
        pw(I::Close, "99 selected", false),
        1.5,
        pw(I::External, "Open", false),
        pw(I::CloudDownload, "Sync", true),
        pw(I::Edit, "Rename", false),
        pw(I::Copy, "Copy", false),
        pw(I::MoveRight, "Move", false),
        pw(I::Trash, "Delete", false),
        pw(I::Copy, "Copy Stream URL", false),
    ];
    if let Some(pl) = paste_label_for(clipboard, active_acct.map(|a| a.id), can_write) {
        parts.extend([1.5, pw(I::Check, pl, false), pw(I::Close, "", false)]);
    }
    let steady = parts.iter().sum::<f32>() + parts.len().saturating_sub(1) as f32 * ACTION_GAP;
    actual.max(steady)
}

pub(super) fn render_upload_menu(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    up_ok: bool,
    actions: &mut Vec<UiAction>,
) {
    let up_resp = menu_dropdown_button(
        ui,
        pal,
        Some(IconKind::CloudUpload),
        "Upload",
        ButtonVariant::Default,
        up_ok,
    );
    if up_ok {
        egui::Popup::menu(&up_resp).show(|ui| {
            let items = [
                PopupMenuItem::new(
                    IconKind::CloudUpload,
                    "Upload Files",
                    ButtonVariant::Default,
                    true,
                    UiAction::ExplorerSyncUpFiles,
                ),
                PopupMenuItem::new(
                    IconKind::Folder,
                    "Upload Folder",
                    ButtonVariant::Default,
                    true,
                    UiAction::ExplorerSyncUpFolder,
                ),
            ];
            render_popup_menu_items(ui, pal, &items, actions);
        });
    }
}

pub(super) fn render_action_controls(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    active_acct: Option<&Account>,
    tsel: &ToolbarSelection,
    clipboard: Option<&ExplorerClipboard>,
    actions: &mut Vec<UiAction>,
) {
    use ButtonVariant as V;
    use IconKind as I;
    use UiAction as A;
    ui.spacing_mut().item_spacing = Vec2::new(ACTION_GAP, 6.0);
    let has_acct = active_acct.is_some();
    let can_write = active_acct.is_some_and(|a| !a.read_only);
    let count = tsel.selected_ids.len();
    if count > 0 {
        let lbl = format!("{count} selected");
        if pill_button(ui, pal, Some(I::Close), &lbl, V::TactilePill, true, true) {
            actions.push(A::ExplorerSelectEntry(None));
        }
        vertical_divider(ui, pal);
    }
    if let Some(entry) = &tsel.single_entry {
        if pill_button(ui, pal, Some(I::External), "Open", V::Primary, true, true) {
            actions.push(activate_entry_action(entry));
        }
    }

    let sync_ok = has_acct && count > 0;
    let sync_resp =
        menu_dropdown_button(ui, pal, Some(I::CloudDownload), "Sync", V::Default, sync_ok);
    if sync_ok {
        egui::Popup::menu(&sync_resp).show(|ui| {
            let ids = tsel.selected_ids.clone();
            let items = [
                PopupMenuItem::new(
                    I::CloudDownload,
                    "Sync Down",
                    V::Default,
                    true,
                    A::ExplorerSyncDown(ids.clone()),
                ),
                PopupMenuItem::new(
                    I::CloudUpload,
                    "Sync Up",
                    V::Default,
                    can_write,
                    A::ExplorerSyncUpItems(ids.clone()),
                ),
                PopupMenuItem::new(
                    I::CloudClear,
                    "Sync Up & Clear Local",
                    V::Default,
                    can_write,
                    A::ExplorerSyncUpAndClear(ids),
                ),
            ];
            render_popup_menu_items(ui, pal, &items, actions);
        });
    }

    render_toolbar_file_ops(ui, pal, tsel, can_write, actions);
    if tsel.is_single_video
        && pill_button(
            ui,
            pal,
            Some(I::Copy),
            "Copy Stream URL",
            V::TactilePill,
            true,
            true,
        )
    {
        actions.push(A::ExplorerCopyStreamUrl(tsel.selected_ids[0]));
    }
    render_clipboard_paste_controls(
        ui,
        pal,
        active_acct.map(|a| a.id),
        clipboard,
        can_write,
        actions,
    );
}
