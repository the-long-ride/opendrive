//! Cloud Explorer sync-state vector pills, context menus, clipboard toolbar, and shortcuts.

use super::file_ops_modal::ExplorerClipboard;
use super::items::activate_entry_action;
use super::selection::SelectionRef;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{icon_status_pill, menu_pill_button, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::explorer_ops::ExplorerEntry;
use eframe::egui::{self, FontId, Vec2};
use opendrive_core::MediaKind;
use opendrive_sync::{ExplorerSyncStatus, NodeId, SHARED_WITH_ME_DRIVE_ID};
use uuid::Uuid;

pub(crate) fn is_mutable_entry(entry: &ExplorerEntry) -> bool {
    entry.drive_id != "root" && entry.drive_id != SHARED_WITH_ME_DRIVE_ID
}

pub(super) fn render_sync_state_pill(ui: &mut egui::Ui, pal: &ThemePalette, entry: &ExplorerEntry) {
    let (bg, fg, border) = (pal.cloud_tint, pal.cloud_fg, pal.cloud_border);
    let (icon, tip, c_bg, c_fg, c_bd) = match entry.sync_status {
        ExplorerSyncStatus::Cloud => (
            IconKind::Cloud,
            "Cloud — Streamed on demand (not stored locally)",
            bg,
            fg,
            border,
        ),
        ExplorerSyncStatus::Indexed => (
            IconKind::FolderIndexed,
            "Indexed — Folder listing cached (files remain in cloud)",
            pal.pill_neutral,
            pal.text_muted,
            pal.card_border,
        ),
        ExplorerSyncStatus::PartialLocal => (
            IconKind::DownloadPartial,
            "Partially Local — Some items inside are synced locally",
            pal.accent_blue_tint,
            pal.pop_cyan,
            pal.pop_cyan,
        ),
        ExplorerSyncStatus::FullLocal => (
            IconKind::DownloadCheck,
            "Local — Fully downloaded and available offline",
            pal.ok_tint,
            pal.ok_fg,
            pal.ok_border,
        ),
        ExplorerSyncStatus::PendingUpload => (
            IconKind::CloudUpload,
            "Pending Upload — Local changes waiting to sync",
            pal.warn_tint,
            pal.warn_fg,
            pal.warn_border,
        ),
        ExplorerSyncStatus::Conflict => (
            IconKind::Alert,
            "Conflict — Both local and cloud versions changed",
            pal.accent_coral_tint,
            pal.accent_coral,
            pal.error_border,
        ),
        ExplorerSyncStatus::Syncing => (
            IconKind::Refresh,
            "Syncing — Downloading file from Google Drive",
            bg,
            fg,
            border,
        ),
    };
    icon_status_pill(ui, pal, icon, tip, (c_bg, c_fg, c_bd));
}

pub(super) fn paste_label_for(
    clipboard: Option<&ExplorerClipboard>,
    account_id: Option<Uuid>,
    can_write: bool,
) -> Option<&'static str> {
    let clip = clipboard?;
    if !can_write || Some(clip.account_id) != account_id {
        return None;
    }
    Some(if clip.is_cut {
        "Move Here"
    } else {
        "Paste Copy Here"
    })
}

pub(super) fn mutable_file_op_items(
    ids: &[NodeId],
    allow_rename: bool,
) -> Vec<(IconKind, &'static str, ButtonVariant, UiAction)> {
    let mut items = Vec::new();
    if allow_rename && ids.len() == 1 {
        items.push((
            IconKind::Edit,
            "Rename",
            ButtonVariant::Default,
            UiAction::ExplorerStartRename(ids[0]),
        ));
    }
    let batch = ids.to_vec();
    items.extend([
        (
            IconKind::Copy,
            "Copy",
            ButtonVariant::Default,
            UiAction::ExplorerSetClipboard {
                node_ids: batch.clone(),
                cut: false,
            },
        ),
        (
            IconKind::MoveRight,
            "Move",
            ButtonVariant::Default,
            UiAction::ExplorerSetClipboard {
                node_ids: batch.clone(),
                cut: true,
            },
        ),
        (
            IconKind::Trash,
            "Delete",
            ButtonVariant::Danger,
            UiAction::ExplorerRequestDelete(batch),
        ),
    ]);
    items
}

