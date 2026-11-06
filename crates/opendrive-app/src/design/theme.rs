//! Image-2 Neo-Pop Bento color palettes (`ColorPop` default + `DarkStudio` toggle) and egui theme.

use crate::design::formatters::StatusTone;
use eframe::egui::{
    self, style::WidgetVisuals, Color32, CornerRadius, CursorIcon, FontId, Shadow, Stroke,
    TextStyle, Theme, Visuals,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    ColorPop,
    #[default]
    DarkStudio,
}

impl ThemeMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::ColorPop => Self::DarkStudio,
            Self::DarkStudio => Self::ColorPop,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub canvas_bg: Color32,
    pub backdrop_dot: Color32,
    pub card_bg: Color32,
    pub card_alt: Color32,
    pub pill_neutral: Color32,
    pub card_border: Color32,
    pub tactile_ink: Color32,
    pub dock_bg: Color32,
    pub dock_fg: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub accent_blue: Color32,
    pub accent_blue_hover: Color32,
    pub accent_blue_tint: Color32,
    pub pop_pink: Color32,
    pub pop_pink_tint: Color32,
    pub pop_cyan: Color32,
    pub pop_cyan_tint: Color32,
    pub accent_coral: Color32,
    pub accent_coral_tint: Color32,
    pub error_border: Color32,
    pub switch_off_bg: Color32,
    pub switch_off_thumb: Color32,
    pub track_bg: Color32,
    pub ok_fg: Color32,
    pub ok_tint: Color32,
    pub ok_border: Color32,
    pub warn_fg: Color32,
    pub warn_tint: Color32,
    pub warn_border: Color32,
    pub pastel_green: Color32,
    pub pastel_orange: Color32,
    pub pastel_red: Color32,
}

pub fn palette(mode: ThemeMode) -> ThemePalette {
    match mode {
        ThemeMode::ColorPop => ThemePalette {
            is_dark: false,
            canvas_bg: Color32::from_rgb(0x64, 0x5d, 0xf6),
            backdrop_dot: Color32::from_rgb(0x88, 0x82, 0xfa),
            card_bg: Color32::from_rgb(0xf8, 0xfa, 0xfc),
            card_alt: Color32::from_rgb(0xee, 0xf2, 0xfa),
            pill_neutral: Color32::from_rgb(0xff, 0xff, 0xff),
            card_border: Color32::from_rgb(0x1c, 0x1e, 0x24),
            tactile_ink: Color32::from_rgb(0x1c, 0x1e, 0x24),
            dock_bg: Color32::from_rgb(0xff, 0xff, 0xff),
            dock_fg: Color32::from_rgb(0x1c, 0x1e, 0x24),
            text_primary: Color32::from_rgb(0x18, 0x1a, 0x20),
            text_muted: Color32::from_rgb(0x4a, 0x51, 0x62),
            accent_blue: Color32::from_rgb(0x4f, 0x46, 0xe5),
            accent_blue_hover: Color32::from_rgb(0x64, 0x5d, 0xf6),
            accent_blue_tint: Color32::from_rgb(0xe8, 0xe7, 0xfe),
            pop_pink: Color32::from_rgb(0xff, 0x3d, 0x7f),
            pop_pink_tint: Color32::from_rgb(0xff, 0xe4, 0xee),
            pop_cyan: Color32::from_rgb(0x00, 0xd4, 0xff),
            pop_cyan_tint: Color32::from_rgb(0xdf, 0xf8, 0xff),
            accent_coral: Color32::from_rgb(0xd9, 0x2d, 0x20),
            accent_coral_tint: Color32::from_rgb(0xfe, 0xe4, 0xe2),
            error_border: Color32::from_rgb(0xb4, 0x23, 0x18),
            switch_off_bg: Color32::from_rgb(0xde, 0xe3, 0xee),
            switch_off_thumb: Color32::from_rgb(0x6e, 0x76, 0x89),
            track_bg: Color32::from_rgb(0xd8, 0xdf, 0xec),
            ok_fg: Color32::from_rgb(0x0f, 0x76, 0x6e),
            ok_tint: Color32::from_rgb(0xdc, 0xfc, 0xf3),
            ok_border: Color32::from_rgb(0x0f, 0x76, 0x6e),
            warn_fg: Color32::from_rgb(0xb4, 0x53, 0x09),
            warn_tint: Color32::from_rgb(0xfe, 0xf3, 0xc7),
            warn_border: Color32::from_rgb(0xb4, 0x53, 0x09),
            pastel_green: Color32::from_rgb(0x6e, 0xe7, 0xb7),
            pastel_orange: Color32::from_rgb(0xfd, 0xba, 0x74),
            pastel_red: Color32::from_rgb(0xfc, 0xa5, 0xa5),
        },
        ThemeMode::DarkStudio => ThemePalette {
            is_dark: true,
            canvas_bg: Color32::from_rgb(17, 18, 23),
            backdrop_dot: Color32::from_rgb(37, 40, 52),
            card_bg: Color32::from_rgb(24, 26, 32),
            card_alt: Color32::from_rgb(30, 32, 41),
            pill_neutral: Color32::from_rgb(35, 38, 49),
            card_border: Color32::from_rgb(46, 50, 65),
            tactile_ink: Color32::from_rgb(9, 10, 15),
            dock_bg: Color32::from_rgb(244, 245, 248),
            dock_fg: Color32::from_rgb(17, 18, 23),
            text_primary: Color32::from_rgb(243, 244, 246),
            text_muted: Color32::from_rgb(156, 163, 175),
            accent_blue: Color32::from_rgb(124, 116, 255),
            accent_blue_hover: Color32::from_rgb(144, 137, 255),
            accent_blue_tint: Color32::from_rgb(35, 33, 70),
            pop_pink: Color32::from_rgb(255, 94, 147),
            pop_pink_tint: Color32::from_rgb(56, 26, 42),
            pop_cyan: Color32::from_rgb(34, 211, 238),
            pop_cyan_tint: Color32::from_rgb(19, 46, 56),
            accent_coral: Color32::from_rgb(248, 113, 113),
            accent_coral_tint: Color32::from_rgb(42, 22, 29),
            error_border: Color32::from_rgb(92, 36, 50),
            switch_off_bg: Color32::from_rgb(45, 49, 58),
            switch_off_thumb: Color32::from_rgb(122, 130, 145),
            track_bg: Color32::from_rgb(17, 18, 23),
            ok_fg: Color32::from_rgb(52, 211, 153),
            ok_tint: Color32::from_rgb(19, 54, 41),
            ok_border: Color32::from_rgb(29, 82, 62),
            warn_fg: Color32::from_rgb(251, 191, 36),
            warn_tint: Color32::from_rgb(51, 39, 20),
            warn_border: Color32::from_rgb(94, 71, 32),
            pastel_green: Color32::from_rgb(134, 239, 172),
            pastel_orange: Color32::from_rgb(253, 186, 116),
            pastel_red: Color32::from_rgb(252, 165, 165),
        },
    }
}

