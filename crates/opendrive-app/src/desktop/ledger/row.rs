use super::{render_alias_editor, render_path_usage};
use crate::design::formatters::{derive_monogram, join_mount_path, status_label, status_tone};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    account_avatar, pill_button, status_pill, thin_usage_bar, ButtonVariant, IconKind,
};
use crate::design::widgets_controls::tactile_switch_tip;
use crate::desktop::drawer::render_oauth_pending;
use crate::desktop::UiAction;
use crate::runtime::auth_runner::AuthSession;
use crate::runtime::bridge_types::AliasEditDraft;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use opendrive_core::{default_account_alias, Account, AccountStatus};

fn row_btn(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: IconKind,
    label: &str,
    variant: ButtonVariant,
) -> bool {
    pill_button(ui, pal, Some(icon), label, variant, true, true)
}

fn render_account_actions(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    is_editing: bool,
    is_pending: bool,
    (idx, total): (usize, usize),
    actions: &mut Vec<UiAction>,
) {
    let is_paused = acct.status == AccountStatus::Paused || !acct.enabled;
    let needs_reauth = matches!(
        acct.status,
        AccountStatus::NeedsReauth | AccountStatus::Error
    );
    let id = acct.id.to_string();

    if needs_reauth {
        let is_sa = acct.credential_set_id.is_nil();
        if pill_button(
            ui,
            pal,
            Some(IconKind::External),
            "Sign In",
            ButtonVariant::Primary,
            true,
            !(is_pending && is_sa),
        ) {
            actions.push(if is_pending {
                UiAction::OpenAuthBrowser
            } else {
                UiAction::ReauthAccount {
                    id: id.clone(),
                    open_browser: true,
                    copy_link: false,
                }
            });
        }
        if !is_sa
            && row_btn(
                ui,
                pal,
                IconKind::Copy,
                "Copy Link",
                ButtonVariant::TactilePill,
            )
        {
            actions.push(if is_pending {
                UiAction::CopyAuthLink
            } else {
                UiAction::ReauthAccount {
                    id: id.clone(),
                    open_browser: false,
                    copy_link: true,
                }
            });
        }
    }
    let (sync_tip, sync_lbl, sync_col) = if is_paused {
        (
            "Sync paused for this account (click to resume)",
            "Paused",
            pal.text_muted,
        )
    } else {
        (
            "Sync active for this account (click to pause)",
            "Sync On",
            pal.text_primary,
        )
    };
    ui.horizontal(|ui| {
        if tactile_switch_tip(ui, pal, !is_paused, true, sync_tip) {
            actions.push(UiAction::AccountAction {
                action: if is_paused { "resume" } else { "pause" },
                id: Some(id.clone()),
            });
        }
        ui.label(RichText::new(sync_lbl).size(11.5).strong().color(sync_col));
    });

    let (ro_icon, ro_lbl) = if acct.read_only {
        (IconKind::Lock, "Read-Only")
    } else {
        (IconKind::Unlock, "Read-Write")
    };
    let ro_var = ButtonVariant::Toggle {
        selected: acct.read_only,
    };
    if row_btn(ui, pal, ro_icon, ro_lbl, ro_var) {
        actions.push(UiAction::AccountAction {
            action: "toggle_readonly",
            id: Some(id.clone()),
        });
    }
    let alias_var = ButtonVariant::Toggle {
        selected: is_editing,
    };
    if row_btn(ui, pal, IconKind::Edit, "Alias", alias_var) {
        if is_editing {
            actions.push(UiAction::CancelAlias);
        } else {
            actions.push(UiAction::StartEditAlias(id.clone()));
        }
    }
    if row_btn(ui, pal, IconKind::Trash, "Remove", ButtonVariant::Danger) {
        actions.push(UiAction::AccountAction {
            action: "remove",
            id: Some(id.clone()),
        });
    }
    if total > 1 {
        for (icon, act, en) in [
            (IconKind::ChevronUp, "move_up", idx > 0),
            (IconKind::ChevronDown, "move_down", idx + 1 < total),
        ] {
            if pill_button(ui, pal, Some(icon), "", ButtonVariant::Default, true, en) {
                actions.push(UiAction::AccountAction {
                    action: act,
                    id: Some(id.clone()),
                });
            }
        }
    }
}

fn render_header_right(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    path: &str,
    actions: &mut Vec<UiAction>,
) {
    status_pill(
        ui,
        pal,
        status_label(&acct.status),
        status_tone(&acct.status),
    );
    if row_btn(ui, pal, IconKind::Folder, "Open", ButtonVariant::Default) {
        actions.push(UiAction::OpenAccountFolder(path.to_owned()));
    }
    if row_btn(
        ui,
        pal,
        IconKind::Explore,
        "Explore",
        ButtonVariant::Primary,
    ) {
        actions.push(UiAction::OpenExplorer(acct.id.to_string()));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_account_row(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    (idx, total): (usize, usize),
    mount_root: &str,
    avatar: Option<&egui::TextureHandle>,
    alias_edit: &mut Option<AliasEditDraft>,
    auth_session: Option<&AuthSession>,
    actions: &mut Vec<UiAction>,
) {
    let display_alias = default_account_alias(&acct.nickname, &acct.email);
    let monogram = derive_monogram(&display_alias, &acct.email);
    let email = if acct.email.is_empty() {
        "CLI OAuth pending"
    } else {
        &acct.email
    };
    let path = join_mount_path(mount_root, &acct.folder_name);
    let is_editing = alias_edit.as_ref().is_some_and(|e| e.account_id == acct.id);
    let row_pending = auth_session.filter(|s| s.target_account == Some(acct.id));
    let ring = if idx.is_multiple_of(2) {
        pal.pop_pink
    } else {
        pal.pop_cyan
    };

    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                account_avatar(ui, pal, avatar, &monogram, ring, 42.0);
                ui.add_space(6.0);
                if ui.available_width() < 560.0 {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&display_alias)
                                    .size(15.0)
                                    .strong()
                                    .color(pal.text_primary),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                render_header_right(ui, pal, acct, &path, actions);
                            });
                        });
                        ui.label(RichText::new(email).size(12.5).color(pal.text_muted));
                    });
                } else {
                    ui.label(
                        RichText::new(&display_alias)
                            .size(15.0)
                            .strong()
                            .color(pal.text_primary),
                    );
                    ui.label(RichText::new(email).size(12.5).color(pal.text_muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        render_header_right(ui, pal, acct, &path, actions);
                    });
                }
            });

            ui.add_space(8.0);
            let pending = row_pending.is_some();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                render_account_actions(ui, pal, acct, is_editing, pending, (idx, total), actions);
            });
            if is_editing {
                if let Some(edit) = alias_edit.as_mut() {
                    ui.add_space(8.0);
                    render_alias_editor(ui, pal, acct, edit, actions);
                }
            }
            if let Some(session) = row_pending {
                ui.add_space(8.0);
                render_oauth_pending(ui, pal, session, actions);
            }
            ui.add_space(8.0);
            render_path_usage(ui, pal, &path, acct.storage_used, acct.storage_quota);
            if acct.storage_quota > 0 && acct.storage_quota >= acct.storage_used {
                ui.add_space(6.0);
                let ratio = (acct.storage_used as f32 / acct.storage_quota as f32).clamp(0.0, 1.0);
                thin_usage_bar(ui, pal, ratio);
            }
        });
}
