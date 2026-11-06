//! Neo-Cel Studio UI primitives (tactile pill buttons, badges, bullets, studio cards).

use crate::design::formatters::StatusTone;
pub use crate::design::icons::{paint_icon, IconKind};
use crate::design::theme::{tone_badge_style, ThemePalette};
pub use crate::design::widgets_controls::{
    bento_card, bento_card_with_header, studio_card, BentoLayerAccent,
};
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Primary,
    TactilePill,
    Danger,
    PastelPink,
    PastelYellow,
    Toggle { selected: bool },
}

fn button_colors(
    pal: &ThemePalette,
    variant: ButtonVariant,
    hovered: bool,
    enabled: bool,
) -> (Color32, Color32, Color32, bool) {
    let dark_fg = Color32::from_rgb(0x11, 0x12, 0x17);
    let (mut bg, mut border, mut fg, lip) = match variant {
        ButtonVariant::Primary if hovered => {
            (pal.accent_blue_hover, pal.tactile_ink, Color32::WHITE, true)
        }
        ButtonVariant::Primary => (pal.accent_blue, pal.tactile_ink, Color32::WHITE, true),
        ButtonVariant::TactilePill if hovered => {
            (pal.accent_blue, pal.tactile_ink, Color32::WHITE, true)
        }
        ButtonVariant::TactilePill => (pal.dock_bg, pal.tactile_ink, pal.dock_fg, true),
        ButtonVariant::PastelPink => {
            let bg = if hovered {
                Color32::from_rgb(0xff, 0x9e, 0xc2)
            } else {
                Color32::from_rgb(0xff, 0xb3, 0xce)
            };
            (bg, pal.tactile_ink, dark_fg, true)
        }
        ButtonVariant::PastelYellow => {
            let bg = if hovered {
                Color32::from_rgb(0xfc, 0xd3, 0x4d)
            } else {
                Color32::from_rgb(0xfd, 0xe6, 0x8a)
            };
            (bg, pal.tactile_ink, dark_fg, true)
        }
        ButtonVariant::Danger if hovered => (
            pal.accent_coral_tint,
            pal.accent_coral,
            pal.accent_coral,
            false,
        ),
        ButtonVariant::Danger => (
            pal.accent_coral_tint,
            pal.error_border,
            pal.accent_coral,
            false,
        ),
        ButtonVariant::Toggle { selected: true } => (
            pal.accent_blue_tint,
            pal.accent_blue,
            pal.accent_blue,
            false,
        ),
        ButtonVariant::Default | ButtonVariant::Toggle { .. } if hovered => {
            (pal.card_alt, pal.pop_cyan, pal.text_primary, false)
        }
        ButtonVariant::Default | ButtonVariant::Toggle { .. } => {
            (pal.pill_neutral, pal.card_border, pal.text_primary, false)
        }
    };
    if !enabled {
        bg = bg.gamma_multiply(0.55);
        fg = fg.gamma_multiply(0.55);
        border = border.gamma_multiply(0.55);
    }
    (bg, border, fg, lip && enabled)
}

