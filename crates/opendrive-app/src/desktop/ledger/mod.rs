//! Neo-Brutalist Connected Accounts card with inline alias editing, avatars, and bottom pastel usage bars.

mod row;

use self::row::render_account_row;
use crate::design::formatters::NewAccountDraft;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card_with_header, check_bullet_row, pill_button, BentoLayerAccent, ButtonVariant,
    IconKind,
};
use crate::desktop::drawer::render_connect_drawer;
use crate::desktop::UiAction;
use crate::runtime::auth_runner::AuthSession;
use crate::runtime::bridge::AppStateResponse;
use crate::runtime::bridge_types::AliasEditDraft;
use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};
use opendrive_core::{default_account_alias, Account};
use std::collections::HashMap;
use uuid::Uuid;

pub(super) fn render_alias_editor(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    edit: &mut AliasEditDraft,
    actions: &mut Vec<UiAction>,
) {
    let hint = default_account_alias("", &acct.email);
    Frame::new()
        .fill(pal.card_bg)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Alias:").size(12.5).color(pal.text_muted));
                let resp = ui.add_sized(
                    [180.0, 28.0],
                    egui::TextEdit::singleline(&mut edit.nickname).hint_text(hint),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    actions.push(UiAction::SaveAlias);
                }
                ui.checkbox(&mut edit.rename_folder, "Also rename local mount folder");
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Check),
                    "Save",
                    ButtonVariant::Primary,
                    true,
                    true,
                ) {
                    actions.push(UiAction::SaveAlias);
                }
                if pill_button(ui, pal, None, "Cancel", ButtonVariant::Default, true, true) {
                    actions.push(UiAction::CancelAlias);
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
pub fn render_accounts_ledger(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    state: &AppStateResponse,
    drawer_open: bool,
    reauth_target_name: Option<&str>,
    draft: &mut NewAccountDraft,
    alias_edit: &mut Option<AliasEditDraft>,
    auth_session: Option<&AuthSession>,
    form_error: Option<&str>,
    avatars: &HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<UiAction>,
) {
    let count = state.accounts.len();
    let count_meta = format!(
        "{count} {}",
        if count == 1 { "account" } else { "accounts" }
    );
    let mut toggle_drawer_clicked = false;
    bento_card_with_header(
        ui,
        pal,
        BentoLayerAccent::Pink,
        "Connected Accounts",
        None,
        |ui| {
            let (icon, lbl, var) = if drawer_open {
                (IconKind::Close, "Close", ButtonVariant::TactilePill)
            } else {
                (IconKind::Plus, "Connect", ButtonVariant::Primary)
            };
            if pill_button(ui, pal, Some(icon), lbl, var, true, true) {
                toggle_drawer_clicked = true;
            }
            ui.add_space(8.0);
            ui.label(RichText::new(count_meta).size(12.5).color(pal.text_muted));
        },
        |ui| {
            if drawer_open {
                render_connect_drawer(
                    ui,
                    pal,
                    reauth_target_name,
                    draft,
                    auth_session,
                    form_error,
                    actions,
                );
            }
            if state.accounts.is_empty() && !drawer_open {
                check_bullet_row(ui, pal, "Multi-account sync into unified local mount roots");
                ui.add_space(6.0);
                check_bullet_row(
                    ui,
                    pal,
                    &format!("Virtual filesystem rooted at {}", state.mount_root),
                );
                ui.add_space(6.0);
                check_bullet_row(
                    ui,
                    pal,
                    "Sign in via Google browser OAuth (with Copy Link) or Service Account JSON",
                );
                ui.add_space(14.0);
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Plus),
                    "Add First Account",
                    ButtonVariant::TactilePill,
                    false,
                    true,
                ) {
                    actions.push(UiAction::ToggleDrawer(Some(true)));
                }
            } else {
                for (idx, acct) in state.accounts.iter().enumerate() {
                    if idx > 0 {
                        ui.add_space(10.0);
                    }
                    render_account_row(
                        ui,
                        pal,
                        acct,
                        idx,
                        &state.mount_root,
                        avatars.get(&acct.id),
                        alias_edit,
                        auth_session,
                        actions,
                    );
                }
            }
        },
    );
    if toggle_drawer_clicked {
        actions.push(UiAction::ToggleDrawer(None));
    }
}
