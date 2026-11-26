//! Centered desktop modals: Settings, Open With Rules, Setup Guide, and Release dialog.

pub mod guide;
mod guide_content;
mod open_rules_actions;
pub mod open_rules_ui;
pub mod release;
pub mod settings;

use crate::design::theme::ThemePalette;
use crate::design::widgets::{pill_button, singleline_input, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::bridge_types::VaultBackupDraft;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};

pub(super) fn render_vault_backup_section(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    vault: &mut VaultBackupDraft,
    actions: &mut Vec<UiAction>,
) {
    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("Encrypted Credential Backup (.odcred)")
                    .size(14.0)
                    .strong()
                    .color(pal.text_primary),
            );
            ui.label(
                RichText::new(
                    "Export or import accounts & secrets across machines (9+ chars: uppercase, lowercase, number, symbol).",
                )
                .size(12.0)
                .color(pal.text_muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 76.0).max(120.0);
                ui.add_sized(
                    [w, 30.0],
                    singleline_input(&mut vault.password, "Vault Password")
                        .password(!vault.show_password),
                );
                let lbl = if vault.show_password { "Hide" } else { "Show" };
                if pill_button(ui, pal, None, lbl, ButtonVariant::Default, true, true) {
                    actions.push(UiAction::ToggleVaultSecret);
                }
            });
            ui.add_space(6.0);
            ui.add_sized(
                [ui.available_width(), 30.0],
                singleline_input(&mut vault.confirm_password, "Confirm Password (for Export)")
                    .password(!vault.show_password),
            );
            if let Some(msg) = &vault.feedback {
                ui.add_space(6.0);
                let fg = if vault.feedback_is_err {
                    pal.accent_coral
                } else {
                    pal.ok_fg
                };
                ui.label(RichText::new(msg).size(12.0).color(fg));
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Key),
                    "Export (.odcred)",
                    ButtonVariant::Primary,
                    true,
                    true,
                ) {
                    actions.push(UiAction::ExportVault);
                }
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Folder),
                    "Import (.odcred)",
                    ButtonVariant::TactilePill,
                    true,
                    true,
                ) {
                    actions.push(UiAction::ImportVault);
                }
            });
        });
}

pub(super) fn render_about_footer(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    actions: &mut Vec<UiAction>,
) {
    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Author: the-long-ride")
                        .size(13.0)
                        .strong()
                        .color(pal.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Star),
                        "Star on GitHub",
                        ButtonVariant::PastelYellow,
                        true,
                        true,
                    ) {
                        actions.push(UiAction::OpenExternalUrl(
                            "https://github.com/the-long-ride/opendrive",
                        ));
                    }
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Heart),
                        "Donate",
                        ButtonVariant::PastelPink,
                        true,
                        true,
                    ) {
                        actions.push(UiAction::OpenExternalUrl(
                            "https://github.com/the-long-ride/#donate",
                        ));
                    }
                });
            });
        });
}
