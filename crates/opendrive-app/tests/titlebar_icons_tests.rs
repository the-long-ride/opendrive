//! Focused tests for the cogwheel Settings icon, ThemeColor/ThemeMono vector icons, and compact titlebar wrapping.

mod common_auth_helpers;
mod support;

use eframe::egui::{
    self, Color32, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2,
};
use opendrive_app::design::icons::{paint_icon, IconKind};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::ControlCenterApp;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::Settings;
use support::{collect, has};

fn walk_icon(shape: &Shape, circles: &mut Vec<(Color32, Color32, f32)>, closed_paths: &mut usize) {
    match shape {
        Shape::Circle(circle) => {
            circles.push((circle.fill, circle.stroke.color, circle.stroke.width));
        }
        Shape::Path(path) if path.closed && path.points.len() == 30 => *closed_paths += 1,
        Shape::Vec(children) => {
            children
                .iter()
                .for_each(|child| walk_icon(child, circles, closed_paths));
        }
        _ => {}
    }
}

fn collect_titlebar_pills(shape: &Shape, y_max: f32, out: &mut Vec<Rect>) {
    match shape {
        Shape::Rect(rs)
            if rs.stroke.width > 0.0
                && (rs.rect.height() - 28.0).abs() < 0.5
                && rs.rect.bottom() < y_max =>
        {
            out.push(rs.rect);
        }
        Shape::Vec(children) => children
            .iter()
            .for_each(|child| collect_titlebar_pills(child, y_max, out)),
        _ => {}
    }
}

fn click_at_size(ctx: &egui::Context, app: &mut ControlCenterApp, size: Vec2, pos: Pos2) {
    for pressed in [true, false] {
        let btn = Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let _ = run_size(ctx, app, size, vec![Event::PointerMoved(pos), btn]);
    }
}

fn run_size(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    size: Vec2,
    events: Vec<Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(raw, |ui| app.render_root(ui));
    output.textures_delta.clear();
    output.shapes
}

#[test]
fn cogwheel_settings_and_theme_icons_preserve_contrast_and_dimming() {
    let ctx = egui::Context::default();
    let icon_rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0));
    let paint = |kind, fg| {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            paint_icon(ui.painter(), icon_rect, kind, fg);
        });
        out.textures_delta.clear();
        let (mut circles, mut closed_paths) = (Vec::new(), 0);
        out.shapes
            .iter()
            .for_each(|cs| walk_icon(&cs.shape, &mut circles, &mut closed_paths));
        (circles, closed_paths)
    };

    let (gear_circles, gear_paths) = paint(IconKind::Settings, Color32::WHITE);
    assert_eq!((gear_paths, gear_circles.len()), (1, 1));

    let dark_pal = palette(ThemeMode::DarkStudio);
    let enabled_fg = dark_pal.dock_fg;
    let disabled_fg = enabled_fg.gamma_multiply(0.55);
    for fg in [enabled_fg, Color32::WHITE, disabled_fg] {
        let (color_circles, _) = paint(IconKind::ThemeColor, fg);
        let fills: Vec<Color32> = color_circles
            .iter()
            .filter_map(|(fill, _, _)| (*fill != Color32::TRANSPARENT).then_some(*fill))
            .collect();
        let rims: Vec<(Color32, f32)> = color_circles
            .iter()
            .filter_map(|(fill, stroke, width)| {
                (*fill == Color32::TRANSPARENT).then_some((*stroke, *width))
            })
            .collect();
        let alpha = fg.a();
        let expected_fills = [
            Color32::from_rgba_unmultiplied(0x5b, 0x52, 0xf6, alpha),
            Color32::from_rgba_unmultiplied(0xff, 0x3d, 0x7f, alpha),
            Color32::from_rgba_unmultiplied(0x00, 0xd4, 0xff, alpha),
        ];
        assert_eq!(fills, expected_fills);
        assert_eq!(rims, vec![(fg, 1.25); 3]);
    }

    let mono_fg = Color32::from_rgb(0x18, 0x1a, 0x20);
    let (mono_circles, _) = paint(IconKind::ThemeMono, mono_fg);
    assert_eq!(mono_circles.len(), 3);
    assert!(mono_circles.iter().all(|(fill, _, _)| *fill == mono_fg));
}

#[test]
fn titlebar_wraps_before_overlap_at_470px_and_toggles_settings_in_compact_mode() {
    let ctx = egui::Context::default();
    let mut app = ControlCenterApp::new(Bridge::new(Settings::default()).0);
    let size = Vec2::new(470.0, 520.0);
    let _ = run_size(&ctx, &mut app, size, Vec::new());
    let shapes = run_size(&ctx, &mut app, size, Vec::new());
    let labels = collect(&shapes);
    assert!(has(&labels, "Open Settings") && has(&labels, "Color"));

    let color_rect = support::text_pos(&shapes, "Color");
    let title_rect = support::text_pos(&shapes, "OpenDrive");
    assert!(color_rect.y > title_rect.y + 18.0);

    let mut pills = Vec::new();
    shapes
        .iter()
        .for_each(|cs| collect_titlebar_pills(&cs.shape, 96.0, &mut pills));
    let mut wrapped: Vec<Rect> = pills
        .into_iter()
        .filter(|rect| rect.top() > title_rect.y + 12.0)
        .collect();
    wrapped.sort_by(|a, b| a.left().total_cmp(&b.left()));
    assert!(wrapped.len() >= 4);
    let wrapped_settings_pos = wrapped[wrapped.len() - 2].center();

    let _ = run_size(
        &ctx,
        &mut app,
        size,
        vec![Event::PointerMoved(wrapped_settings_pos)],
    );
    let hovered = collect(&run_size(
        &ctx,
        &mut app,
        size,
        vec![Event::PointerMoved(wrapped_settings_pos)],
    ));
    assert!(has(
        &hovered,
        "Open application preferences and sync settings"
    ));

    click_at_size(&ctx, &mut app, size, wrapped_settings_pos);
    assert!(app.settings_open);
    click_at_size(&ctx, &mut app, size, wrapped_settings_pos);
    assert!(!app.settings_open);

    let theme_btn = support::text_pos(&shapes, "Color");
    click_at_size(&ctx, &mut app, size, theme_btn);
    assert_eq!(app.theme_mode, ThemeMode::ColorPop);
    let dark_btn = support::text_pos(&run_size(&ctx, &mut app, size, Vec::new()), "Dark");
    click_at_size(&ctx, &mut app, size, dark_btn);
    assert_eq!(app.theme_mode, ThemeMode::DarkStudio);
}
