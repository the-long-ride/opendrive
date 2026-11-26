//! Step-by-step Google Cloud Console setup guide modal for OAuth 2.0 and Service Accounts.

use super::guide_content::{
    Seg, StepItem, OAUTH_STEPS, SERVICE_ACCOUNT_STEPS, TROUBLESHOOTING_TIPS,
};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{bento_card, pill_button, BentoLayerAccent, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Stroke, Vec2,
};

fn build_rich_line(pal: &ThemePalette, prefix: &str, segs: &[Seg]) -> LayoutJob {
    let mut job = LayoutJob::default();
    let normal = TextFormat {
        font_id: FontId::proportional(12.0),
        color: pal.text_muted,
        ..Default::default()
    };
    let hi = TextFormat {
        font_id: FontId::monospace(11.5),
        color: pal.text_primary,
        background: pal.accent_blue_tint,
        ..Default::default()
    };
    job.append(prefix, 0.0, normal.clone());
    for &(text, highlight) in segs {
        job.append(
            text,
            0.0,
            if highlight {
                hi.clone()
            } else {
                normal.clone()
            },
        );
    }
    job
}

fn render_guide_section(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    title: &str,
    badge: &str,
    steps: &[StepItem],
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
                    RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(pal.text_primary),
                );
                if !badge.is_empty() {
                    ui.label(
                        RichText::new(format!("· {badge}"))
                            .size(12.0)
                            .color(pal.accent_blue),
                    );
                }
            });
            ui.add_space(8.0);
            for (idx, (heading, bullets)) in steps.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{}.", idx + 1))
                            .size(12.5)
                            .strong()
                            .color(pal.accent_blue),
                    );
                    ui.label(
                        RichText::new(*heading)
                            .size(12.5)
                            .strong()
                            .color(pal.text_primary),
                    );
                });
                for bullet in *bullets {
                    ui.add(egui::Label::new(build_rich_line(pal, "   • ", bullet)).wrap());
                }
                ui.add_space(6.0);
            }
        });
}

pub fn render_cloud_guide_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::ToggleGuide(Some(false)));
        return;
    }
    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = Rect::from_min_max(Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("cloud_guide_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::ToggleGuide(Some(false)));
            }
        });

    let modal_w = (overlay.width() - 64.0).clamp(320.0, 680.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 156.0).clamp(120.0, 480.0);
    egui::Area::new(Id::unique("cloud_guide_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            bento_card(
                ui,
                pal,
                BentoLayerAccent::Dual,
                "Google Cloud Console Setup Guide",
                Some("Highlighted badges mark values, menus, and buttons to locate in Google Cloud Console."),
                None,
                |ui| {
                    ScrollArea::vertical()
                        .max_height(max_body_h)
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            render_guide_section(ui, pal, "Option A — Browser Sign-In (OAuth 2.0 Desktop App)", "Recommended for personal & Google Workspace Drive", OAUTH_STEPS);
                            ui.add_space(10.0);
                            render_guide_section(ui, pal, "Option B — Service Account JSON (No-Login Mode)", "Shared Drives & headless automation", SERVICE_ACCOUNT_STEPS);
                            ui.add_space(10.0);
                            render_guide_section(ui, pal, "Quick Troubleshooting", "", TROUBLESHOOTING_TIPS);
                        });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if pill_button(ui, pal, Some(IconKind::External), "Open Google Cloud Console", ButtonVariant::Primary, true, true) {
                            actions.push(UiAction::OpenCloudConsole);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(ui, pal, Some(IconKind::Close), "Close", ButtonVariant::Default, true, true) {
                                actions.push(UiAction::ToggleGuide(Some(false)));
                            }
                        });
                    });
                },
            );
        });
}
