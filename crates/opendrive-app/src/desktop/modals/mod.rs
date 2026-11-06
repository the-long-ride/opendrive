//! Centered desktop modals: Settings, Google Cloud Console Setup Guide, and New Release dialog.

pub mod guide;
pub mod release;
pub mod settings;

use crate::design::theme::ThemePalette;
use crate::design::widgets::{pill_button, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};

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
