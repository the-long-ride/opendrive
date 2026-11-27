//! Inline Connect Google Drive Account drawer supporting Browser OAuth & Service Account JSON.

mod sections;

pub(crate) use self::sections::render_oauth_pending;
use self::sections::{render_browser_oauth_body, render_service_account_body};
use crate::design::formatters::{ConnectAuthMode, NewAccountDraft};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{
    bento_card_with_header, pill_button, singleline_input, BentoLayerAccent, ButtonVariant,
    IconKind,
};
use crate::design::widgets_controls::setting_switch_row;
use crate::desktop::UiAction;
use crate::runtime::auth_runner::AuthSession;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{self, Color32, Id, Pos2, Rect, RichText, ScrollArea, Vec2};

pub(super) fn drawer_field(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    label: &str,
    value: &mut String,
    hint: &str,
) {
    ui.label(RichText::new(label).size(12.5).color(pal.text_muted));
    let w = ui.available_width();
    ui.add_sized([w, 32.0], singleline_input(value, hint));
}

pub(super) fn render_optional_fields(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    draft: &mut NewAccountDraft,
) {
    ui.columns(2, |cols| {
        drawer_field(
            &mut cols[0],
            pal,
            "Account Nickname (optional)",
            &mut draft.nickname,
            "Work Drive",
        );
        drawer_field(
            &mut cols[1],
            pal,
            "Mount Folder (optional)",
            &mut draft.folder_name,
            "work-drive",
        );
    });
    ui.add_space(10.0);
    if setting_switch_row(
        ui,
        pal,
        draft.read_only,
        "Mount as Read-Only",
        "Block local file changes from uploading to Google Drive",
        if draft.read_only {
            "Locked"
        } else {
            "Read-Write"
        },
    ) {
        draft.read_only = !draft.read_only;
    }
}

pub(super) fn render_form_error(ui: &mut egui::Ui, pal: &ThemePalette, form_error: Option<&str>) {
    if let Some(err) = form_error {
        ui.add_space(8.0);
        ui.label(
            RichText::new(err)
                .size(12.5)
                .strong()
                .color(pal.accent_coral),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_connect_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    reauth_target_name: Option<&str>,
    draft: &mut NewAccountDraft,
    auth_session: Option<&AuthSession>,
    form_error: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::ToggleDrawer(Some(false)));
        return;
    }
    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = Rect::from_min_max(Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("connect_modal_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::ToggleDrawer(Some(false)));
            }
        });

    let heading = match reauth_target_name {
        Some(name) => format!("Re-authenticate {name}"),
        None => "Connect Google Drive Account".to_string(),
    };
    let modal_w = (overlay.width() - 64.0).clamp(320.0, 680.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 130.0).clamp(160.0, 500.0);
    egui::Area::new(Id::unique("connect_modal_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .default_size(Vec2::new(modal_w, max_body_h + 130.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            let mut hdr_actions = Vec::new();
            bento_card_with_header(
                ui,
                pal,
                BentoLayerAccent::Dual,
                &heading,
                None,
                |ui| {
                    if pill_button(ui, pal, None, "Cancel", ButtonVariant::Default, true, true) {
                        hdr_actions.push(UiAction::ToggleDrawer(Some(false)));
                    }
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::Key),
                        "Import Backup",
                        ButtonVariant::Default,
                        true,
                        true,
                    ) {
                        hdr_actions.push(UiAction::OpenVaultBackup);
                    }
                    if pill_button(
                        ui,
                        pal,
                        Some(IconKind::External),
                        "Setup Guide",
                        ButtonVariant::Default,
                        true,
                        true,
                    ) {
                        hdr_actions.push(UiAction::ToggleGuide(Some(true)));
                    }
                },
                |ui| {
                    ScrollArea::vertical()
                        .max_height(max_body_h)
                        .min_scrolled_height(max_body_h)
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                if pill_button(
                                    ui,
                                    pal,
                                    Some(IconKind::External),
                                    "Browser Sign-In",
                                    ButtonVariant::Toggle {
                                        selected: draft.auth_mode == ConnectAuthMode::BrowserOAuth,
                                    },
                                    true,
                                    true,
                                ) {
                                    actions
                                        .push(UiAction::SetAuthMode(ConnectAuthMode::BrowserOAuth));
                                }
                                if pill_button(
                                    ui,
                                    pal,
                                    Some(IconKind::Key),
                                    "Service Account JSON (No Login)",
                                    ButtonVariant::Toggle {
                                        selected: draft.auth_mode
                                            == ConnectAuthMode::ServiceAccount,
                                    },
                                    true,
                                    true,
                                ) {
                                    actions.push(UiAction::SetAuthMode(
                                        ConnectAuthMode::ServiceAccount,
                                    ));
                                }
                            });
                            ui.add_space(10.0);
                            match draft.auth_mode {
                                ConnectAuthMode::BrowserOAuth => {
                                    render_browser_oauth_body(
                                        ui,
                                        pal,
                                        draft,
                                        auth_session,
                                        form_error,
                                        actions,
                                    );
                                }
                                ConnectAuthMode::ServiceAccount => {
                                    render_service_account_body(
                                        ui,
                                        pal,
                                        draft,
                                        auth_session,
                                        form_error,
                                        actions,
                                    );
                                }
                            }
                        });
                },
            );
            actions.extend(hdr_actions);
        });
}
