//! Centered Settings Modal with OS/VFS/bandwidth/cache preferences and encrypted .odcred credential backup.

use crate::design::formatters::{
    bandwidth_to_step, cache_to_step, compute_cache_ratio, format_bandwidth, format_bytes,
    step_to_bandwidth, step_to_cache, BANDWIDTH_STEPS, CACHE_STEPS_BYTES,
};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{bento_card, pill_button, BentoLayerAccent, ButtonVariant, IconKind};
use crate::design::widgets_controls::{setting_switch_row, tactile_step_slider, tactile_switch};
use crate::desktop::UiAction;
use crate::runtime::bridge::AppStateResponse;
use crate::runtime::bridge_types::VaultBackupDraft;
use eframe::egui::{
    self, Align, Color32, Frame, Id, Layout, Margin, RichText, ScrollArea, Stroke, TextEdit, Vec2,
};
use opendrive_core::DocsMode;

fn render_vault_backup_section(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    vault: &mut VaultBackupDraft,
    actions: &mut Vec<UiAction>,
) {
    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(egui::CornerRadius::same(10))
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
                    TextEdit::singleline(&mut vault.password)
                        .password(!vault.show_password)
                        .hint_text("Vault Password"),
                );
                let lbl = if vault.show_password { "Hide" } else { "Show" };
                if pill_button(ui, pal, None, lbl, ButtonVariant::Default, true, true) {
                    actions.push(UiAction::ToggleVaultSecret);
                }
            });
            ui.add_space(6.0);
            ui.add_sized(
                [ui.available_width(), 30.0],
                TextEdit::singleline(&mut vault.confirm_password)
                    .password(!vault.show_password)
                    .hint_text("Confirm Password (for Export)"),
            );
            if let Some(msg) = &vault.feedback {
                ui.add_space(6.0);
                let fg = if vault.feedback_is_err { pal.accent_coral } else { pal.ok_fg };
                ui.label(RichText::new(msg).size(12.0).color(fg));
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if pill_button(ui, pal, Some(IconKind::Key), "Export (.odcred)", ButtonVariant::Primary, true, true) {
                    actions.push(UiAction::ExportVault);
                }
                if pill_button(ui, pal, Some(IconKind::Folder), "Import (.odcred)", ButtonVariant::TactilePill, true, true) {
                    actions.push(UiAction::ImportVault);
                }
            });
        });
}

pub fn render_settings_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    state: &AppStateResponse,
    vault: &mut VaultBackupDraft,
    release_checking: bool,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::ToggleSettings(Some(false)));
        return;
    }

    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = egui::Rect::from_min_max(egui::Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("settings_modal_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::ToggleSettings(Some(false)));
            }
        });

    let modal_w = (overlay.width() - 64.0).clamp(320.0, 640.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 160.0).clamp(120.0, 480.0);
    egui::Area::new(Id::unique("settings_modal_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            bento_card(
                ui,
                pal,
                BentoLayerAccent::Dual,
                "OpenDrive Settings",
                Some("Configure startup, virtual drive mapping, cache/bandwidth, and encrypted backups."),
                None,
                |ui| {
                    ScrollArea::vertical()
                        .max_height(max_body_h)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                        let settings = &state.settings;
                        if setting_switch_row(
                            ui,
                            pal,
                            settings.autostart,
                            "Autostart on Login",
                            "Launch OpenDrive automatically when signing in",
                            "",
                        ) {
                            actions.push(UiAction::ToggleAutostart);
                        }

                        ui.add_space(10.0);
                        let vd_supported = opendrive_platform::is_virtual_drive_supported();
                        let vd_title = format!("Mount Virtual Drive ({}:\\)", settings.virtual_drive_letter);
                        let vd_sub = if vd_supported {
                            format!(
                                "Map {} as {}: under This PC (off for sidebar folder only)",
                                state.mount_root, settings.virtual_drive_letter
                            )
                        } else {
                            "Windows only — maps OpenDrive folder to a drive letter".into()
                        };
                        if setting_switch_row(ui, pal, settings.virtual_drive_enabled, &vd_title, &vd_sub, "") {
                            actions.push(UiAction::ToggleVirtualDrive);
                        }

                        ui.add_space(10.0);
                        let export_office = settings.docs_mode == DocsMode::ExportOffice;
                        if setting_switch_row(
                            ui,
                            pal,
                            export_office,
                            "Export Google Docs as Office Files",
                            "Save Docs, Sheets, and Slides locally instead of .gdoc web links",
                            "",
                        ) {
                            actions.push(UiAction::SetDocsMode(if export_office {
                                DocsMode::Link
                            } else {
                                DocsMode::ExportOffice
                            }));
                        }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.set_min_width(ui.available_width());
                            if tactile_switch(ui, pal, settings.check_updates, true) {
                                actions.push(UiAction::ToggleCheckUpdates);
                            }
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("Check for GitHub Releases")
                                        .size(14.5)
                                        .strong()
                                        .color(pal.text_primary),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("Notify when a newer release is published (off by default)")
                                            .size(12.5)
                                            .color(pal.text_muted),
                                    )
                                    .wrap(),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let lbl = if release_checking { "Checking..." } else { "Check Now" };
                                if pill_button(
                                    ui,
                                    pal,
                                    Some(IconKind::Refresh),
                                    lbl,
                                    ButtonVariant::Default,
                                    true,
                                    !release_checking,
                                ) {
                                    actions.push(UiAction::CheckReleasesNow);
                                }
                            });
                        });

                        ui.add_space(12.0);
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

                        ui.add_space(14.0);
                        render_vault_backup_section(ui, pal, vault, actions);
                        ui.add_space(10.0);
                        super::render_about_footer(ui, pal, actions);
                    });

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if pill_button(ui, pal, Some(IconKind::Purge), "Purge Unpinned Cache", ButtonVariant::Default, true, true) {
                            actions.push(UiAction::PurgeCache);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(ui, pal, Some(IconKind::Check), "Done", ButtonVariant::Primary, true, true) {
                                actions.push(UiAction::ToggleSettings(Some(false)));
                            }
                        });
                    });
                },
            );
        });
}