pub fn tone_colors(pal: &ThemePalette, tone: StatusTone) -> (Color32, Color32) {
    let (bg, fg, _) = tone_badge_style(pal, tone);
    (bg, fg)
}

pub fn tone_badge_style(pal: &ThemePalette, tone: StatusTone) -> (Color32, Color32, Color32) {
    match tone {
        StatusTone::Connected => (pal.ok_tint, pal.ok_fg, pal.ok_border),
        StatusTone::Paused | StatusTone::Standby => {
            (pal.pill_neutral, pal.text_muted, pal.card_border)
        }
        StatusTone::Warn => (pal.warn_tint, pal.warn_fg, pal.warn_border),
        StatusTone::Error => (pal.accent_coral_tint, pal.accent_coral, pal.error_border),
    }
}

fn widget_style(
    bg_fill: Color32,
    weak_bg_fill: Color32,
    border: Color32,
    fg: Color32,
    radius: u8,
) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill,
        weak_bg_fill,
        bg_stroke: Stroke::new(1.5, border),
        corner_radius: CornerRadius::same(radius),
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    }
}

pub fn apply_studio_theme(ctx: &egui::Context, mode: ThemeMode) {
    let egui_theme = match mode {
        ThemeMode::ColorPop => Theme::Light,
        ThemeMode::DarkStudio => Theme::Dark,
    };
    ctx.set_theme(egui_theme);
    let pal = palette(mode);

    let mut visuals = if pal.is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.dark_mode = pal.is_dark;
    visuals.override_text_color = Some(pal.text_primary);
    visuals.weak_text_color = Some(pal.text_muted);
    visuals.panel_fill = pal.canvas_bg;
    visuals.window_fill = pal.card_bg;
    visuals.extreme_bg_color = pal.card_alt;
    visuals.text_edit_bg_color = Some(pal.card_alt);
    visuals.faint_bg_color = pal.card_alt;
    visuals.code_bg_color = pal.card_alt;
    visuals.warn_fg_color = pal.warn_fg;
    visuals.error_fg_color = pal.accent_coral;
    visuals.hyperlink_color = pal.accent_blue;
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;
    visuals.window_stroke = Stroke::new(1.5, pal.tactile_ink);
    visuals.selection.bg_fill = pal.accent_blue;
    visuals.selection.stroke = Stroke::new(1.5, Color32::WHITE);
    visuals.text_cursor.stroke = Stroke::new(1.5, Color32::WHITE);
    visuals.interact_cursor = Some(CursorIcon::PointingHand);

    visuals.widgets.noninteractive = widget_style(
        pal.card_bg,
        pal.canvas_bg,
        pal.card_border,
        pal.text_primary,
        8,
    );
    visuals.widgets.inactive = widget_style(
        pal.card_alt,
        pal.card_alt,
        pal.card_border,
        pal.text_primary,
        8,
    );
    visuals.widgets.hovered = widget_style(
        pal.card_alt,
        pal.card_alt,
        pal.accent_blue,
        pal.text_primary,
        8,
    );
    visuals.widgets.active = widget_style(
        pal.accent_blue_tint,
        pal.accent_blue_tint,
        pal.accent_blue,
        pal.accent_blue,
        8,
    );
    visuals.widgets.open = widget_style(
        pal.card_alt,
        pal.card_alt,
        pal.accent_blue,
        pal.text_primary,
        8,
    );

    ctx.set_visuals_of(egui_theme, visuals.clone());
    ctx.set_visuals(visuals);

    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size = egui::vec2(32.0, 32.0);
        style.spacing.scroll.bar_width = 8.0;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.5));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(17.5));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
    });
}