pub fn pill_button(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: Option<IconKind>,
    label: &str,
    variant: ButtonVariant,
    small: bool,
    enabled: bool,
) -> bool {
    let font = FontId::proportional(if small { 12.0 } else { 13.0 });
    let height = if small { 28.0 } else { 34.0 };
    let hpad = if small { 12.0 } else { 15.0 };
    let icon_sz = if small { 13.0 } else { 14.0 };
    let icon_gap = if icon.is_some() && !label.is_empty() {
        7.0
    } else {
        0.0
    };
    let icon_w = if icon.is_some() { icon_sz } else { 0.0 };

    // Estimate width up-front, then resolve (bg, border, fg) BEFORE laying out the text galley.
    let probe = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), pal.text_primary);
    let width = (hpad * 2.0 + icon_w + icon_gap + probe.size().x).max(height + 2.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    let hovered = enabled && resp.hovered();
    let pressed = enabled && resp.is_pointer_button_down_on();
    let (bg, border, fg, _lip) = button_colors(pal, variant, hovered, enabled);

    let radius = CornerRadius::same(8);
    let p = ui.painter();
    if enabled {
        p.rect_filled(rect.translate(Vec2::new(2.0, 2.0)), radius, pal.tactile_ink);
    }
    let draw_rect = if pressed {
        rect.translate(Vec2::new(2.0, 2.0))
    } else if hovered {
        rect.translate(Vec2::new(1.0, 1.0))
    } else {
        rect
    };
    let stroke_w = 1.5;
    p.rect(
        draw_rect,
        radius,
        bg,
        Stroke::new(stroke_w, border),
        StrokeKind::Inside,
    );

    let mut x = draw_rect.left() + hpad;
    if let Some(kind) = icon {
        let pos = Pos2::new(x, draw_rect.center().y - icon_sz * 0.5);
        paint_icon(p, Rect::from_min_size(pos, Vec2::splat(icon_sz)), kind, fg);
        x += icon_sz + icon_gap;
    }
    if !label.is_empty() {
        let galley = p.layout_no_wrap(label.to_owned(), font, fg);
        p.galley(
            Pos2::new(x, draw_rect.center().y - galley.size().y * 0.5),
            galley,
            fg,
        );
    }
    enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

pub fn status_pill(ui: &mut egui::Ui, pal: &ThemePalette, label: &str, tone: StatusTone) {
    let (bg, fg, border) = tone_badge_style(pal, tone);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(11.5), fg);
    let width = 10.0 + 6.0 + 6.0 + galley.size().x + 12.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(7),
        bg,
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    let dot_pos = Pos2::new(rect.left() + 12.0, rect.center().y);
    if tone == StatusTone::Connected {
        let t = ui.ctx().input(|i| i.time);
        let pulse = ((t * 3.0).sin() * 0.5 + 0.5) as f32;
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        p.circle_filled(
            dot_pos,
            4.5 + pulse * 2.5,
            fg.gamma_multiply(0.28 * (1.0 - pulse * 0.6)),
        );
    }
    let dot_rect = Rect::from_center_size(dot_pos, Vec2::splat(5.5));
    p.rect_filled(dot_rect, CornerRadius::same(2), fg);
    p.galley(
        Pos2::new(rect.left() + 20.0, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
}

pub fn telemetry_badge(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), FontId::monospace(12.0), Color32::WHITE);
    let width = galley.size().x + 20.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(7), pal.accent_blue);
    p.galley(rect.center() - galley.size() * 0.5, galley, Color32::WHITE);
}

pub fn account_avatar(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    texture: Option<&egui::TextureHandle>,
    monogram: &str,
    ring_color: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.0), Sense::hover());
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(9),
        ring_color,
        Stroke::new(2.0, pal.tactile_ink),
        StrokeKind::Inside,
    );
    let inner = rect.shrink(3.0);
    if let Some(tex) = texture {
        let img_rect = inner;
        p.add(egui::Shape::Rect(
            egui::epaint::RectShape::filled(img_rect, CornerRadius::same(7), Color32::WHITE)
                .with_texture(
                    tex.id(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                ),
        ));
        p.rect_stroke(
            img_rect,
            CornerRadius::same(7),
            Stroke::new(1.0, pal.tactile_ink),
            StrokeKind::Inside,
        );
    } else {
        p.rect_filled(inner, CornerRadius::same(7), pal.card_bg);
        let galley = p.layout_no_wrap(
            monogram.to_owned(),
            FontId::proportional(13.5),
            pal.text_primary,
        );
        p.galley(
            inner.center() - galley.size() * 0.5,
            galley,
            pal.text_primary,
        );
    }
}

pub fn quota_progress_bar(ui: &mut egui::Ui, pal: &ThemePalette, ratio: f32, fill: Color32) {
    let w = ui.available_width().max(60.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 10.0), Sense::hover());
    let p = ui.painter();
    let rad = CornerRadius::same(5);
    p.rect(
        rect,
        rad,
        pal.track_bg,
        Stroke::new(1.5, pal.tactile_ink),
        StrokeKind::Inside,
    );
    let clamped = ratio.clamp(0.0, 1.0);
    if clamped > 0.0 {
        let fill_w = (rect.width() * clamped).max(10.0).min(rect.width());
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
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
    let w = ui.available_width().max(40.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 5.0), Sense::hover());
    let p = ui.painter();
    let rad = CornerRadius::same(3);
    p.rect_filled(rect, rad, pal.track_bg);
    let clamped = ratio.clamp(0.0, 1.0);
    if clamped > 0.0 {
        let fill_w = (rect.width() * clamped).max(6.0).min(rect.width());
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
        p.rect_filled(fill_rect, rad, usage_bar_color(pal, clamped));
    }
}

pub fn check_bullet_row(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 9.0, pal.accent_blue);
        paint_icon(
            ui.painter(),
            rect.shrink(3.0),
            IconKind::Check,
            Color32::WHITE,
        );
        ui.label(egui::RichText::new(text).size(13.5).color(pal.text_primary));
    });
}
