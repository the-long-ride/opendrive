//! Neo-Cel Studio UI primitives (tactile pill buttons, badges, bullets, studio cards).

pub use crate::design::avatars::account_avatar;
pub use crate::design::icons::{paint_icon, IconKind};
use crate::design::theme::ThemePalette;
use crate::design::tooltip::{attach_smart_tooltip, button_tooltip_text};
pub use crate::design::widgets_badges::{icon_status_pill, status_pill, vertical_divider};
pub use crate::design::widgets_controls::{
    bento_card, bento_card_with_header, check_bullet_row, quota_progress_bar, studio_card,
    thin_usage_bar, usage_bar_color, BentoLayerAccent,
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
) -> (Color32, Color32, Color32) {
    let dark_fg = Color32::from_rgb(0x11, 0x12, 0x17);
    let pick = |h, n| if hovered { h } else { n };
    let (mut bg, mut border, mut fg) = match variant {
        ButtonVariant::Primary => (
            pick(pal.accent_blue_hover, pal.accent_blue),
            pal.tactile_ink,
            Color32::WHITE,
        ),
        ButtonVariant::TactilePill => (
            pick(pal.accent_blue, pal.dock_bg),
            pal.tactile_ink,
            pick(Color32::WHITE, pal.dock_fg),
        ),
        ButtonVariant::PastelPink => (
            pick(
                Color32::from_rgb(0xff, 0x9e, 0xc2),
                Color32::from_rgb(0xff, 0xb3, 0xce),
            ),
            pal.tactile_ink,
            dark_fg,
        ),
        ButtonVariant::PastelYellow => (
            pick(
                Color32::from_rgb(0xfc, 0xd3, 0x4d),
                Color32::from_rgb(0xfd, 0xe6, 0x8a),
            ),
            pal.tactile_ink,
            dark_fg,
        ),
        ButtonVariant::Danger => (
            pal.accent_coral_tint,
            pick(pal.accent_coral, pal.error_border),
            pal.accent_coral,
        ),
        ButtonVariant::Toggle { selected: true } => {
            (pal.accent_blue_tint, pal.accent_blue, pal.accent_blue)
        }
        ButtonVariant::Default | ButtonVariant::Toggle { .. } => (
            pick(pal.card_alt, pal.pill_neutral),
            pick(pal.pop_cyan, pal.card_border),
            pal.text_primary,
        ),
    };
    if !enabled {
        bg = bg.gamma_multiply(0.55);
        fg = fg.gamma_multiply(0.55);
        border = border.gamma_multiply(0.55);
    }
    (bg, border, fg)
}

fn pill_metrics(small: bool, has_icon: bool, has_label: bool) -> (FontId, f32, f32, f32, f32) {
    let font = FontId::proportional(if small { 12.0 } else { 13.0 });
    let height = if small { 28.0 } else { 34.0 };
    let hpad = if small { 12.0 } else { 15.0 };
    let icon_sz = if small { 13.0 } else { 14.0 };
    let icon_gap = if has_icon && has_label { 7.0 } else { 0.0 };
    (font, height, hpad, icon_sz, icon_gap)
}

/// Computes the rendered width of a pill button for layout fitting and drawing.
pub fn pill_button_width(
    ui: &egui::Ui,
    pal: &ThemePalette,
    icon: Option<IconKind>,
    label: &str,
    small: bool,
    trailing_chevron: bool,
) -> f32 {
    let (font, height, hpad, icon_sz, icon_gap) =
        pill_metrics(small, icon.is_some(), !label.is_empty());
    let icon_w = if icon.is_some() { icon_sz } else { 0.0 };
    let chev_w = if trailing_chevron { 16.0 } else { 0.0 };
    let probe = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, pal.text_primary);
    (hpad * 2.0 + icon_w + icon_gap + probe.size().x + chev_w).max(height + 2.0)
}

#[allow(clippy::too_many_arguments)]
fn pill_button_sized(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: Option<IconKind>,
    label: &str,
    tip_override: Option<&str>,
    variant: ButtonVariant,
    small: bool,
    min_width: f32,
    trailing_chevron: bool,
    enabled: bool,
) -> (bool, egui::Response) {
    let (font, height, hpad, icon_sz, icon_gap) =
        pill_metrics(small, icon.is_some(), !label.is_empty());
    let width = pill_button_width(ui, pal, icon, label, small, trailing_chevron).max(min_width);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    let (hovered, pressed) = (
        enabled && resp.hovered(),
        enabled && resp.is_pointer_button_down_on(),
    );
    let (bg, border, fg) = button_colors(pal, variant, hovered, enabled);
    let radius = CornerRadius::same(8);
    let p = ui.painter();
    if enabled {
        p.rect_filled(rect.translate(Vec2::new(2.0, 2.0)), radius, pal.tactile_ink);
    }
    let shift = if pressed {
        2.0
    } else if hovered {
        1.0
    } else {
        0.0
    };
    let draw_rect = rect.translate(Vec2::splat(shift));
    p.rect(
        draw_rect,
        radius,
        bg,
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    let mut x = draw_rect.left() + hpad;
    if let Some(kind) = icon {
        let pos = Pos2::new(x, draw_rect.center().y - icon_sz * 0.5);
        paint_icon(p, Rect::from_min_size(pos, Vec2::splat(icon_sz)), kind, fg);
        x += icon_sz + icon_gap;
    }
    if !label.is_empty() {
        let g = p.layout_no_wrap(label.to_owned(), font, fg);
        let gw = g.size().x;
        p.galley(Pos2::new(x, draw_rect.center().y - g.size().y * 0.5), g, fg);
        x += gw + 5.0;
    }
    if trailing_chevron {
        let pos = Pos2::new(x, draw_rect.center().y - 5.5);
        paint_icon(
            p,
            Rect::from_min_size(pos, Vec2::splat(11.0)),
            IconKind::ChevronDown,
            fg,
        );
    }
    let tip = tip_override
        .map(str::to_owned)
        .unwrap_or_else(|| button_tooltip_text(icon, label, variant, enabled));
    let resp = if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    };
    attach_smart_tooltip(&resp, ui, pal, &tip);
    (enabled && resp.clicked(), resp)
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
    pill_button_sized(
        ui, pal, icon, label, None, variant, small, 0.0, false, enabled,
    )
    .0
}

pub fn menu_dropdown_button(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: Option<IconKind>,
    label: &str,
    variant: ButtonVariant,
    enabled: bool,
) -> egui::Response {
    pill_button_sized(
        ui, pal, icon, label, None, variant, true, 0.0, true, enabled,
    )
    .1
}

#[allow(clippy::too_many_arguments)]
pub fn pill_button_tip(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: Option<IconKind>,
    label: &str,
    tip: &str,
    variant: ButtonVariant,
    small: bool,
    enabled: bool,
) -> bool {
    pill_button_sized(
        ui,
        pal,
        icon,
        label,
        Some(tip),
        variant,
        small,
        0.0,
        false,
        enabled,
    )
    .0
}

pub fn singleline_input<'t>(text: &'t mut String, hint: &str) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .align(egui::Align2::LEFT_CENTER)
        .margin(egui::Margin::symmetric(10, 4))
        .hint_text(hint.to_owned())
}

pub fn menu_pill_button(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    icon: Option<IconKind>,
    label: &str,
    variant: ButtonVariant,
    min_width: f32,
    enabled: bool,
) -> bool {
    pill_button_sized(
        ui, pal, icon, label, None, variant, true, min_width, false, enabled,
    )
    .0
}
