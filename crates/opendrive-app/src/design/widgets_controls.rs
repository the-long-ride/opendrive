//! Interactive Image-2 Neo-Cel pill switches, tactile step sliders, quota bars, and layered Bento cards.

use crate::design::formatters::StatusTone;
use crate::design::icons::{paint_icon, IconKind};
use crate::design::theme::ThemePalette;
use crate::design::tooltip::attach_smart_tooltip;
use crate::design::widgets::status_pill;
use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Pos2, Rect, RichText,
    Sense, Shape, Stroke, StrokeKind, Vec2,
};
use eframe::epaint::RectShape;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BentoLayerAccent {
    None,
    Pink,
    Cyan,
    Dual,
}

fn rt(text: &str, size: f32, col: Color32) -> RichText {
    RichText::new(text).size(size).color(col)
}

pub fn studio_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    title: &str,
    subtitle: Option<&str>,
    right_pill: Option<(&str, StatusTone)>,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    let acc = BentoLayerAccent::None;
    bento_card(ui, pal, acc, title, subtitle, right_pill, add_body);
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
    let header_right = |ui: &mut egui::Ui| {
        if let Some((pill_text, tone)) = right_pill {
            status_pill(ui, pal, pill_text, tone);
        }
    };
    bento_card_with_header(ui, pal, accent, title, subtitle, header_right, add_body);
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
    let pad = i8::from(accent != BentoLayerAccent::None) * 4;
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
                    ui.label(rt(title, 18.0, pal.text_primary).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), add_header_right);
                });
                if let Some(sub) = subtitle {
                    ui.add_space(3.0);
                    ui.label(rt(sub, 12.5, pal.text_muted));
                }
                ui.add_space(10.0);
                add_body(ui);
            });
    });
    if accent == BentoLayerAccent::None {
        return;
    }
    let card_rect = res.response.rect.shrink(pad as f32);
    let layer = |dx: f32, dy: f32, fill: Color32| -> Shape {
        let r = card_rect.translate(Vec2::new(dx, dy));
        Shape::Rect(RectShape::new(
            r,
            rounding,
            fill,
            Stroke::NONE,
            StrokeKind::Inside,
        ))
    };
    let shapes = match accent {
        BentoLayerAccent::Pink => vec![layer(3.5, 3.5, pal.pop_pink)],
        BentoLayerAccent::Cyan => vec![layer(3.5, 3.5, pal.pop_cyan)],
        BentoLayerAccent::Dual => vec![
            layer(-2.5, -2.5, pal.pop_pink),
            layer(2.5, 2.5, pal.pop_cyan),
        ],
        BentoLayerAccent::None => Vec::new(),
    };
    ui.painter().set(back_slot, Shape::Vec(shapes));
}

pub fn tactile_switch_tip(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    on: bool,
    enabled: bool,
    tooltip: &str,
) -> bool {
    let sense = enabled.then(Sense::click).unwrap_or(Sense::hover());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 26.0), sense);
    let (mut track_bg, mut thumb_fill) = if on {
        (pal.accent_blue, Color32::WHITE)
    } else {
        (pal.switch_off_bg, pal.switch_off_thumb)
    };
    let mut border = pal.tactile_ink;
    if !enabled {
        track_bg = track_bg.gamma_multiply(0.55);
        border = border.gamma_multiply(0.55);
        thumb_fill = thumb_fill.gamma_multiply(0.55);
    }

    let p = ui.painter();
    let rad = CornerRadius::same(13);
    p.rect(
        rect,
        rad,
        track_bg,
        Stroke::new(2.0, border),
        StrokeKind::Inside,
    );
    let thumb_x = rect.left() + if on { 33.0 } else { 13.0 };
    let thumb_c = Pos2::new(thumb_x, rect.center().y);
    p.circle_filled(thumb_c, 8.5, thumb_fill);
    p.circle_stroke(thumb_c, 8.5, Stroke::new(1.5, border));

    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    attach_smart_tooltip(&resp, ui, pal, tooltip);
    enabled && resp.clicked()
}

