//! Neo-Cel status/error notice banner and main Bento workspace container.

use crate::design::theme::ThemePalette;
use crate::design::widgets::{paint_icon, pill_button, ButtonVariant, IconKind};
use crate::desktop::bento::{paint_bento_dots, render_bento_overview};
use crate::desktop::ledger::render_accounts_ledger;
use crate::desktop::modals::guide::render_cloud_guide_modal;
use crate::desktop::modals::release::render_release_modal;
use crate::desktop::modals::settings::render_settings_modal;
use crate::desktop::{ControlCenterApp, NoticeTone, UiAction, UiNotice};
use eframe::egui::{
    self, Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Vec2,
};

pub(super) fn render_overlays(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_y: f32,
    app: &mut ControlCenterApp,
    actions: &mut Vec<UiAction>,
) {
    if app.settings_open {
        render_settings_modal(
            ctx,
            pal,
            top_y,
            &app.state,
            &mut app.vault_draft,
            &mut app.open_rule_draft,
            app.release_checking,
            actions,
        );
    }
    if app.guide_open {
        render_cloud_guide_modal(ctx, pal, top_y, actions);
    }
    if let Some(release) = app.available_release.as_ref() {
        render_release_modal(ctx, pal, top_y, release, actions);
    }
    if let Some(modal) = app.explorer.modal.as_mut() {
        super::explorer::render_explorer_modal(ctx, pal, top_y, modal, actions);
    }
    render_notice_toast(ctx, pal, app.notice.as_ref(), actions);
}

pub fn render_workspace_panel(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    app: &mut ControlCenterApp,
    actions: &mut Vec<UiAction>,
) {
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(pal.canvas_bg)
                .inner_margin(Margin::symmetric(18, 16)),
        )
        .show(ui, |ui| {
            paint_bento_dots(ui.painter(), ui.max_rect(), pal);
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let avail_w = ui.available_width();
                    let content_w = avail_w.min(940.0);
                    let side_pad = ((avail_w - content_w) * 0.5).max(0.0);
                    ui.horizontal(|ui| {
                        if side_pad > 0.0 {
                            ui.add_space(side_pad);
                        }
                        ui.allocate_ui_with_layout(
                            Vec2::new(content_w, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_min_width(content_w);
                                ui.set_max_width(content_w);
                                render_bento_overview(ui, pal, &app.state, actions);
                                ui.add_space(16.0);
                                let reauth_name = app.reauth_account_id.and_then(|id| {
                                    app.state
                                        .accounts
                                        .iter()
                                        .find(|a| a.id == id)
                                        .map(|a| a.nickname.clone())
                                });
                                render_accounts_ledger(
                                    ui,
                                    pal,
                                    &app.state,
                                    app.drawer_open,
                                    reauth_name.as_deref(),
                                    &mut app.draft,
                                    &mut app.alias_edit,
                                    app.auth_session.as_ref(),
                                    app.form_error.as_deref(),
                                    &app.avatar_textures,
                                    actions,
                                );
                            },
                        );
                    });
                });
        });
}

pub const TOAST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub fn tick_notice_timeout(
    notice: &mut Option<UiNotice>,
    expires_at: &mut Option<std::time::Instant>,
    ctx: &egui::Context,
) {
    if notice.is_none() {
        *expires_at = None;
        return;
    }
    let now = std::time::Instant::now();
    let deadline = *expires_at.get_or_insert_with(|| now + TOAST_TIMEOUT);
    if now >= deadline {
        *notice = None;
        *expires_at = None;
    } else {
        ctx.request_repaint_after(deadline.saturating_duration_since(now));
    }
}

pub fn render_notice_toast(
    ctx: &egui::Context,
    pal: &ThemePalette,
    notice: Option<&UiNotice>,
    actions: &mut Vec<UiAction>,
) {
    let Some(notice) = notice else {
        return;
    };
    let (bg, border, fg, icon) = match notice.tone {
        NoticeTone::Ok => (pal.ok_tint, pal.ok_fg, pal.ok_fg, IconKind::Check),
        NoticeTone::Error => (
            pal.accent_coral_tint,
            pal.accent_coral,
            pal.accent_coral,
            IconKind::Alert,
        ),
    };
    egui::Area::new(egui::Id::unique("toast_notice"))
        .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -20.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            let rad = CornerRadius::same(8);
            let shadow_slot = ui.painter().add(egui::Shape::Noop);
            let res = Frame::new()
                .outer_margin(Margin {
                    left: 0,
                    right: 4,
                    top: 0,
                    bottom: 4,
                })
                .fill(bg)
                .stroke(Stroke::new(2.0, border))
                .corner_radius(rad)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                        paint_icon(ui.painter(), r, icon, fg);
                        ui.add(
                            egui::Label::new(RichText::new(&notice.message).size(13.0).color(fg))
                                .wrap()
                                .selectable(false),
                        );
                        if notice.tone == NoticeTone::Error
                            && pill_button(
                                ui,
                                pal,
                                Some(IconKind::Refresh),
                                "Retry",
                                ButtonVariant::Default,
                                true,
                                true,
                            )
                        {
                            actions.push(UiAction::Refresh);
                        }
                        if pill_button(
                            ui,
                            pal,
                            Some(IconKind::Close),
                            "",
                            ButtonVariant::Default,
                            true,
                            true,
                        ) {
                            actions.push(UiAction::DismissNotice);
                        }
                    });
                });
            let card_r = egui::Rect::from_min_max(
                res.response.rect.min,
                res.response.rect.max - Vec2::splat(4.0),
            );
            ui.painter().set(
                shadow_slot,
                egui::Shape::rect_filled(card_r.translate(Vec2::splat(3.5)), rad, pal.tactile_ink),
            );
        });
}

pub fn handle_keyboard_zoom(ctx: &egui::Context) {
    let delta: Option<f32> = ctx.input(|i| {
        if i.modifiers.command {
            if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                return Some(0.1);
            }
            if i.key_pressed(egui::Key::Minus) {
                return Some(-0.1);
            }
            if i.key_pressed(egui::Key::Num0) {
                return Some(0.0);
            }
            if i.smooth_scroll_delta.y > 0.0 {
                return Some(0.1);
            }
            if i.smooth_scroll_delta.y < 0.0 {
                return Some(-0.1);
            }
        }
        i.events.iter().find_map(|ev| match ev {
            egui::Event::MouseWheel {
                delta, modifiers, ..
            } if modifiers.command && delta.y != 0.0 => {
                Some(if delta.y > 0.0 { 0.1 } else { -0.1 })
            }
            _ => None,
        })
    });
    if let Some(d) = delta {
        let next = if d == 0.0 {
            1.0
        } else {
            (ctx.zoom_factor() + d).clamp(0.7, 2.0)
        };
        ctx.set_zoom_factor(next);
    }
}