pub(super) struct PopupMenuItem<'a> {
    pub icon: IconKind,
    pub label: &'a str,
    pub variant: ButtonVariant,
    pub enabled: bool,
    pub action: UiAction,
}

impl<'a> PopupMenuItem<'a> {
    pub fn new(
        icon: IconKind,
        label: &'a str,
        variant: ButtonVariant,
        enabled: bool,
        action: UiAction,
    ) -> Self {
        Self {
            icon,
            label,
            variant,
            enabled,
            action,
        }
    }

    pub fn def(icon: IconKind, label: &'a str, action: UiAction) -> Self {
        Self::new(icon, label, ButtonVariant::Default, true, action)
    }
}

pub(super) fn render_popup_menu_items(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    items: &[PopupMenuItem<'_>],
    actions: &mut Vec<UiAction>,
) {
    let font = FontId::proportional(12.0);
    let menu_w = items.iter().fold(188.0_f32, |acc, item| {
        let tw = ui
            .painter()
            .layout_no_wrap(item.label.to_owned(), font.clone(), pal.text_primary)
            .size()
            .x;
        acc.max(48.0 + tw)
    });
    ui.set_min_width(menu_w);
    ui.set_max_width(menu_w);
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
    for item in items {
        if menu_pill_button(
            ui,
            pal,
            Some(item.icon),
            item.label,
            item.variant,
            menu_w,
            item.enabled,
        ) {
            actions.push(item.action.clone());
            ui.close();
        }
    }
}

pub(super) fn render_entry_context_menu(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    entry: &ExplorerEntry,
    sel: SelectionRef<'_>,
    can_write: bool,
    paste_label: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    use ButtonVariant as V;
    use IconKind as I;
    use UiAction as A;
    let targets: Vec<NodeId> = if sel.is_selected(entry.id) && sel.selected_ids.len() > 1 {
        sel.selected_ids.to_vec()
    } else {
        vec![entry.id]
    };
    let mutable_targets = sel.mutable_ids(&targets);
    let multi = targets.len() > 1;
    let mut items = Vec::new();
    if !multi {
        items.push(PopupMenuItem::def(
            I::External,
            "Open",
            activate_entry_action(entry),
        ));
    }
    items.push(PopupMenuItem::def(
        I::CloudDownload,
        "Sync Down (Pin Local)",
        A::ExplorerSyncDown(targets.clone()),
    ));
    if can_write {
        items.push(PopupMenuItem::def(
            I::CloudUpload,
            "Sync Up",
            A::ExplorerSyncUpItems(targets.clone()),
        ));
        items.push(PopupMenuItem::def(
            I::CloudClear,
            "Sync Up & Clear Local",
            A::ExplorerSyncUpAndClear(targets),
        ));
        if !mutable_targets.is_empty() {
            let allow_ren = !multi && mutable_targets.len() == 1;
            for (icon, lbl, var, act) in mutable_file_op_items(&mutable_targets, allow_ren) {
                items.push(PopupMenuItem::new(icon, lbl, var, true, act));
            }
        }
        if let Some(pl) = paste_label {
            let dest =
                (entry.is_folder && entry.drive_id != SHARED_WITH_ME_DRIVE_ID).then_some(entry.id);
            items.push(PopupMenuItem::new(
                I::Check,
                pl,
                V::TactilePill,
                true,
                A::ExplorerPasteClipboard(dest),
            ));
        }
    }
    if !multi && entry.media_kind == MediaKind::Video {
        items.push(PopupMenuItem::def(
            I::Copy,
            "Copy Stream URL",
            A::ExplorerCopyStreamUrl(entry.id),
        ));
    }
    render_popup_menu_items(ui, pal, &items, actions);
}
