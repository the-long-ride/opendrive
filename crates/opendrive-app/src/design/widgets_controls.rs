//! Interactive Image-2 Neo-Cel pill switches, tactile step sliders, and layered Bento cards.

use crate::design::formatters::StatusTone;
use crate::design::theme::ThemePalette;
use crate::design::widgets::status_pill;
use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Pos2, Rect, Sense,
    Shape, Stroke, StrokeKind, Vec2,
};
use eframe::epaint::RectShape;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BentoLayerAccent {
    None,
    Pink,
    Cyan,
    Dual,
}

pub fn studio_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    title: &str,
    subtitle: Option<&str>,
    right_pill: Option<(&str, StatusTone)>,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    bento_card(
        ui,
        pal,
        BentoLayerAccent::None,
        title,
        subtitle,
        right_pill,
        add_body,
    );
}

pub fn bento_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    accent: BentoLayerAccent,
    title: &str,
    subtitle: Option<&str>,
    right_pill: Option<(&str, StatusTone)>,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    bento_card_with_header(
        ui,
        pal,
        accent,
        title,
        subtitle,
        |ui| {
            if let Some((pill_text, tone)) = right_pill {
                status_pill(ui, pal, pill_text, tone);
            }
        },
        add_body,
    );
}

pub fn bento_card_with_header(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    accent: BentoLayerAccent,
    title: &str,
    subtitle: Option<&str>,
    add_header_right: impl FnOnce(&mut egui::Ui),
    add_body: impl FnOnce(&mut egui::Ui),
) {
    let pad = if accent == BentoLayerAccent::None {
        0
    } else {
        4
    };
    let rounding = CornerRadius::same(12);
    let back_slot = ui.painter().add(Shape::Noop);
    let res = Frame::new().outer_margin(Margin::same(pad)).show(ui, |ui| {
        Frame::new()
            .fill(pal.card_bg)
            .stroke(Stroke::new(1.5, pal.tactile_ink))
            .corner_radius(rounding)
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .size(18.0)
                            .strong()
                            .color(pal.text_primary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), add_header_right);
                });
                if let Some(sub) = subtitle {
                    ui.add_space(3.0);
                    ui.label(egui::RichText::new(sub).size(12.5).color(pal.text_muted));
                }
                ui.add_space(10.0);
                add_body(ui);
            });
    });
    if accent != BentoLayerAccent::None {
        let card_rect = res.response.rect.shrink(pad as f32);
        let fill_layer = |offset: Vec2, fill: Color32| -> Shape {
            Shape::Rect(RectShape::new(
                card_rect.translate(offset),
                rounding,
                fill,
                Stroke::NONE,
                StrokeKind::Inside,
            ))
        };
        let shapes = match accent {
            BentoLayerAccent::Pink => vec![fill_layer(Vec2::new(3.5, 3.5), pal.pop_pink)],
            BentoLayerAccent::Cyan => vec![fill_layer(Vec2::new(3.5, 3.5), pal.pop_cyan)],
            BentoLayerAccent::Dual => vec![
                fill_layer(Vec2::new(-2.5, -2.5), pal.pop_pink),
                fill_layer(Vec2::new(2.5, 2.5), pal.pop_cyan),
            ],
            BentoLayerAccent::None => Vec::new(),
        };
        ui.painter().set(back_slot, Shape::Vec(shapes));
    }
}

pub fn tactile_switch(ui: &mut egui::Ui, pal: &ThemePalette, on: bool, enabled: bool) -> bool {
    let size = Vec2::new(46.0, 26.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let mut track_bg = if on {
        pal.accent_blue
    } else {
        pal.switch_off_bg
    };
    let mut border = pal.tactile_ink;
    let mut thumb_fill = if on {
        Color32::WHITE
    } else {
        pal.switch_off_thumb
    };
    if !enabled {
        track_bg = track_bg.gamma_multiply(0.55);
        border = border.gamma_multiply(0.55);
        thumb_fill = thumb_fill.gamma_multiply(0.55);
    }

    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(13),
        track_bg,
        Stroke::new(2.0, border),
        StrokeKind::Inside,
    );
    let thumb_x = if on {
        rect.right() - 13.0
    } else {
        rect.left() + 13.0
    };
    let thumb_c = Pos2::new(thumb_x, rect.center().y);
    p.circle_filled(thumb_c, 8.5, thumb_fill);
    p.circle_stroke(thumb_c, 8.5, Stroke::new(1.5, border));

    enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

pub fn setting_switch_row(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    on: bool,
    title: &str,
    subtitle: &str,
    right_status: &str,
) -> bool {
    let mut toggled = false;
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        if tactile_switch(ui, pal, on, true) {
            toggled = true;
        }
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(14.5)
                    .strong()
                    .color(pal.text_primary),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(subtitle)
                        .size(12.5)
                        .color(pal.text_muted),
                )
                .wrap(),
            );
        });
        if !right_status.is_empty() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let col = if on { pal.accent_blue } else { pal.text_muted };
                ui.label(egui::RichText::new(right_status).size(12.5).color(col));
            });
        }
    });
    toggled
}

pub fn tactile_step_slider(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    current_step: usize,
    steps_count: usize,
    title: &str,
    right_value: &str,
) -> Option<usize> {
    if steps_count <= 1 {
        return None;
    }
    let clamped = current_step.min(steps_count - 1);
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            egui::RichText::new(title)
                .size(13.5)
                .strong()
                .color(pal.text_primary),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                egui::RichText::new(right_value)
                    .size(13.0)
                    .monospace()
                    .strong()
                    .color(pal.text_primary),
            );
        });
    });
    ui.add_space(4.0);
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(60.0), 24.0),
        Sense::click_and_drag(),
    );
    let min_x = rect.left() + 9.5;
    let max_x = (rect.right() - 9.5).max(min_x + 1.0);
    let ratio = (clamped as f32) / ((steps_count - 1) as f32);
    let thumb_x = min_x + (max_x - min_x) * ratio;

    let groove = Rect::from_min_max(
        Pos2::new(rect.left(), rect.center().y - 4.0),
        Pos2::new(rect.right(), rect.center().y + 4.0),
    );
    let p = ui.painter();
    p.rect(
        groove,
        CornerRadius::same(4),
        pal.track_bg,
        Stroke::new(1.5, pal.tactile_ink),
        StrokeKind::Inside,
    );
    let active_w = (thumb_x - groove.left()).clamp(4.0, groove.width());
    let active_rect = Rect::from_min_size(groove.min, Vec2::new(active_w, groove.height()));
    p.rect_filled(active_rect, CornerRadius::same(4), pal.accent_blue);

    let thumb_c = Pos2::new(thumb_x, rect.center().y);
    p.circle_filled(thumb_c, 9.5, Color32::WHITE);
    p.circle_stroke(thumb_c, 9.5, Stroke::new(2.5, pal.tactile_ink));

    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if resp.clicked() || resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let rel = ((pos.x - min_x) / (max_x - min_x)).clamp(0.0, 1.0);
            let next = ((rel * (steps_count - 1) as f32).round() as usize).min(steps_count - 1);
            return (next != clamped).then_some(next);
        }
    }
    None
}
