//! Centered New Release Available modal with Open Release Page and Skip Version actions.

use crate::design::theme::ThemePalette;
use crate::design::widgets::{bento_card, pill_button, BentoLayerAccent, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::release_check::ReleaseInfo;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Stroke, Vec2,
};

pub fn render_release_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    release: &ReleaseInfo,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::DismissReleaseDialog);
        return;
    }
    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = Rect::from_min_max(Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("release_modal_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::DismissReleaseDialog);
            }
        });

    let modal_w = (overlay.width() - 64.0).clamp(320.0, 560.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 156.0).clamp(100.0, 320.0);
    let subtitle = format!(
        "{} is available (current: v{}).",
        release.tag_name,
        env!("CARGO_PKG_VERSION")
    );
    egui::Area::new(Id::unique("release_modal_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            bento_card(
                ui,
                pal,
                BentoLayerAccent::Dual,
                "New OpenDrive Release",
                Some(&subtitle),
                None,
                |ui| {
                    let notes = if release.body.trim().is_empty() {
                        "Open the GitHub release page to download the latest installer or portable build."
                    } else {
                        release.body.trim()
                    };
                    Frame::new()
                        .fill(pal.card_alt)
                        .stroke(Stroke::new(1.5, pal.card_border))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            if !release.name.trim().is_empty() {
                                ui.label(
                                    RichText::new(&release.name)
                                        .size(13.5)
                                        .strong()
                                        .color(pal.text_primary),
                                );
                                ui.add_space(6.0);
                            }
                            ScrollArea::vertical()
                                .max_height(max_body_h)
                                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(notes).size(12.5).color(pal.text_muted),
                                        )
                                        .wrap(),
                                    );
                                });
                        });
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        if pill_button(
                            ui,
                            pal,
                            Some(IconKind::External),
                            "Open Release Page",
                            ButtonVariant::Primary,
                            true,
                            true,
                        ) {
                            actions.push(UiAction::OpenReleasePage(release.html_url.clone()));
                        }
                        if pill_button(
                            ui,
                            pal,
                            None,
                            "Never Show This Version",
                            ButtonVariant::Default,
                            true,
                            true,
                        ) {
                            actions.push(UiAction::IgnoreReleaseVersion(release.tag_name.clone()));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(
                                ui,
                                pal,
                                Some(IconKind::Close),
                                "Later",
                                ButtonVariant::Default,
                                true,
                                true,
                            ) {
                                actions.push(UiAction::DismissReleaseDialog);
                            }
                        });
                    });
                },
            );
        });
}
