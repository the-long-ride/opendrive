use super::render_alias_editor;
use crate::design::formatters::{
    derive_monogram, format_account_storage, join_mount_path, status_label, status_tone,
};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    account_avatar, pill_button, status_pill, thin_usage_bar, ButtonVariant, IconKind,
};
use crate::design::widgets_controls::tactile_switch;
use crate::desktop::drawer::render_oauth_pending;
use crate::desktop::UiAction;
use crate::runtime::auth_runner::AuthSession;
use crate::runtime::bridge_types::AliasEditDraft;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use opendrive_core::{default_account_alias, Account, AccountStatus};

fn render_account_actions(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    is_editing: bool,
    actions: &mut Vec<UiAction>,
) {
    let is_paused = acct.status == AccountStatus::Paused || !acct.enabled;
    let needs_reauth = matches!(
        acct.status,
        AccountStatus::NeedsReauth | AccountStatus::Error
    );
    let id = acct.id.to_string();

    if needs_reauth {
        if pill_button(
            ui,
            pal,
            Some(IconKind::External),
            "Sign In",
            ButtonVariant::Primary,
            true,
            true,
        ) {
            actions.push(UiAction::ReauthAccount {
                id: id.clone(),
                open_browser: true,
                copy_link: false,
            });
        }
        if pill_button(
            ui,
            pal,
            Some(IconKind::Copy),
            "Copy Link",
            ButtonVariant::TactilePill,
            true,
            true,
        ) {
            actions.push(UiAction::ReauthAccount {
                id: id.clone(),
                open_browser: false,
                copy_link: true,
            });
        }
    }
    if tactile_switch(ui, pal, !is_paused, true) {
        actions.push(UiAction::AccountAction {
            action: if is_paused { "resume" } else { "pause" },
            id: Some(id.clone()),
        });
    }
    ui.label(
        RichText::new(if is_paused { "Paused" } else { "Sync On" })
            .size(11.5)
            .strong()
            .color(if is_paused {
                pal.text_muted
            } else {
                pal.text_primary
            }),
    );
    let (ro_icon, ro_lbl) = if acct.read_only {
        (IconKind::Lock, "Read-Only")
    } else {
        (IconKind::Unlock, "Read-Write")
    };
    if pill_button(
        ui,
        pal,
        Some(ro_icon),
        ro_lbl,
        ButtonVariant::Toggle {
            selected: acct.read_only,
        },
        true,
        true,
    ) {
        actions.push(UiAction::AccountAction {
            action: "toggle_readonly",
            id: Some(id.clone()),
        });
    }
    if pill_button(
        ui,
        pal,
        Some(IconKind::Edit),
        "Alias",
        ButtonVariant::Toggle {
            selected: is_editing,
        },
        true,
        true,
    ) {
        if is_editing {
            actions.push(UiAction::CancelAlias);
        } else {
            actions.push(UiAction::StartEditAlias(id.clone()));
        }
    }
    if pill_button(
        ui,
        pal,
        Some(IconKind::Trash),
        "Remove",
        ButtonVariant::Danger,
        true,
        true,
    ) {
        actions.push(UiAction::AccountAction {
            action: "remove",
            id: Some(id),
        });
    }
}

fn render_path_usage(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    path: &str,
    used: u64,
    quota: u64,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal_wrapped(|ui| {
        if pill_button(
            ui,
            pal,
            Some(IconKind::Folder),
            "Open",
            ButtonVariant::Default,
            true,
            true,
        ) {
            actions.push(UiAction::OpenAccountFolder(path.to_owned()));
        }
        ui.label(
            RichText::new(path)
                .size(12.0)
                .monospace()
                .color(pal.text_muted),
        );
        if let Some(quota_lbl) = format_account_storage(used, quota) {
            ui.label(
                RichText::new(format!("· {quota_lbl}"))
                    .size(12.0)
                    .color(pal.text_muted),
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_account_row(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    idx: usize,
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
            let narrow = ui.available_width() < 760.0;
            ui.horizontal(|ui| {
                account_avatar(ui, pal, avatar, &monogram, ring);
                ui.add_space(6.0);
                let narrow_hdr = ui.available_width() < 420.0;
                let pill_lbl = status_label(&acct.status);
                let pill_tone = status_tone(&acct.status);
                if narrow_hdr {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&display_alias)
                                    .size(15.0)
                                    .strong()
                                    .color(pal.text_primary),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                status_pill(ui, pal, pill_lbl, pill_tone);
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
                        status_pill(ui, pal, pill_lbl, pill_tone);
                    });
                }
            });

            ui.add_space(8.0);
            if narrow {
                ui.horizontal_wrapped(|ui| {
                    render_account_actions(ui, pal, acct, is_editing, actions)
                });
            } else {
                ui.horizontal(|ui| {
                    render_account_actions(ui, pal, acct, is_editing, actions);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        render_path_usage(
                            ui,
                            pal,
                            &path,
                            acct.storage_used,
                            acct.storage_quota,
                            actions,
                        );
                    });
                });
            }

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
            if narrow {
                ui.add_space(6.0);
                render_path_usage(
                    ui,
                    pal,
                    &path,
                    acct.storage_used,
                    acct.storage_quota,
                    actions,
                );
            }
            if acct.storage_quota > 0 {
                ui.add_space(8.0);
                let ratio = (acct.storage_used as f32 / acct.storage_quota as f32).clamp(0.0, 1.0);
                thin_usage_bar(ui, pal, ratio);
            }
        });
}
