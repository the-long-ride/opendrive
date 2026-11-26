//! Open With Rules search, collapsible card editor, and saved rule cards for Settings modal.

use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    pill_button, pill_button_tip, singleline_input, ButtonVariant, IconKind,
};
use crate::desktop::UiAction;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use opendrive_core::OpenWithRule;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenRuleDraft {
    pub search_query: String,
    pub editor_open: bool,
    pub editing_index: Option<usize>,
    pub pattern: String,
    pub app_command: String,
    pub error: Option<String>,
}

fn render_rule_editor_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    draft: &mut OpenRuleDraft,
    actions: &mut Vec<UiAction>,
) {
    let title = if draft.editing_index.is_some() {
        "Edit Open With Rule"
    } else {
        "New Open With Rule"
    };
    Frame::new()
        .fill(pal.card_bg)
        .stroke(Stroke::new(1.5, pal.accent_blue))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(28.0);
                ui.label(
                    RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(pal.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Check),
                        "Save",
                        ButtonVariant::Primary,
                        true,
                        true,
                    ) {
                        actions.push(UiAction::SaveOpenWithRule);
                    }
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Close),
                        "Cancel",
                        ButtonVariant::Default,
                        true,
                        true,
                    ) {
                        actions.push(UiAction::CancelOpenWithRuleEdit);
                    }
                });
            });
            ui.add_space(6.0);
            let avail = ui.available_width();
            let ext_w = (avail * 0.30).clamp(120.0, 185.0);
            let browse_w = 88.0;
            let cmd_w = (avail - ext_w - browse_w - 16.0).max(120.0);
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.set_min_width(avail);
                ui.allocate_ui_with_layout(
                    egui::vec2(ext_w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.label(
                            RichText::new("Extension / Group")
                                .size(11.5)
                                .color(pal.text_muted),
                        );
                        ui.add_space(3.0);
                        ui.add_sized(
                            [ext_w, 30.0],
                            singleline_input(&mut draft.pattern, "mp4, mkv or @video"),
                        );
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(cmd_w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.label(
                            RichText::new("Application")
                                .size(11.5)
                                .color(pal.text_muted),
                        );
                        ui.add_space(3.0);
                        ui.add_sized(
                            [cmd_w, 30.0],
                            singleline_input(&mut draft.app_command, "vlc or C:\\app.exe"),
                        );
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(browse_w, 0.0),
                    Layout::top_down(Align::Max),
                    |ui| {
                        ui.add_space(21.0);
                        if pill_button(
                            ui,
                            pal,
                            Some(IconKind::Folder),
                            "Browse",
                            ButtonVariant::Default,
                            true,
                            true,
                        ) {
                            actions.push(UiAction::BrowseOpenWithRuleApp);
                        }
                    },
                );
            });
            if let Some(err) = &draft.error {
                ui.add_space(6.0);
                ui.label(RichText::new(err).size(12.0).color(pal.accent_coral));
            }
        });
}

pub fn render_open_rules_section(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    rules: &[OpenWithRule],
    draft: &mut OpenRuleDraft,
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
                RichText::new("Open With Rules (Custom Format Apps)")
                    .size(14.0)
                    .strong()
                    .color(pal.text_primary),
            );
            ui.label(
                RichText::new(
                    "Match extensions (mp4, mkv), groups (@video, @image, @pdf, @audio, @code, @archive), or *.",
                )
                .size(12.0)
                .color(pal.text_muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let search_w = (ui.available_width() - 88.0).max(140.0);
                ui.add_sized(
                    [search_w, 30.0],
                    singleline_input(&mut draft.search_query, "Search extension or app..."),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill_button_tip(
                        ui,
                        pal,
                        Some(IconKind::Plus),
                        "Add",
                        "Add a new Open With rule",
                        ButtonVariant::Primary,
                        true,
                        true,
                    ) {
                        actions.push(UiAction::StartAddOpenWithRule);
                    }
                });
            });
            if draft.editor_open {
                ui.add_space(8.0);
                render_rule_editor_card(ui, pal, draft, actions);
            }
            let q = draft.search_query.trim().to_ascii_lowercase();
            let filtered: Vec<(usize, &OpenWithRule)> = rules
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    q.is_empty()
                        || r.pattern.to_ascii_lowercase().contains(&q)
                        || r.app_command.to_ascii_lowercase().contains(&q)
                })
                .collect();
            ui.add_space(8.0);
            if filtered.is_empty() {
                let msg = if rules.is_empty() {
                    "No custom rules yet — system default launcher is used."
                } else {
                    "No matching rules found."
                };
                ui.label(RichText::new(msg).size(12.0).color(pal.text_muted));
                return;
            }
            for (pos, (idx, rule)) in filtered.into_iter().enumerate() {
                if pos > 0 {
                    ui.add_space(4.0);
                }
                Frame::new()
                    .fill(pal.card_bg)
                    .stroke(Stroke::new(1.5, pal.card_border))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(
                                RichText::new(&rule.pattern)
                                    .size(12.5)
                                    .monospace()
                                    .strong()
                                    .color(pal.accent_blue),
                            );
                            ui.label(RichText::new("->").size(12.0).color(pal.text_muted));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&rule.app_command)
                                        .size(12.5)
                                        .monospace()
                                        .color(pal.text_primary),
                                )
                                .truncate(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if pill_button_tip(
                                    ui,
                                    pal,
                                    Some(IconKind::Trash),
                                    "Remove",
                                    "Remove this Open With rule",
                                    ButtonVariant::Danger,
                                    true,
                                    true,
                                ) {
                                    actions.push(UiAction::RemoveOpenWithRule(idx));
                                }
                                if pill_button_tip(
                                    ui,
                                    pal,
                                    Some(IconKind::Edit),
                                    "Edit",
                                    "Edit this Open With rule",
                                    ButtonVariant::Default,
                                    true,
                                    true,
                                ) {
                                    actions.push(UiAction::StartEditOpenWithRule(idx));
                                }
                            });
                        });
                    });
            }
        });
}
