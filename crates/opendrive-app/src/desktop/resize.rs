//! Frameless window 8-edge perimeter resize detection and native OS cursor feedback.

use crate::desktop::{ControlCenterApp, UiAction};
use eframe::egui::{self, CursorIcon, Pos2, Rect, ResizeDirection, ViewportCommand};
use opendrive_core::AppCommand;

const RESIZE_BORDER: f32 = 6.0;

pub fn detect_resize_direction(rect: Rect, pos: Pos2, border: f32) -> Option<ResizeDirection> {
    if !rect.contains(pos) {
        return None;
    }
    let left = pos.x - rect.left() <= border;
    let right = rect.right() - pos.x <= border;
    let top = pos.y - rect.top() <= border;
    let bottom = rect.bottom() - pos.y <= border;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (_, true, true, _) => Some(ResizeDirection::NorthEast),
        (true, _, _, true) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, false, false) => Some(ResizeDirection::West),
        (_, true, false, false) => Some(ResizeDirection::East),
        (false, false, true, _) => Some(ResizeDirection::North),
        (false, false, _, true) => Some(ResizeDirection::South),
        _ => None,
    }
}

pub fn resize_cursor_icon(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    }
}

pub fn handle_frameless_resize(ctx: &egui::Context) {
    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    if is_max {
        return;
    }
    let rect = ctx.content_rect();
    let (pos, pressed) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_pressed()));
    let Some(pos) = pos else {
        return;
    };
    if let Some(dir) = detect_resize_direction(rect, pos, RESIZE_BORDER) {
        ctx.set_cursor_icon(resize_cursor_icon(dir));
        if pressed {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}

impl ControlCenterApp {
    pub(crate) fn handle_window_action(&mut self, action: UiAction) {
        match action {
            UiAction::WindowMinimize => {
                if let Some(ctx) = &self.repaint_ctx {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            }
            UiAction::WindowToggleMaximize => {
                if let Some(ctx) = &self.repaint_ctx {
                    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!is_max));
                }
            }
            UiAction::WindowRequestClose => {
                if self.tray_active && !self.quit_requested {
                    self.hidden_to_tray = true;
                    if let Some(ctx) = &self.repaint_ctx {
                        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                    }
                } else {
                    self.quit_requested = true;
                    if let Some(ctx) = &self.repaint_ctx {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
            }
            UiAction::WindowShowFromTray => {
                self.hidden_to_tray = false;
                if let Some(ctx) = &self.repaint_ctx {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
            }
            UiAction::WindowForceQuit => {
                self.quit_requested = true;
                self.hidden_to_tray = false;
                self.shutdown_session();
                let _ = self.bridge.handle_blocking(AppCommand::Quit);
                if let Some(ctx) = &self.repaint_ctx {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
            _ => {}
        }
    }

    pub(super) fn shutdown_session(&mut self) {
        self.cancel_active_auth();
        self.bridge.mounts().shutdown_all();
    }

    pub(super) fn sync_tray_summary(&mut self) {
        let Some(updater) = &self.tray_summary_updater else {
            return;
        };
        let text = crate::shell::tray::format_tray_summary(&self.state.accounts);
        if text != self.last_tray_summary {
            updater(&text);
            self.last_tray_summary = text;
        }
    }

    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        if !(self.drawer_open
            && self.draft.auth_mode == crate::design::formatters::ConnectAuthMode::ServiceAccount)
        {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            let path = file.path();
            if !path.as_os_str().is_empty() {
                self.draft.service_account_input = path.display().to_string();
                self.form_error = None;
            } else if let Ok(bytes) = file.bytes() {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    self.draft.service_account_input = text.to_string();
                    self.form_error = None;
                }
            }
        }
    }

    pub(super) fn apply_result(
        &mut self,
        res: Result<crate::runtime::bridge::AppStateResponse, String>,
        ok_message: &str,
    ) {
        match res {
            Ok(next) => {
                self.state = next;
                self.set_ok(ok_message);
            }
            Err(err) => self.set_err(err),
        }
    }
}
