//! Inline Connect Google Drive Account drawer supporting Browser OAuth & Service Account JSON.

mod sections;

pub(crate) use self::sections::render_oauth_pending;
use self::sections::{render_browser_oauth_body, render_service_account_body};
use crate::design::formatters::{ConnectAuthMode, NewAccountDraft};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{pill_button, ButtonVariant, IconKind};
use crate::design::widgets_controls::setting_switch_row;
use crate::desktop::UiAction;
use crate::runtime::auth_runner::AuthSession;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke, TextEdit};

pub(super) fn drawer_field(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    label: &str,
    value: &mut String,
    hint: &str,
) {
    ui.label(RichText::new(label).size(12.5).color(pal.text_muted));
    let w = ui.available_width();
    ui.add_sized([w, 32.0], TextEdit::singleline(value).hint_text(hint));
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

pub fn render_connect_drawer(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    reauth_target_name: Option<&str>,
    draft: &mut NewAccountDraft,
    auth_session: Option<&AuthSession>,
    form_error: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    let heading = match reauth_target_name {
        Some(name) => format!("Re-authenticate {name}"),
        None => "Connect Google Drive Account".to_string(),
    };
    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.0, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(heading)
                        .size(15.0)
                        .strong()
                        .color(pal.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill_button(ui, pal, None, "Cancel", ButtonVariant::Default, true, true) {
                        actions.push(UiAction::ToggleDrawer(Some(false)));
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
                        actions.push(UiAction::OpenVaultBackup);
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
                        actions.push(UiAction::ToggleGuide(Some(true)));
                    }
                });
            });
            ui.add_space(10.0);
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
                    actions.push(UiAction::SetAuthMode(ConnectAuthMode::BrowserOAuth));
                }
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Key),
                    "Service Account JSON (No Login)",
                    ButtonVariant::Toggle {
                        selected: draft.auth_mode == ConnectAuthMode::ServiceAccount,
                    },
                    true,
                    true,
                ) {
                    actions.push(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));
                }
            });
            ui.add_space(10.0);
            match draft.auth_mode {
                ConnectAuthMode::BrowserOAuth => {
                    render_browser_oauth_body(ui, pal, draft, auth_session, form_error, actions);
                }
                ConnectAuthMode::ServiceAccount => {
                    render_service_account_body(ui, pal, draft, auth_session, form_error, actions);
                }
            }
        });
    ui.add_space(12.0);
}
