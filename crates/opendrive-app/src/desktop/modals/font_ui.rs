//! UI Font selector, Google Fonts/local file validator + importer, and Reset Font control in Settings.

use crate::design::theme::ThemePalette;
use crate::design::widgets::{pill_button, singleline_input, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::fonts::DiscoveredFont;
use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

#[derive(Debug, Clone, Default)]
pub struct FontPickerDraft {
    pub query: String,
    pub import_input: String,
    pub feedback: Option<String>,
    pub feedback_is_err: bool,
    pub importing: bool,
}

pub fn render_font_section(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    active_font: Option<&str>,
    fonts: &[DiscoveredFont],
    draft: &mut FontPickerDraft,
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
                    RichText::new("UI Font")
                        .size(14.0)
                        .strong()
                        .color(pal.text_primary),
                );
                let active_lbl = active_font.unwrap_or("OpenDrive Default");
                ui.label(
                    RichText::new(format!("· Active: {active_lbl}"))
                        .size(12.0)
                        .color(pal.accent_blue),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Refresh),
                        "Reset Font",
                        ButtonVariant::Default,
                        true,
                        active_font.is_some(),
                    ) {
                        actions.push(UiAction::ResetUiFont);
                    }
                });
            });
            ui.label(
                RichText::new(
                    "Choose an installed font, enter a Google Font name, paste a public https:// CDN/font URL, or pick a local .ttf/.otf file.",
                )
                .size(12.0)
                .color(pal.text_muted),
            );
            ui.add_space(8.0);
            let filter = draft.query.trim().to_ascii_lowercase();
            let current = active_font.unwrap_or("OpenDrive Default");
            egui::ComboBox::from_id_salt("settings_ui_font_combo")
                .selected_text(current)
                .width(280.0)
                .height(260.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    ui.set_min_size(egui::Vec2::new(264.0, 224.0));
                    ui.add_sized(
                        [260.0, 26.0],
                        singleline_input(&mut draft.query, "Search installed fonts..."),
                    );
                    ui.separator();
                    let mut close_popup = false;
                    egui::ScrollArea::vertical()
                        .id_salt("settings_ui_font_list_scroll")
                        .max_height(180.0)
                        .min_scrolled_height(180.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(260.0);
                            if ui
                                .selectable_label(active_font.is_none(), "OpenDrive Default")
                                .clicked()
                            {
                                actions.push(UiAction::ResetUiFont);
                                close_popup = true;
                            }
                            let mut matched = 0usize;
                            for font in fonts.iter().filter(|f| {
                                filter.is_empty()
                                    || f.family.to_ascii_lowercase().contains(&filter)
                            }) {
                                matched += 1;
                                let tag = if font.imported { " [Imported]" } else { "" };
                                let label = format!("{}{tag}", font.family);
                                let selected = active_font
                                    .is_some_and(|a| a.eq_ignore_ascii_case(&font.family));
                                if ui.selectable_label(selected, label).clicked() {
                                    actions.push(UiAction::SelectUiFont(font.family.clone()));
                                    close_popup = true;
                                }
                            }
                            if matched == 0 && !filter.is_empty() {
                                ui.label(
                                    RichText::new("No matching fonts")
                                        .size(12.0)
                                        .color(pal.text_muted),
                                );
                            }
                        });
                    if close_popup {
                        ui.close();
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 186.0).max(120.0);
                let resp = ui.add_sized(
                    [w, 30.0],
                    singleline_input(
                        &mut draft.import_input,
                        "Google Font, public https:// CDN URL, or .ttf/.otf path",
                    ),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !draft.importing {
                    actions.push(UiAction::ImportUiFont);
                }
                let import_lbl = if draft.importing { "Importing..." } else { "Import" };
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::CloudDownload),
                    import_lbl,
                    ButtonVariant::Primary,
                    true,
                    !draft.importing,
                ) {
                    actions.push(UiAction::ImportUiFont);
                }
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Folder),
                    "Browse",
                    ButtonVariant::Default,
                    true,
                    !draft.importing,
                ) {
                    actions.push(UiAction::BrowseUiFont);
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Note: Google Fonts and CDN URLs use your internet connection to download and import the font (public https:// links only, max 20 MiB).",
                )
                .size(11.0)
                .color(pal.text_muted),
            );
            if let Some(msg) = &draft.feedback {
                ui.add_space(6.0);
                let fg = if draft.feedback_is_err {
                    pal.accent_coral
                } else {
                    pal.ok_fg
                };
                ui.label(RichText::new(msg).size(12.0).color(fg));
            }
        });
}
