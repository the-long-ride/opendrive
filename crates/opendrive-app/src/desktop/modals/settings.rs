//! Centered Settings Modal with OS/VFS/bandwidth/cache, Open With rules, and encrypted .odcred backup.

use super::font_ui::{render_font_section, FontPickerDraft};
use super::open_rules_ui::{render_open_rules_section, OpenRuleDraft};
use super::{
    render_about_footer, render_bandwidth_and_cache, render_vault_backup_section,
    render_version_changelog_link,
};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card_with_header, pill_button, BentoLayerAccent, ButtonVariant, IconKind,
};
use crate::design::widgets_controls::{setting_switch_row, tactile_switch_tip};
use crate::desktop::UiAction;
use crate::runtime::bridge::AppStateResponse;
use crate::runtime::bridge_types::VaultBackupDraft;
use crate::runtime::fonts::DiscoveredFont;
use eframe::egui::{self, Align, Color32, Id, Layout, RichText, ScrollArea, Vec2};
use opendrive_core::DocsMode;

#[allow(clippy::too_many_arguments)]
pub fn render_settings_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    state: &AppStateResponse,
    vault: &mut VaultBackupDraft,
    rule_draft: &mut OpenRuleDraft,
    fonts: &[DiscoveredFont],
    font_draft: &mut FontPickerDraft,
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

    let modal_w = (overlay.width() - 48.0).clamp(320.0, 768.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 160.0).clamp(120.0, 480.0);
    egui::Area::new(Id::unique("settings_modal_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            let mut hdr_actions = Vec::new();
            bento_card_with_header(
                ui,
                pal,
                BentoLayerAccent::Dual,
                "OpenDrive Settings",
                Some("Configure startup, virtual drive mapping, cache/bandwidth, fonts, open rules, and backups."),
                |ui| render_version_changelog_link(ui, pal, &mut hdr_actions),
                |ui| {
                    ScrollArea::vertical()
                        .max_height(max_body_h)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            render_settings_body(
                                ui,
                                pal,
                                state,
                                vault,
                                rule_draft,
                                fonts,
                                font_draft,
                                release_checking,
                                actions,
                            );
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
            actions.extend(hdr_actions);
        });
}

#[allow(clippy::too_many_arguments)]
fn render_settings_body(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    state: &AppStateResponse,
    vault: &mut VaultBackupDraft,
    rule_draft: &mut OpenRuleDraft,
    fonts: &[DiscoveredFont],
    font_draft: &mut FontPickerDraft,
    release_checking: bool,
    actions: &mut Vec<UiAction>,
) {
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
    let vd_title = "Show OpenDrive in This PC";
    let vd_sub = if vd_supported {
        format!(
            "Show {} with OpenDrive logo and cloud quota under This PC -> Devices and drives",
            state.mount_root
        )
    } else {
        "Windows only — shows OpenDrive with cloud quota under This PC -> Devices and drives".into()
    };
    if setting_switch_row(
        ui,
        pal,
        settings.virtual_drive_enabled,
        vd_title,
        &vd_sub,
        "",
    ) {
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
    if setting_switch_row(
        ui,
        pal,
        settings.explorer_folder_cache,
        "Cache Explorer Folder Paths",
        "Keep visited folders cached when navigating back and forth until you update or refresh",
        "",
    ) {
        actions.push(UiAction::ToggleExplorerFolderCache);
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        if tactile_switch_tip(
            ui,
            pal,
            settings.check_updates,
            true,
            "Check for GitHub Releases: notify when a newer release is published",
        ) {
            actions.push(UiAction::ToggleCheckUpdates);
        }
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Check for GitHub Releases")
                    .size(14.5)
                    .strong()
                    .color(pal.text_primary),
            );
            ui.add(
                egui::Label::new(
                    RichText::new("Notify when a newer release is published (off by default)")
                        .size(12.5)
                        .color(pal.text_muted),
                )
                .wrap(),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let lbl = if release_checking {
                "Checking..."
            } else {
                "Check Now"
            };
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
    render_bandwidth_and_cache(ui, pal, state, actions);
    ui.add_space(14.0);
    render_font_section(
        ui,
        pal,
        settings.ui_font_family.as_deref(),
        fonts,
        font_draft,
        actions,
    );
    ui.add_space(12.0);
    render_open_rules_section(ui, pal, &settings.open_with_rules, rule_draft, actions);
    ui.add_space(12.0);
    render_vault_backup_section(ui, pal, vault, actions);
    ui.add_space(10.0);
    render_about_footer(ui, pal, actions);
}
