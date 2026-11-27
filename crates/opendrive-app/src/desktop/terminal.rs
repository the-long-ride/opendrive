//! In-app Terminal view with Tab 1 (PowerShell-style live console) & Tab 2 (structured App Logs with OD-* codes).

use crate::design::formatters::StatusTone;
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card_with_header, pill_button, singleline_input, status_pill, BentoLayerAccent,
    ButtonVariant, IconKind,
};
use crate::desktop::bento::paint_bento_dots;
use crate::desktop::modals::support::build_support_prompt;
use crate::desktop::{ControlCenterApp, UiAction};
use crate::runtime::app_logs::{
    app_log_hub, format_app_logs_text, format_terminal_text, AppLogEntry,
};
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
};
use opendrive_core::{lookup_diagnostic, LogSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalTab {
    #[default]
    Console,
    AppLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFilterLevel {
    #[default]
    All,
    WarnPlus,
    ErrorOnly,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalViewState {
    pub active_tab: TerminalTab,
    pub search_query: String,
    pub level_filter: LogFilterLevel,
}

pub fn filter_log_entries<'a>(
    entries: &'a [AppLogEntry],
    state: &TerminalViewState,
) -> Vec<&'a AppLogEntry> {
    let q = state.search_query.trim().to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| match state.level_filter {
            LogFilterLevel::All => true,
            LogFilterLevel::WarnPlus => matches!(e.level, LogSeverity::Warn | LogSeverity::Error),
            LogFilterLevel::ErrorOnly => e.level == LogSeverity::Error,
        })
        .filter(|e| {
            q.is_empty()
                || e.message.to_ascii_lowercase().contains(&q)
                || e.target.to_ascii_lowercase().contains(&q)
                || e.code
                    .as_deref()
                    .is_some_and(|c| c.to_ascii_lowercase().contains(&q))
        })
        .collect()
}

fn render_console_tab(ui: &mut egui::Ui, pal: &ThemePalette, entries: &[&AppLogEntry]) {
    let term_bg = Color32::from_rgb(0x0c, 0x0e, 0x14);
    Frame::new()
        .fill(term_bg)
        .stroke(Stroke::new(1.5, pal.tactile_ink))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let h = ui.available_height().max(200.0);
            ScrollArea::vertical()
                .max_height(h)
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    if entries.is_empty() {
                        ui.label(
                            RichText::new("No console log output captured yet.")
                                .size(12.0)
                                .monospace()
                                .color(Color32::from_rgb(0x94, 0xa3, 0xb8)),
                        );
                        return;
                    }
                    for entry in entries {
                        let color = match entry.level {
                            LogSeverity::Error => Color32::from_rgb(0xf8, 0x71, 0x71),
                            LogSeverity::Warn => Color32::from_rgb(0xfb, 0xbf, 0x24),
                            LogSeverity::Info => Color32::from_rgb(0xd8, 0xe2, 0xf2),
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(&entry.terminal_line)
                                    .size(12.0)
                                    .monospace()
                                    .color(color),
                            )
                            .wrap(),
                        );
                    }
                });
        });
}

fn render_app_logs_tab(ui: &mut egui::Ui, pal: &ThemePalette, entries: &[&AppLogEntry]) {
    let h = ui.available_height().max(200.0);
    ScrollArea::vertical()
        .max_height(h)
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if entries.is_empty() {
                ui.label(
                    RichText::new("No matching app log entries.")
                        .size(13.0)
                        .color(pal.text_muted),
                );
                return;
            }
            for entry in entries {
                let (tone, border) = match entry.level {
                    LogSeverity::Error => (StatusTone::Error, pal.error_border),
                    LogSeverity::Warn => (StatusTone::Warn, pal.warn_border),
                    LogSeverity::Info => (StatusTone::Connected, pal.card_border),
                };
                Frame::new()
                    .fill(pal.card_alt)
                    .stroke(Stroke::new(1.5, border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            status_pill(ui, pal, entry.level.as_str(), tone);
                            if let Some(code) = &entry.code {
                                status_pill(ui, pal, code, tone);
                            }
                            ui.label(
                                RichText::new(&entry.timestamp)
                                    .size(11.5)
                                    .monospace()
                                    .color(pal.text_muted),
                            );
                            ui.label(
                                RichText::new(&entry.target)
                                    .size(11.5)
                                    .monospace()
                                    .color(pal.text_muted),
                            );
                        });
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&entry.message)
                                    .size(12.5)
                                    .color(pal.text_primary),
                            )
                            .wrap(),
                        );
                        if let Some(diag) = entry.code.as_deref().and_then(lookup_diagnostic) {
                            ui.add_space(3.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{}: {}", diag.title, diag.guidance))
                                        .size(11.5)
                                        .color(pal.accent_blue),
                                )
                                .wrap(),
                            );
                        }
                    });
                ui.add_space(6.0);
            }
        });
}

