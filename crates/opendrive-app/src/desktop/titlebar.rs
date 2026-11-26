//! Minimal frameless draggable native bar with non-selectable title, Theme switch, icon-only Sync toggle & Settings clone, divider, and window chrome.

use crate::design::logo::paint_logo;
use crate::design::theme::{ThemeMode, ThemePalette};
use crate::design::widgets::{pill_button, vertical_divider, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::bridge::AppStateResponse;
use eframe::egui::{
    self, Align, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2, ViewportCommand,
};
use opendrive_core::AccountStatus;

fn render_window_controls(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    is_max: bool,
    actions: &mut Vec<UiAction>,
) {
    if pill_button(
        ui,
        pal,
        Some(IconKind::Close),
        "",
        ButtonVariant::Danger,
        true,
        true,
    ) {
        actions.push(UiAction::WindowRequestClose);
    }
    let max_icon = if is_max {
        IconKind::Restore
    } else {
        IconKind::Maximize
    };
    if pill_button(
        ui,
        pal,
        Some(max_icon),
        "",
        ButtonVariant::Default,
        true,
        true,
    ) {
        actions.push(UiAction::WindowToggleMaximize);
    }
    if pill_button(
        ui,
        pal,
        Some(IconKind::Minimize),
        "",
        ButtonVariant::Default,
        true,
        true,
    ) {
        actions.push(UiAction::WindowMinimize);
    }
}

fn render_theme_switch(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    mode: ThemeMode,
    actions: &mut Vec<UiAction>,
) {
    let (theme_icon, theme_lbl) = match mode {
        ThemeMode::DarkStudio => (IconKind::ThemeColor, "Color"),
        ThemeMode::ColorPop => (IconKind::ThemeMono, "Dark"),
    };
    if pill_button(
        ui,
        pal,
        Some(theme_icon),
        theme_lbl,
        ButtonVariant::TactilePill,
        true,
        true,
    ) {
        actions.push(UiAction::ToggleTheme);
    }
}

fn render_sync_toggle(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    has_accounts: bool,
    all_paused: bool,
    actions: &mut Vec<UiAction>,
) {
    let (icon, variant, action) = if all_paused {
        (
            IconKind::Play,
            ButtonVariant::Toggle { selected: true },
            "resume_all",
        )
    } else {
        (IconKind::Pause, ButtonVariant::Default, "pause_all")
    };
    if pill_button(ui, pal, Some(icon), "", variant, true, has_accounts) {
        actions.push(UiAction::AccountAction { action, id: None });
    }
}

fn render_settings_button(ui: &mut egui::Ui, pal: &ThemePalette, actions: &mut Vec<UiAction>) {
    if pill_button(
        ui,
        pal,
        Some(IconKind::Settings),
        "",
        ButtonVariant::Default,
        true,
        true,
    ) {
        actions.push(UiAction::ToggleSettings(None));
    }
}

pub fn render_custom_titlebar(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    mode: ThemeMode,
    state: &AppStateResponse,
    actions: &mut Vec<UiAction>,
) -> f32 {
    let ctx = ui.ctx().clone();
    let has_accounts = !state.accounts.is_empty();
    let all_paused = has_accounts
        && state
            .accounts
            .iter()
            .all(|a| !a.enabled || a.status == AccountStatus::Paused);
    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

    let panel = egui::Panel::top("custom_titlebar")
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(pal.card_bg)
                .stroke(Stroke::new(1.5, pal.tactile_ink))
                .inner_margin(Margin::symmetric(14, 10)),
        )
        .show(ui, |ui| {
            // Register background drag before child widgets so child pills and buttons keep pointer priority.
            let drag_resp = ui.interact(
                ui.max_rect(),
                ui.scope_id().with("titlebar_drag"),
                Sense::click_and_drag(),
            );
            let compact = ui.available_width() < 460.0;
            ui.horizontal(|ui| {
                let (logo_rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                paint_logo(ui.painter(), logo_rect, pal);
                ui.add(
                    egui::Label::new(
                        RichText::new("OpenDrive")
                            .size(18.0)
                            .strong()
                            .color(pal.text_primary),
                    )
                    .selectable(false),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    render_window_controls(ui, pal, is_max, actions);
                    if !compact {
                        vertical_divider(ui, pal);
                        render_settings_button(ui, pal, actions);
                        render_sync_toggle(ui, pal, has_accounts, all_paused, actions);
                        render_theme_switch(ui, pal, mode, actions);
                    }
                });
            });
            if compact {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    render_theme_switch(ui, pal, mode, actions);
                    render_sync_toggle(ui, pal, has_accounts, all_paused, actions);
                    render_settings_button(ui, pal, actions);
                });
            }
            if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if drag_resp.double_clicked_by(egui::PointerButton::Primary) {
                actions.push(UiAction::WindowToggleMaximize);
            }
        });
    panel.response.rect.bottom()
}
