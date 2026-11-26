use super::{drawer_field, render_form_error, render_optional_fields};
use crate::design::formatters::{ConnectAuthMode, NewAccountDraft};
use crate::design::theme::ThemePalette;
use crate::design::widgets::{pill_button, singleline_input, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use crate::runtime::auth_runner::AuthSession;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke, TextEdit};

fn truncate_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let prefix: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{prefix}...")
}

pub(crate) fn render_oauth_pending(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    session: &AuthSession,
    actions: &mut Vec<UiAction>,
) {
    let preview = session
        .auth_url
        .as_deref()
        .map(|u| truncate_preview(u, 76))
        .unwrap_or_else(|| "Preparing loopback callback...".to_string());
    let has_url = session.auth_url.is_some();

    Frame::new()
        .fill(pal.accent_blue_tint)
        .stroke(Stroke::new(1.0, pal.accent_blue))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("Waiting for Google sign-in in browser...")
                    .size(13.0)
                    .strong()
                    .color(pal.text_primary),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(preview)
                    .size(11.5)
                    .monospace()
                    .color(pal.text_muted),
            );
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Copy),
                    "Copy Link",
                    ButtonVariant::TactilePill,
                    true,
                    has_url,
                ) {
                    actions.push(UiAction::CopyAuthLink);
                }
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::External),
                    "Open Browser",
                    ButtonVariant::Default,
                    true,
                    has_url,
                ) {
                    actions.push(UiAction::OpenAuthBrowser);
                }
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Close),
                    "Cancel Sign-In",
                    ButtonVariant::Danger,
                    true,
                    true,
                ) {
                    actions.push(UiAction::CancelAuth);
                }
            });
        });
}

pub(super) fn render_browser_oauth_body(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    draft: &mut NewAccountDraft,
    auth_session: Option<&AuthSession>,
    form_error: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    drawer_field(
        ui,
        pal,
        "OAuth Client ID",
        &mut draft.client_id,
        "1234567890-abc.apps.googleusercontent.com",
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new("OAuth Client Secret")
            .size(12.5)
            .color(pal.text_muted),
    );
    ui.horizontal(|ui| {
        let field_w = (ui.available_width() - 78.0).max(100.0);
        ui.add_sized(
            [field_w, 32.0],
            singleline_input(&mut draft.client_secret, "GOCSPX-...").password(!draft.show_secret),
        );
        let toggle_lbl = if draft.show_secret { "Hide" } else { "Show" };
        if pill_button(
            ui,
            pal,
            None,
            toggle_lbl,
            ButtonVariant::Default,
            true,
            true,
        ) {
            actions.push(UiAction::ToggleSecretVisibility);
        }
    });
    ui.add_space(8.0);
    render_optional_fields(ui, pal, draft);
    render_form_error(ui, pal, form_error);
    ui.add_space(12.0);

    if let Some(session) = auth_session.filter(|s| s.mode == ConnectAuthMode::BrowserOAuth) {
        render_oauth_pending(ui, pal, session, actions);
    } else {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill_button(
                ui,
                pal,
                Some(IconKind::External),
                "Sign in with Google",
                ButtonVariant::Primary,
                false,
                true,
            ) {
                actions.push(UiAction::StartBrowserOAuth {
                    open_browser: true,
                    copy_link: false,
                });
            }
            if pill_button(
                ui,
                pal,
                Some(IconKind::Copy),
                "Copy Link",
                ButtonVariant::TactilePill,
                false,
                true,
            ) {
                actions.push(UiAction::StartBrowserOAuth {
                    open_browser: false,
                    copy_link: true,
                });
            }
        });
    }
}

pub(super) fn render_service_account_body(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    draft: &mut NewAccountDraft,
    auth_session: Option<&AuthSession>,
    form_error: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    ui.label(
        RichText::new(
            "Share target Google Drive folders with the service account client_email. Zero browser login required (supports file path, pasted JSON, or drag-and-drop).",
        )
        .size(12.0)
        .color(pal.text_muted),
    );
    ui.add_space(8.0);
    ui.label(
        RichText::new("Service Account JSON Path or Raw JSON")
            .size(12.5)
            .color(pal.text_muted),
    );
    let w = ui.available_width();
    ui.add_sized(
        [w, 66.0],
        TextEdit::multiline(&mut draft.service_account_input)
            .hint_text("Paste JSON or drop/type C:\\keys\\service-account.json")
            .desired_rows(3),
    );
    ui.add_space(8.0);
    render_optional_fields(ui, pal, draft);
    render_form_error(ui, pal, form_error);
    ui.add_space(12.0);

    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if auth_session.is_some() {
            let _ = pill_button(
                ui,
                pal,
                Some(IconKind::Key),
                "Connecting...",
                ButtonVariant::Primary,
                false,
                false,
            );
            if pill_button(
                ui,
                pal,
                Some(IconKind::Close),
                "Cancel",
                ButtonVariant::Danger,
                false,
                true,
            ) {
                actions.push(UiAction::CancelAuth);
            }
        } else if pill_button(
            ui,
            pal,
            Some(IconKind::Key),
            "Connect Service Account",
            ButtonVariant::Primary,
            false,
            true,
        ) {
            actions.push(UiAction::StartServiceAccount);
        }
    });
}