pub fn render_terminal_panel(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    app: &mut ControlCenterApp,
    actions: &mut Vec<UiAction>,
) {
    let all_entries = app_log_hub().snapshot();
    let filtered = filter_log_entries(&all_entries, &app.terminal);
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(pal.canvas_bg)
                .inner_margin(Margin::symmetric(18, 16)),
        )
        .show(ui, |ui| {
            paint_bento_dots(ui.painter(), ui.max_rect(), pal);
            let mut hdr_actions = Vec::new();
            bento_card_with_header(
                ui,
                pal,
                BentoLayerAccent::Cyan,
                "Terminal & Diagnostic Logs",
                Some("Credentials and tokens are masked automatically. Share App Logs or AI prompt for troubleshooting."),
                |ui| {
                    if pill_button(ui, pal, Some(IconKind::ArrowLeft), "Back", ButtonVariant::Default, true, true) {
                        hdr_actions.push(UiAction::ToggleTerminal(Some(false)));
                    }
                    if pill_button(ui, pal, Some(IconKind::Trash), "Clear", ButtonVariant::Danger, true, !all_entries.is_empty()) {
                        hdr_actions.push(UiAction::ClearLogs);
                    }
                    if pill_button(ui, pal, Some(IconKind::Copy), "Copy Logs", ButtonVariant::Primary, true, !filtered.is_empty()) {
                        hdr_actions.push(UiAction::CopyLogs);
                    }
                },
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if pill_button(ui, pal, Some(IconKind::Terminal), "Terminal Log", ButtonVariant::Toggle { selected: app.terminal.active_tab == TerminalTab::Console }, true, true) {
                            actions.push(UiAction::SetTerminalTab(TerminalTab::Console));
                        }
                        if pill_button(ui, pal, Some(IconKind::List), "App Logs", ButtonVariant::Toggle { selected: app.terminal.active_tab == TerminalTab::AppLogs }, true, true) {
                            actions.push(UiAction::SetTerminalTab(TerminalTab::AppLogs));
                        }
                        if app.terminal.active_tab == TerminalTab::AppLogs {
                            for (lvl, lbl) in [
                                (LogFilterLevel::All, "All"),
                                (LogFilterLevel::WarnPlus, "Warn+"),
                                (LogFilterLevel::ErrorOnly, "Errors"),
                            ] {
                                if pill_button(ui, pal, None, lbl, ButtonVariant::Toggle { selected: app.terminal.level_filter == lvl }, true, true) {
                                    app.terminal.level_filter = lvl;
                                }
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_sized(
                                [220.0, 28.0],
                                singleline_input(&mut app.terminal.search_query, "Filter logs or OD-* code..."),
                            );
                        });
                    });
                    ui.add_space(10.0);
                    match app.terminal.active_tab {
                        TerminalTab::Console => render_console_tab(ui, pal, &filtered),
                        TerminalTab::AppLogs => render_app_logs_tab(ui, pal, &filtered),
                    }
                },
            );
            actions.extend(hdr_actions);
        });
}

impl ControlCenterApp {
    pub(crate) fn toggle_terminal(&mut self, force: Option<bool>) {
        self.terminal_open = force.unwrap_or(!self.terminal_open);
    }

    pub(crate) fn toggle_support_modal(&mut self, force: Option<bool>) {
        self.support_open = force.unwrap_or(!self.support_open);
    }

    pub(crate) fn copy_logs(&mut self) {
        let all = app_log_hub().snapshot();
        let filtered: Vec<AppLogEntry> = filter_log_entries(&all, &self.terminal)
            .into_iter()
            .cloned()
            .collect();
        let text = match self.terminal.active_tab {
            TerminalTab::Console => format_terminal_text(&filtered),
            TerminalTab::AppLogs => format_app_logs_text(&filtered),
        };
        self.pending_clipboard = Some(text);
        self.set_ok("Copied masked logs to clipboard");
    }

    pub(crate) fn copy_support_prompt(&mut self) {
        self.pending_clipboard = Some(build_support_prompt(env!("CARGO_PKG_VERSION")));
        self.set_ok("Copied support prompt to clipboard");
    }
}
