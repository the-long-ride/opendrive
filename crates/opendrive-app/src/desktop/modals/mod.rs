//! Centered desktop modals: Settings, Open With Rules, Setup Guide, and Release dialog.

pub mod font_ui;
pub mod guide;
mod guide_content;
mod guide_visuals;
mod open_rules_actions;
pub mod open_rules_ui;
pub mod release;
pub mod settings;
pub mod support;

use crate::design::formatters::{
    bandwidth_to_step, cache_to_step, compute_cache_ratio, format_bandwidth, format_bytes,
    step_to_bandwidth, step_to_cache, BANDWIDTH_STEPS, CACHE_STEPS_BYTES,
};
use crate::design::theme::ThemePalette;
use crate::design::tooltip::attach_smart_tooltip;
use crate::design::widgets::{pill_button, singleline_input, ButtonVariant, IconKind};
use crate::design::widgets_controls::tactile_step_slider;
use crate::desktop::UiAction;
use crate::runtime::bridge::AppStateResponse;
use crate::runtime::bridge_types::VaultBackupDraft;
use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, Frame, Layout, Margin, RichText, Sense, Stroke,
};

const CHANGELOG_URL: &str = "https://github.com/the-long-ride/opendrive/blob/main/CHANGELOG.md";
const AUTHOR_URL: &str = "https://the-long-ride.github.io/";

pub(super) fn render_version_changelog_link(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    actions: &mut Vec<UiAction>,
) {
    let ver_label = concat!("v", env!("CARGO_PKG_VERSION"));
    let resp = ui
        .add(
            egui::Label::new(RichText::new(ver_label).size(12.5).color(pal.text_muted))
                .sense(Sense::click()),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    if resp.hovered() {
        ui.painter().line_segment(
            [
                egui::Pos2::new(resp.rect.left(), resp.rect.bottom() - 1.0),
                egui::Pos2::new(resp.rect.right(), resp.rect.bottom() - 1.0),
            ],
            Stroke::new(1.0, pal.text_muted),
        );
    }
    attach_smart_tooltip(&resp, ui, pal, "Open Changelog on GitHub");
    if resp.clicked() {
        actions.push(UiAction::OpenExternalUrl(CHANGELOG_URL));
    }
}

pub(super) fn render_bandwidth_and_cache(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    state: &AppStateResponse,
    actions: &mut Vec<UiAction>,
) {
    let settings = &state.settings;
    let bw_step = bandwidth_to_step(settings.bandwidth_limit);
    let bw_lbl = format_bandwidth(settings.bandwidth_limit);
    let cache_pct = compute_cache_ratio(state.stats.cache_bytes, settings.cache_max_size);
    let cache_readout = format!(
        "{} / {} ({cache_pct}%)",
        format_bytes(state.stats.cache_bytes),
        format_bytes(settings.cache_max_size),
    );
    let cache_step = cache_to_step(settings.cache_max_size);
    let mut next_bw = None;
    let mut next_cache = None;
    if ui.available_width() >= 420.0 {
        ui.columns(2, |cols| {
            next_bw = tactile_step_slider(
                &mut cols[0],
                pal,
                bw_step,
                BANDWIDTH_STEPS.len(),
                "Bandwidth",
                &bw_lbl,
            );
            next_cache = tactile_step_slider(
                &mut cols[1],
                pal,
                cache_step,
                CACHE_STEPS_BYTES.len(),
                "Cache Limit",
                &cache_readout,
            );
        });
    } else {
        next_bw = tactile_step_slider(
            ui,
            pal,
            bw_step,
            BANDWIDTH_STEPS.len(),
            "Bandwidth",
            &bw_lbl,
        );
        ui.add_space(10.0);
        next_cache = tactile_step_slider(
            ui,
            pal,
            cache_step,
            CACHE_STEPS_BYTES.len(),
            "Cache Limit",
            &cache_readout,
        );
    }
    if let Some(next_step) = next_bw {
        actions.push(UiAction::SetBandwidth(step_to_bandwidth(next_step)));
    }
    if let Some(next_step) = next_cache {
        actions.push(UiAction::SetCacheMax(step_to_cache(next_step)));
    }
}

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
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(
                        RichText::new("Author:")
                            .size(13.0)
                            .strong()
                            .color(pal.text_primary),
                    );
                    let author_resp = ui
                        .add(
                            egui::Label::new(
                                RichText::new("the-long-ride")
                                    .size(13.0)
                                    .strong()
                                    .color(pal.text_primary),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    ui.painter().line_segment(
                        [
                            egui::Pos2::new(
                                author_resp.rect.left(),
                                author_resp.rect.bottom() - 1.0,
                            ),
                            egui::Pos2::new(
                                author_resp.rect.right(),
                                author_resp.rect.bottom() - 1.0,
                            ),
                        ],
                        Stroke::new(1.0, pal.text_primary),
                    );
                    attach_smart_tooltip(&author_resp, ui, pal, AUTHOR_URL);
                    if author_resp.clicked() {
                        actions.push(UiAction::OpenExternalUrl(AUTHOR_URL));
                    }
                });
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
