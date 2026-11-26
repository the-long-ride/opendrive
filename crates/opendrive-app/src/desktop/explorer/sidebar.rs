//! Left sidebar for Cloud Explorer: Dashboard back button, account switcher with accurate quota, and Connect trigger.

use crate::design::formatters::{derive_monogram, format_bytes, status_tone};
use crate::design::theme::{tone_badge_style, ThemePalette};
use crate::design::widgets::{account_avatar, pill_button, ButtonVariant, IconKind};
use crate::design::widgets_controls::usage_bar_color;
use crate::desktop::{ControlCenterApp, UiAction};
use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, FontId, Id, Layout, Margin, Rect, RichText, ScrollArea,
    Sense, Vec2,
};
use opendrive_core::{default_account_alias, Account};
use uuid::Uuid;

fn render_account_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    acct: &Account,
    idx: usize,
    selected: bool,
    avatar: Option<&egui::TextureHandle>,
    actions: &mut Vec<UiAction>,
) {
    let alias = default_account_alias(&acct.nickname, &acct.email);
    let monogram = derive_monogram(&alias, &acct.email);
    let email = if acct.email.is_empty() {
        "CLI OAuth pending"
    } else {
        &acct.email
    };
    let ring = if idx.is_multiple_of(2) {
        pal.pop_pink
    } else {
        pal.pop_cyan
    };
    let (_, dot_fg, _) = tone_badge_style(pal, status_tone(&acct.status));
    let frame_res = super::item_frame(pal, selected, 9, Margin::symmetric(9, 6)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 2.0);
        ui.spacing_mut().interact_size.y = 14.0;
        ui.horizontal(|ui| {
            account_avatar(ui, pal, avatar, &monogram, ring, 30.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal(|ui| {
                    let (dot_r, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                    ui.painter()
                        .rect_filled(dot_r, CornerRadius::same(2), dot_fg);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&alias)
                                .size(12.5)
                                .strong()
                                .color(pal.text_primary),
                        )
                        .truncate(),
                    );
                });
                ui.add(
                    egui::Label::new(RichText::new(email).size(10.5).color(pal.text_muted))
                        .truncate(),
                );
            });
        });
        if acct.storage_quota > 0 && acct.storage_quota >= acct.storage_used {
            ui.add_space(2.0);
            let ratio = (acct.storage_used as f32 / acct.storage_quota as f32).clamp(0.0, 1.0);
            let qtxt = format!(
                "{} / {}",
                format_bytes(acct.storage_used),
                format_bytes(acct.storage_quota)
            );
            ui.horizontal(|ui| {
                let qw = ui
                    .painter()
                    .layout_no_wrap(qtxt.clone(), FontId::proportional(10.0), pal.text_muted)
                    .size()
                    .x;
                let bar_w = (ui.available_width() - qw - 8.0).max(36.0);
                let (bar_r, _) = ui.allocate_exact_size(Vec2::new(bar_w, 4.0), Sense::hover());
                let rad = CornerRadius::same(2);
                ui.painter().rect_filled(bar_r, rad, pal.track_bg);
                if ratio > 0.0 {
                    let fw = (bar_w * ratio).max(4.0).min(bar_w);
                    ui.painter().rect_filled(
                        Rect::from_min_size(bar_r.min, Vec2::new(fw, 4.0)),
                        rad,
                        usage_bar_color(pal, ratio),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(qtxt).size(10.0).color(pal.text_muted))
                            .truncate(),
                    );
                });
            });
        }
    });

    let resp = ui
        .interact(
            frame_res.response.rect,
            Id::unique(("explorer_sidebar_acct", acct.id)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    if resp.clicked() {
        actions.push(UiAction::ExplorerSelectAccount(acct.id.to_string()));
    }
}

pub fn render_explorer_sidebar(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    app: &ControlCenterApp,
    narrow: bool,
    actions: &mut Vec<UiAction>,
) {
    let active_id: Option<Uuid> = app.explorer.active_account_id;
    ui.set_min_width(ui.available_width());
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        if pill_button(
            ui,
            pal,
            Some(IconKind::ArrowLeft),
            "Dashboard",
            ButtonVariant::TactilePill,
            true,
            true,
        ) {
            actions.push(UiAction::CloseExplorer);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill_button(
                ui,
                pal,
                Some(IconKind::Plus),
                "New Account",
                ButtonVariant::Primary,
                true,
                true,
            ) {
                actions.push(UiAction::CloseExplorer);
                actions.push(UiAction::ToggleDrawer(Some(true)));
            }
        });
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new("Drive Accounts")
            .size(12.5)
            .strong()
            .color(pal.text_muted),
    );
    ui.add_space(4.0);

    if narrow {
        ui.horizontal_wrapped(|ui| {
            for acct in &app.state.accounts {
                let alias = default_account_alias(&acct.nickname, &acct.email);
                let selected = active_id == Some(acct.id);
                if pill_button(
                    ui,
                    pal,
                    Some(IconKind::Folder),
                    &alias,
                    ButtonVariant::Toggle { selected },
                    true,
                    true,
                ) {
                    actions.push(UiAction::ExplorerSelectAccount(acct.id.to_string()));
                }
            }
        });
        return;
    }

    let list_h = ui.available_height().max(60.0);
    ScrollArea::vertical()
        .id_salt(Id::unique("explorer_sidebar_scroll"))
        .max_height(list_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.state.accounts.is_empty() {
                ui.label(
                    RichText::new("No accounts connected yet.")
                        .size(12.0)
                        .color(pal.text_muted),
                );
            } else {
                for (idx, acct) in app.state.accounts.iter().enumerate() {
                    if idx > 0 {
                        ui.add_space(6.0);
                    }
                    render_account_card(
                        ui,
                        pal,
                        acct,
                        idx,
                        active_id == Some(acct.id),
                        app.avatar_textures.get(&acct.id),
                        actions,
                    );
                }
            }
        });
}
