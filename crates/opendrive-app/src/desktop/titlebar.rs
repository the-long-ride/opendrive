//! Minimal frameless draggable native bar with non-selectable title, Pause/Resume, Theme toggle, and window chrome.

use crate::design::logo::paint_logo;
use crate::design::theme::{ThemeMode, ThemePalette};
use crate::design::widgets::{pill_button, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::bridge::AppStateResponse;
use eframe::egui::{
    self, Align, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2, ViewportCommand,
};

fn render_titlebar_controls(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    mode: ThemeMode,
    has_accounts: bool,
    is_max: bool,
    include_window_btns: bool,
    actions: &mut Vec<UiAction>,
) {
    if include_window_btns {
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

    let (theme_icon, theme_lbl) = match mode {
        ThemeMode::DarkStudio => (IconKind::Sun, "Color"),
        ThemeMode::ColorPop => (IconKind::Moon, "Dark"),
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

    if has_accounts {
        if pill_button(
            ui,
            pal,
            Some(IconKind::Play),
            "Resume",
            ButtonVariant::Default,
            true,
            true,
        ) {
            actions.push(UiAction::AccountAction {
                action: "resume_all",
                id: None,
            });
        }
        if pill_button(
            ui,
            pal,
            Some(IconKind::Pause),
            "Pause",
            ButtonVariant::Default,
            true,
            true,
        ) {
            actions.push(UiAction::AccountAction {
                action: "pause_all",
                id: None,
            });
        }
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
            // Register background drag before child widgets so selectable text and pills keep pointer priority.
            let drag_resp = ui.interact(
                ui.max_rect(),
                ui.scope_id().with("titlebar_drag"),
                Sense::click_and_drag(),
            );
            let compact = ui.available_width() < 560.0;
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
                    if compact {
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
                    } else {
                        render_titlebar_controls(
                            ui,
                            pal,
                            mode,
                            has_accounts,
                            is_max,
                            true,
                            actions,
                        );
                    }
                });
            });
            if compact {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    render_titlebar_controls(ui, pal, mode, has_accounts, is_max, false, actions);
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
