//! Status and telemetry vector icon badges for Neo-Cel cards, Explorer rows, and headers.

use crate::design::formatters::StatusTone;
use crate::design::icons::{paint_icon, IconKind};
use crate::design::theme::{tone_badge_style, ThemePalette};
use crate::design::tooltip::{attach_smart_tooltip, status_badge_spec};
use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, Vec2};

fn paint_icon_badge(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: IconKind,
    tooltip: &str,
    (bg, fg, border): (Color32, Color32, Color32),
    pulse: bool,
) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(28.0, 24.0), Sense::hover());
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(7),
        bg,
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    if pulse {
        let t = ui.ctx().input(|i| i.time);
        let phase = ((t * 3.0).sin() * 0.5 + 0.5) as f32;
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        let alpha = 0.22 * (1.0 - phase * 0.6);
        p.circle_filled(rect.center(), 5.5 + phase * 3.0, fg.gamma_multiply(alpha));
    }
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(14.5));
    paint_icon(p, icon_rect, icon, fg);
    attach_smart_tooltip(&resp, ui, pal, tooltip);
}

pub fn status_pill(ui: &mut egui::Ui, pal: &ThemePalette, label: &str, tone: StatusTone) {
    let (icon, tip) = status_badge_spec(label, tone);
    let cols = tone_badge_style(pal, tone);
    paint_icon_badge(ui, pal, icon, &tip, cols, tone == StatusTone::Connected);
}

pub fn vertical_divider(ui: &mut egui::Ui, pal: &ThemePalette) {
    let (slot, _) = ui.allocate_exact_size(Vec2::new(1.5, 28.0), Sense::hover());
    let sep = Rect::from_center_size(slot.center(), Vec2::new(1.5, 18.0));
    ui.painter().rect_filled(sep, 0.0, pal.card_border);
}

pub fn icon_status_pill(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: IconKind,
    tooltip: &str,
    colors: (Color32, Color32, Color32),
) {
    paint_icon_badge(ui, pal, icon, tooltip, colors, false);
}
