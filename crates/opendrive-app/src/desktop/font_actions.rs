//! Desktop UI font discovery, custom import/validation, reset, and live egui font syncing.

use super::ControlCenterApp;
use crate::design::theme_fonts::apply_ui_font;
use crate::runtime::app_logs::app_log_hub;
use crate::runtime::font_import::{import_local_font_file, spawn_font_import};
use crate::runtime::fonts::{discover_system_fonts, resolve_font_bytes};
use eframe::egui;
use opendrive_core::LogSeverity;
use std::path::PathBuf;
use std::sync::mpsc;

impl ControlCenterApp {
    pub(crate) fn fonts_storage_dir(&self) -> PathBuf {
        self.bridge
            .data_dir()
            .map(|d| d.join("fonts"))
            .unwrap_or_else(|| std::env::temp_dir().join("opendrive-fonts"))
    }

    pub(crate) fn ensure_font_catalog_loaded(&mut self) {
        if self.fonts_loaded {
            return;
        }
        self.fonts_loaded = true;
        let dir = self.fonts_storage_dir();
        self.available_fonts = discover_system_fonts(Some(&dir));
    }

    pub(crate) fn sync_applied_ui_font(&mut self, ctx: &egui::Context) {
        let target = self.state.settings.ui_font_family.clone();
        if self.applied_font.as_ref() == Some(&target) {
            return;
        }
        self.applied_font = Some(target.clone());
        let bytes = target.as_deref().and_then(|fam| {
            self.ensure_font_catalog_loaded();
            resolve_font_bytes(fam, &self.available_fonts)
        });
        apply_ui_font(ctx, target.as_deref(), bytes);
    }

    fn set_font_msg(&mut self, is_err: bool, msg: impl Into<String>) {
        let text = msg.into();
        self.font_draft.feedback_is_err = is_err;
        self.font_draft.feedback = Some(text.clone());
        if is_err {
            self.set_err(text);
        } else {
            self.set_ok(text);
        }
    }

    fn set_font_err(&mut self, code: &str, msg: impl Into<String>) {
        let text = msg.into();
        app_log_hub().record_event(LogSeverity::Error, "opendrive::font", Some(code), &text);
        self.font_draft.feedback_is_err = true;
        self.font_draft.feedback = Some(text.clone());
        self.set_err_without_log(text);
    }

    pub(crate) fn select_ui_font(&mut self, family: String) {
        self.ensure_font_catalog_loaded();
        let trimmed = family.trim();
        if trimmed.is_empty() {
            self.reset_ui_font();
            return;
        }
        let resolved = self
            .available_fonts
            .iter()
            .find(|f| f.family.eq_ignore_ascii_case(trimmed))
            .map(|f| f.family.clone());
        let Some(matched) = resolved else {
            let msg = format!("Font \"{trimmed}\" is not installed; import it first.");
            self.set_font_err("OD-FONT-001", msg);
            return;
        };
        let res = self.bridge.update_ui_font(Some(matched.clone()));
        if res.is_ok() {
            self.applied_font = None;
            self.set_font_msg(false, format!("UI font set to {matched}"));
            app_log_hub().record_event(
                LogSeverity::Info,
                "opendrive::font",
                None,
                &format!("UI font changed to {matched}"),
            );
        }
        if let Err(err) = res {
            self.set_font_msg(true, err);
        } else if let Ok(next) = res {
            self.state = next;
        }
    }

    pub(crate) fn reset_ui_font(&mut self) {
        match self.bridge.update_ui_font(None) {
            Ok(next) => {
                self.state = next;
                self.applied_font = None;
                self.set_font_msg(false, "Restored default OpenDrive UI font");
            }
            Err(err) => self.set_font_msg(true, err),
        }
    }

