//! Image-2 inspired Bento Grid hero row and studio dot backdrop painter.

use crate::design::formatters::{
    format_bytes, format_platform_label, summarize_sync_health, summarize_total_storage, StatusTone,
};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card, bento_card_with_header, pill_button, quota_progress_bar, status_pill,
    BentoLayerAccent, ButtonVariant, IconKind,
};
use crate::desktop::UiAction;
use crate::runtime::bridge::AppStateResponse;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Pos2, Rect, RichText, Stroke};
use opendrive_core::DocsMode;

pub fn paint_bento_dots(painter: &egui::Painter, rect: Rect, pal: &ThemePalette) {
    let step = 24.0;
    let mut y = rect.top() + 12.0;
    while y < rect.bottom() {
        let mut x = rect.left() + 12.0;
        while x < rect.right() {
            painter.circle_filled(Pos2::new(x, y), 1.5, pal.backdrop_dot);
            x += step;
        }
        y += step;
    }
}

fn pulse_tile(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    label: &str,
    value: String,
    value_color: Color32,
) {
    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.5, pal.tactile_ink))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(label).size(11.5).color(pal.text_muted));
            ui.add_space(3.0);
            ui.label(
                RichText::new(value)
                    .size(19.0)
                    .monospace()
                    .strong()
                    .color(value_color),
            );
        });
}

fn render_storage_tile(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    state: &AppStateResponse,
    actions: &mut Vec<UiAction>,
) {
    let total = summarize_total_storage(&state.accounts);
    let drive_chip = if state.settings.virtual_drive_enabled
        && opendrive_platform::is_virtual_drive_supported()
    {
        format!("{}:\\ Active", state.settings.virtual_drive_letter)
    } else {
        format_platform_label(&state.platform).to_string()
    };
    bento_card(
        ui,
        pal,
        BentoLayerAccent::Dual,
        "Storage & Workspace",
        Some(&state.mount_root),
        Some((&drive_chip, StatusTone::Connected)),
        |ui| {
            ui.label(
                RichText::new(&total.detail_text)
                    .size(14.0)
                    .strong()
                    .color(pal.text_primary),
            );
            ui.add_space(6.0);
            quota_progress_bar(
                ui,
                pal,
                total.ratio,
                crate::design::widgets::usage_bar_color(pal, total.ratio),
            );
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Folder),
                    "Open Folder",
                    ButtonVariant::TactilePill,
                    true,
                    true,
                ) {
                    actions.push(UiAction::OpenMountRoot);
                }
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Settings),
                    "Open Settings",
                    ButtonVariant::Default,
                    true,
                    true,
                ) {
                    actions.push(UiAction::ToggleSettings(Some(true)));
                }
            });
        },
    );
}

fn render_pulse_tile(ui: &mut egui::Ui, pal: &ThemePalette, state: &AppStateResponse) {
    let stats = &state.stats;
    let summary = summarize_sync_health(&state.accounts);
    let docs_lbl = match state.settings.docs_mode {
        DocsMode::Link => "Links (.gdoc)",
        DocsMode::ExportOffice => "Office Export",
    };
    let cache_lbl = format!(
        "Cache: {} / {}",
        format_bytes(stats.cache_bytes),
        format_bytes(state.settings.cache_max_size),
    );
    bento_card_with_header(
        ui,
        pal,
        BentoLayerAccent::Cyan,
        "Sync Pulse",
        Some(&cache_lbl),
        |ui| {
            status_pill(ui, pal, docs_lbl, StatusTone::Standby);
            ui.add_space(4.0);
            status_pill(ui, pal, summary.label, summary.tone);
        },
        |ui| {
            ui.columns(3, |cols| {
                pulse_tile(
                    &mut cols[0],
                    pal,
                    "Transfers",
                    stats.transfers.to_string(),
                    pal.text_primary,
                );
                pulse_tile(
                    &mut cols[1],
                    pal,
                    "Queue",
                    stats.queue_depth.to_string(),
                    pal.text_primary,
                );
                let err_fg = if stats.errors > 0 {
                    pal.accent_coral
                } else {
                    pal.text_primary
                };
                pulse_tile(
                    &mut cols[2],
                    pal,
                    "Errors",
                    stats.errors.to_string(),
                    err_fg,
                );
            });
        },
    );
}

pub fn render_bento_overview(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    state: &AppStateResponse,
    actions: &mut Vec<UiAction>,
) {
    let wide = ui.available_width() >= 700.0;
    if wide {
        ui.columns(2, |cols| {
            render_storage_tile(&mut cols[0], pal, state, actions);
            render_pulse_tile(&mut cols[1], pal, state);
        });
    } else {
        render_storage_tile(ui, pal, state, actions);
        ui.add_space(12.0);
        render_pulse_tile(ui, pal, state);
    }
}