pub fn tactile_switch(ui: &mut egui::Ui, pal: &ThemePalette, on: bool, enabled: bool) -> bool {
    let tip = if !enabled {
        "Switch disabled"
    } else if on {
        "Enabled (click to turn off)"
    } else {
        "Disabled (click to turn on)"
    };
    tactile_switch_tip(ui, pal, on, enabled, tip)
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
    let tip = if subtitle.is_empty() {
        format!("{title} ({})", if on { "On" } else { "Off" })
    } else {
        format!("{title}: {subtitle} ({})", if on { "On" } else { "Off" })
    };
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        toggled = tactile_switch_tip(ui, pal, on, true, &tip);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(rt(title, 14.5, pal.text_primary).strong());
            ui.add(egui::Label::new(rt(subtitle, 12.5, pal.text_muted)).wrap());
        });
        if !right_status.is_empty() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let col = if on { pal.accent_blue } else { pal.text_muted };
                ui.label(rt(right_status, 12.5, col));
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
        ui.label(rt(title, 13.5, pal.text_primary).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(rt(right_value, 13.0, pal.text_primary).monospace().strong());
        });
    });
    ui.add_space(4.0);
    let size = Vec2::new(ui.available_width().max(60.0), 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let min_x = rect.left() + 9.5;
    let max_x = (rect.right() - 9.5).max(min_x + 1.0);
    let ratio = (clamped as f32) / ((steps_count - 1) as f32);
    let thumb_x = min_x + (max_x - min_x) * ratio;

    let groove = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 8.0));
    let p = ui.painter();
    let rad = CornerRadius::same(4);
    let ink = Stroke::new(1.5, pal.tactile_ink);
    p.rect(groove, rad, pal.track_bg, ink, StrokeKind::Inside);
    let active_w = (thumb_x - groove.left()).clamp(4.0, groove.width());
    let active_rect = Rect::from_min_size(groove.min, Vec2::new(active_w, groove.height()));
    p.rect_filled(active_rect, rad, pal.accent_blue);

    let thumb_c = Pos2::new(thumb_x, rect.center().y);
    p.circle_filled(thumb_c, 9.5, Color32::WHITE);
    p.circle_stroke(thumb_c, 9.5, Stroke::new(2.5, pal.tactile_ink));

    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let active = resp.clicked() || resp.dragged();
    if let Some(pos) = active.then(|| resp.interact_pointer_pos()).flatten() {
        let rel = ((pos.x - min_x) / (max_x - min_x)).clamp(0.0, 1.0);
        let next = ((rel * (steps_count - 1) as f32).round() as usize).min(steps_count - 1);
        return (next != clamped).then_some(next);
    }
    None
}

fn bar_fill_rect(rect: Rect, ratio: f32, min_w: f32) -> Option<(Rect, f32)> {
    let clamped = ratio.clamp(0.0, 1.0);
    if clamped <= 0.0 {
        return None;
    }
    let fill_w = (rect.width() * clamped).max(min_w).min(rect.width());
    Some((
        Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height())),
        clamped,
    ))
}

pub fn quota_progress_bar(ui: &mut egui::Ui, pal: &ThemePalette, ratio: f32, fill: Color32) {
    let size = Vec2::new(ui.available_width().max(60.0), 10.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let rad = CornerRadius::same(5);
    let ink = Stroke::new(1.5, pal.tactile_ink);
    p.rect(rect, rad, pal.track_bg, ink, StrokeKind::Inside);
    if let Some((fill_rect, _)) = bar_fill_rect(rect, ratio, 10.0) {
        p.rect_filled(fill_rect.shrink(1.5), CornerRadius::same(4), fill);
    }
}

pub fn usage_bar_color(pal: &ThemePalette, ratio: f32) -> Color32 {
    if ratio >= 0.90 {
        pal.pastel_red
    } else if ratio >= 0.75 {
        pal.pastel_orange
    } else {
        pal.pastel_green
    }
}

pub fn thin_usage_bar(ui: &mut egui::Ui, pal: &ThemePalette, ratio: f32) {
    let size = Vec2::new(ui.available_width().max(40.0), 5.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let rad = CornerRadius::same(3);
    p.rect_filled(rect, rad, pal.track_bg);
    if let Some((fill_rect, clamped)) = bar_fill_rect(rect, ratio, 6.0) {
        p.rect_filled(fill_rect, rad, usage_bar_color(pal, clamped));
    }
}

pub fn check_bullet_row(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
        let p = ui.painter();
        p.circle_filled(rect.center(), 9.0, pal.accent_blue);
        let icon_rect = rect.shrink(3.0);
        paint_icon(p, icon_rect, IconKind::Check, Color32::WHITE);
        ui.label(rt(text, 13.5, pal.text_primary));
    });
}