    pub(crate) fn import_ui_font(&mut self) {
        let input = self.font_draft.import_input.trim().to_string();
        if input.is_empty() {
            self.set_font_msg(
                true,
                "Enter a Google Font name, public https:// font URL, or local .ttf/.otf path.",
            );
            return;
        }
        self.ensure_font_catalog_loaded();
        if let Some(existing) = self
            .available_fonts
            .iter()
            .find(|f| f.family.eq_ignore_ascii_case(&input))
            .map(|f| f.family.clone())
        {
            self.font_draft.import_input.clear();
            self.select_ui_font(existing);
            return;
        }
        self.font_draft.importing = true;
        self.font_draft.feedback = Some(format!("Validating and importing \"{input}\"..."));
        self.font_draft.feedback_is_err = false;
        self.font_import = Some(spawn_font_import(
            input,
            self.auth_endpoints.google_fonts_css_base.clone(),
            self.fonts_storage_dir(),
            self.repaint_ctx.clone(),
        ));
    }

    pub(crate) fn browse_ui_font(&mut self) {
        let Some(path) = (self.auth_endpoints.font_open_picker)() else {
            return;
        };
        let dir = self.fonts_storage_dir();
        match import_local_font_file(&path, &dir) {
            Ok((family, _)) => {
                self.fonts_loaded = false;
                self.ensure_font_catalog_loaded();
                self.select_ui_font(family);
            }
            Err(err) => self.set_font_err("OD-FONT-001", err),
        }
    }

    pub(super) fn poll_font_import(&mut self) {
        let Some(handle) = &self.font_import else {
            return;
        };
        let res = match handle.rx.try_recv() {
            Ok(res) => res,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.font_import = None;
                self.font_draft.importing = false;
                self.set_font_err("OD-FONT-002", "Font import worker stopped unexpectedly.");
                return;
            }
        };
        self.font_import = None;
        self.font_draft.importing = false;
        match res {
            Ok((family, _)) => {
                self.font_draft.import_input.clear();
                self.fonts_loaded = false;
                self.ensure_font_catalog_loaded();
                self.select_ui_font(family);
            }
            Err(err) => {
                let low = err.to_ascii_lowercase();
                let code = if [
                    "not found",
                    "no downloadable",
                    "unsupported font",
                    "too small",
                    "truncated",
                    "public https://",
                    "public link",
                    "publicly accessible",
                    "public internet host",
                    "invalid font url",
                ]
                .iter()
                .any(|k| low.contains(k))
                {
                    "OD-FONT-001"
                } else {
                    "OD-FONT-002"
                };
                self.set_font_err(code, err);
            }
        }
    }

    pub(super) fn handle_utility_action(&mut self, action: crate::desktop::UiAction) {
        use crate::desktop::UiAction;
        match action {
            UiAction::ToggleTerminal(force) => self.toggle_terminal(force),
            UiAction::SetTerminalTab(tab) => self.terminal.active_tab = tab,
            UiAction::CopyLogs => self.copy_logs(),
            UiAction::ClearLogs => app_log_hub().clear(),
            UiAction::ToggleSupportModal(force) => self.toggle_support_modal(force),
            UiAction::CopySupportPrompt => self.copy_support_prompt(),
            UiAction::SelectUiFont(fam) => self.select_ui_font(fam),
            UiAction::ResetUiFont => self.reset_ui_font(),
            UiAction::ImportUiFont => self.import_ui_font(),
            UiAction::BrowseUiFont => self.browse_ui_font(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::bridge::Bridge;
    use crate::runtime::font_import::FontImportHandle;
    use opendrive_core::Settings;

    #[test]
    fn poll_font_import_handles_disconnected_worker() {
        let (bridge, _rx) = Bridge::new(Settings::default());
        let mut app = ControlCenterApp::new(bridge);
        let (tx, rx) = mpsc::channel();
        drop(tx);
        app.font_draft.importing = true;
        app.font_import = Some(FontImportHandle { rx });
        app.poll_font_import();
        assert!(!app.font_draft.importing);
        assert!(app.font_draft.feedback_is_err);
    }
}
