//! Centered AI Support Prompt modal matching the version-pinned llm.txt & CHANGELOG prompt template.

use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card_with_header, pill_button, BentoLayerAccent, ButtonVariant, IconKind,
};
use crate::desktop::UiAction;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Stroke, Vec2,
};

pub fn build_support_prompt(version: &str) -> String {
    let ver = version.trim().trim_start_matches(['v', 'V']);
    format!(
        "I use OpenDrive v{ver}. Help me learn how to use this version of the app and answer my follow-up questions with practical, step-by-step instructions.\n\n\
Read these official sources first:\n\
- Exact-version guide (try first): https://github.com/the-long-ride/opendrive/blob/v{ver}/llm.txt\n\
- Product and feature guide (latest fallback): https://github.com/the-long-ride/opendrive/blob/main/llm.txt\n\
- Changelog and version changes: https://github.com/the-long-ride/opendrive/blob/main/CHANGELOG.md\n\
- Repository and README: https://github.com/the-long-ride/opendrive\n\
- Exact release notes: https://github.com/the-long-ride/opendrive/releases/tag/v{ver}\n\
- All releases: https://github.com/the-long-ride/opendrive/releases\n\n\
Use the changelog to distinguish features available in my version from newer changes. Ask which platform I use (Windows, macOS, or Linux) before giving platform-specific steps. If you cannot access a link, tell me and ask me to paste its content. Do not invent features, shortcuts, settings, or error codes. Please start with a brief overview of what I can do, then ask what I want to accomplish."
    )
}

pub fn render_support_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::ToggleSupportModal(Some(false)));
        return;
    }
    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = Rect::from_min_max(Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("support_modal_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::ToggleSupportModal(Some(false)));
            }
        });

    let ver = env!("CARGO_PKG_VERSION");
    let prompt = build_support_prompt(ver);
    let subtitle = format!(
        "Includes your installed version (v{ver}), an exact-release guide link, and a safe fallback for version mismatches."
    );
    let modal_w = (overlay.width() - 64.0).clamp(320.0, 680.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 170.0).clamp(120.0, 320.0);

    egui::Area::new(Id::unique("support_modal_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            let mut hdr_actions = Vec::new();
            bento_card_with_header(
                ui,
                pal,
                BentoLayerAccent::Dual,
                "Example support prompt",
                Some(&subtitle),
                |ui| {
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Copy),
                        "Copy",
                        ButtonVariant::Primary,
                        true,
                        true,
                    ) {
                        hdr_actions.push(UiAction::CopySupportPrompt);
                    }
                },
                |ui| {
                    Frame::new()
                        .fill(Color32::from_rgb(0x0c, 0x0e, 0x14))
                        .stroke(Stroke::new(1.5, pal.card_border))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ScrollArea::vertical()
                                .max_height(max_body_h)
                                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&prompt)
                                                .size(12.5)
                                                .monospace()
                                                .color(Color32::from_rgb(0xe2, 0xe8, 0xf0)),
                                        )
                                        .wrap()
                                        .selectable(true),
                                    );
                                });
                        });
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        if pill_button(
                            ui,
                            pal,
                            Some(IconKind::External),
                            "Open llm.txt",
                            ButtonVariant::TactilePill,
                            true,
                            true,
                        ) {
                            actions.push(UiAction::OpenExternalUrl(
                                "https://github.com/the-long-ride/opendrive/blob/main/llm.txt",
                            ));
                        }
                        if pill_button(
                            ui,
                            pal,
                            Some(IconKind::Terminal),
                            "View App Logs",
                            ButtonVariant::Default,
                            true,
                            true,
                        ) {
                            actions.push(UiAction::ToggleSupportModal(Some(false)));
                            actions.push(UiAction::SetTerminalTab(
                                crate::desktop::TerminalTab::AppLogs,
                            ));
                            actions.push(UiAction::ToggleTerminal(Some(true)));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(
                                ui,
                                pal,
                                Some(IconKind::Close),
                                "Close",
                                ButtonVariant::Default,
                                true,
                                true,
                            ) {
                                actions.push(UiAction::ToggleSupportModal(Some(false)));
                            }
                        });
                    });
                },
            );
            actions.extend(hdr_actions);
        });
}
