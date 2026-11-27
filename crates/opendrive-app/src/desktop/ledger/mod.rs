//! Neo-Brutalist Connected Accounts card with inline alias editing, avatars, and bottom pastel usage bars.

mod row;

use self::row::render_account_row;
use crate::design::formatters::format_account_storage;
use crate::design::theme::ThemePalette;
use crate::design::tooltip::attach_smart_tooltip;
use crate::design::widgets::{
    bento_card_with_header, check_bullet_row, pill_button, singleline_input, BentoLayerAccent,
    ButtonVariant, IconKind,
};
use crate::desktop::{ControlCenterApp, UiAction};
use crate::runtime::auth_runner::AuthSession;
use crate::runtime::bridge::AppStateResponse;
use crate::runtime::bridge_types::AliasEditDraft;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use opendrive_core::{default_account_alias, Account};
use std::collections::HashMap;
use uuid::Uuid;

pub(super) fn render_path_usage(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    path: &str,
    used: u64,
    quota: u64,
) {
    let quota_lbl = format_account_storage(used, quota);
    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = 16.0;
        let right_w = quota_lbl.as_ref().map_or(0.0, |q| {
            ui.painter()
                .layout_no_wrap(q.clone(), egui::FontId::proportional(12.0), pal.text_muted)
                .size()
                .x
                + 12.0
        });
        let max_path_w = (ui.available_width() - right_w).max(80.0);
        ui.scope(|ui| {
            ui.set_max_width(max_path_w);
            ui.add(
                egui::Label::new(
                    RichText::new(path)
                        .size(12.0)
                        .monospace()
                        .color(pal.text_muted),
                )
                .truncate()
                .selectable(false),
            );
        });
        if let Some(q) = quota_lbl {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(q).size(12.0).color(pal.text_muted))
                        .selectable(false),
                );
            });
        }
    });
}

pub(super) fn render_alias_editor(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    edit: &mut AliasEditDraft,
    actions: &mut Vec<UiAction>,
) {
    let hint = default_account_alias("", &acct.email);
    Frame::new()
        .fill(pal.card_bg)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Alias:").size(12.5).color(pal.text_muted));
                let resp = ui.add_sized([180.0, 28.0], singleline_input(&mut edit.nickname, &hint));
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    actions.push(UiAction::SaveAlias);
                }
                let cb = ui.checkbox(&mut edit.rename_folder, "Also rename local mount folder");
                attach_smart_tooltip(
                    &cb,
                    ui,
                    pal,
                    "Rename the local mount folder on disk to match the updated alias",
                );
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Check),
                    "Save",
                    ButtonVariant::Primary,
                    true,
                    true,
                ) {
                    actions.push(UiAction::SaveAlias);
                }
                if pill_button(ui, pal, None, "Cancel", ButtonVariant::Default, true, true) {
                    actions.push(UiAction::CancelAlias);
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
pub fn render_accounts_ledger(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    state: &AppStateResponse,
    alias_edit: &mut Option<AliasEditDraft>,
    auth_session: Option<&AuthSession>,
    avatars: &HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<UiAction>,
) {
    let count = state.accounts.len();
    let count_meta = format!(
        "{count} {}",
        if count == 1 { "account" } else { "accounts" }
    );
    let mut toggle_drawer_clicked = false;
    bento_card_with_header(
        ui,
        pal,
        BentoLayerAccent::Pink,
        "Connected Accounts",
        None,
        |ui| {
            if pill_button(
                ui,
                pal,
                Some(IconKind::Plus),
                "Connect",
                ButtonVariant::Primary,
                true,
                true,
            ) {
                toggle_drawer_clicked = true;
            }
            ui.add_space(8.0);
            ui.label(RichText::new(count_meta).size(12.5).color(pal.text_muted));
        },
        |ui| {
            if state.accounts.is_empty() {
                check_bullet_row(ui, pal, "Multi-account sync into unified local mount roots");
                ui.add_space(6.0);
                check_bullet_row(
                    ui,
                    pal,
                    &format!("Virtual filesystem rooted at {}", state.mount_root),
                );
                ui.add_space(6.0);
                check_bullet_row(
                    ui,
                    pal,
                    "Sign in via Google browser OAuth (with Copy Link) or Service Account JSON",
                );
                ui.add_space(14.0);
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Plus),
                    "Add First Account",
                    ButtonVariant::TactilePill,
                    false,
                    true,
                ) {
                    actions.push(UiAction::ToggleDrawer(Some(true)));
                }
            } else {
                for (idx, acct) in state.accounts.iter().enumerate() {
                    if idx > 0 {
                        ui.add_space(10.0);
                    }
                    render_account_row(
                        ui,
                        pal,
                        acct,
                        (idx, count),
                        &state.mount_root,
                        avatars.get(&acct.id),
                        alias_edit,
                        auth_session,
                        actions,
                    );
                }
            }
        },
    );
    if toggle_drawer_clicked {
        actions.push(UiAction::ToggleDrawer(None));
    }
}

impl ControlCenterApp {
    pub(crate) fn start_edit_alias(&mut self, id_str: &str) {
        let Ok(id) = Uuid::parse_str(id_str.trim()) else {
            return;
        };
        if let Some(acct) = self.state.accounts.iter().find(|a| a.id == id) {
            self.alias_edit = Some(AliasEditDraft {
                account_id: id,
                nickname: default_account_alias(&acct.nickname, &acct.email),
                rename_folder: false,
            });
        }
    }

    pub(crate) fn save_edit_alias(&mut self) {
        let Some(draft) = self.alias_edit.take() else {
            return;
        };
        let folder = draft.rename_folder.then(|| draft.nickname.clone());
        match self
            .bridge
            .rename_account(&draft.account_id.to_string(), draft.nickname, folder)
        {
            Ok((next, folder_changed)) => {
                self.state = next;
                self.set_ok("Saved account alias");
                if folder_changed {
                    self.invalidate_account_folder_cache(draft.account_id);
                    self.bridge.spawn_mount_sync(self.repaint_ctx.clone());
                }
            }
            Err(err) => self.set_err(err),
        }
    }

    pub fn open_account_folder(&mut self, path: &str) {
        let _ = std::fs::create_dir_all(path);
        if let Err(err) = (self.auth_endpoints.folder_opener)(path) {
            self.set_err(format!("Could not open folder: {err}"));
        }
    }
}
