use super::{ControlCenterApp, NoticeTone, UiAction, UiNotice};
use crate::runtime::bridge_types::AliasEditDraft;
use crate::shell::tray::apply_tray_event;
use eframe::egui::{self, ViewportCommand};
use opendrive_core::{default_account_alias, DocsMode, Settings};
use uuid::Uuid;

impl ControlCenterApp {
    pub fn refresh(&mut self) {
        self.state = self.bridge.state_snapshot();
        self.avatar_attempted.clear();
        self.notice = None;
        self.notice_expires_at = None;
    }

    pub fn toggle_drawer(&mut self, force: Option<bool>) {
        self.drawer_open = force.unwrap_or(!self.drawer_open);
        self.form_error = None;
        if !self.drawer_open {
            self.reauth_account_id = None;
        }
    }

    pub fn toggle_settings(&mut self, force: Option<bool>) {
        self.settings_open = force.unwrap_or(!self.settings_open);
    }

    pub fn toggle_guide(&mut self, force: Option<bool>) {
        self.guide_open = force.unwrap_or(!self.guide_open);
    }

    pub fn open_vault_backup(&mut self) {
        self.drawer_open = false;
        self.settings_open = true;
    }

    pub fn open_cloud_console(&mut self) {
        if let Err(err) = (self.auth_endpoints.browser_opener)("https://console.cloud.google.com/")
        {
            self.set_err(format!("Could not open browser: {err}"));
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme_mode = self.theme_mode.toggle();
    }

    pub(super) fn set_ok(&mut self, message: impl Into<String>) {
        self.notice = Some(UiNotice {
            tone: NoticeTone::Ok,
            message: message.into(),
        });
        self.notice_expires_at =
            Some(std::time::Instant::now() + crate::desktop::panels::TOAST_TIMEOUT);
    }

    pub(super) fn set_err(&mut self, message: impl Into<String>) {
        self.notice = Some(UiNotice {
            tone: NoticeTone::Error,
            message: message.into(),
        });
        self.notice_expires_at =
            Some(std::time::Instant::now() + crate::desktop::panels::TOAST_TIMEOUT);
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
        self.notice_expires_at = None;
    }
    pub fn run_account_action(&mut self, action: &str, id: Option<&str>) {
        let ok_message = match action {
            "pause" => "Paused account sync",
            "resume" => "Resumed account sync",
            "remove" => "Removed account",
            "toggle_readonly" => "Updated read-only mount flag",
            "pause_all" => "Paused all accounts",
            "resume_all" => "Resumed all accounts",
            _ => "Updated account",
        };
        let res = self.bridge.account_action(action, id);
        let ok = res.is_ok();
        self.apply_result(res, ok_message);
        if ok {
            self.bridge.spawn_mount_sync(self.repaint_ctx.clone());
        }
    }

    fn update_settings_with(&mut self, ok_message: &str, mutate: impl FnOnce(&mut Settings)) {
        let mut s = self.state.settings.clone();
        mutate(&mut s);
        let res = self.bridge.update_settings(
            s.cache_max_size,
            s.bandwidth_limit,
            s.autostart,
            s.docs_mode,
        );
        self.apply_result(res, ok_message);
    }

    pub fn set_bandwidth(&mut self, limit: Option<u64>) {
        let msg = if limit.unwrap_or(0) == 0 {
            "Bandwidth set to Unlimited"
        } else {
            "Updated bandwidth limit"
        };
        self.update_settings_with(msg, |s| s.bandwidth_limit = limit);
    }

    pub fn set_cache_max(&mut self, max_bytes: u64) {
        self.update_settings_with("Updated local cache limit", |s| {
            s.cache_max_size = max_bytes
        });
    }

    pub fn set_docs_mode(&mut self, mode: DocsMode) {
        let msg = match mode {
            DocsMode::Link => "Docs mode set to Link (.gdoc)",
            DocsMode::ExportOffice => "Docs mode set to ExportOffice",
        };
        self.update_settings_with(msg, |s| s.docs_mode = mode);
    }

    pub fn toggle_autostart(&mut self) {
        let next = !self.state.settings.autostart;
        let msg = if next {
            "Autostart enabled"
        } else {
            "Autostart disabled"
        };
        self.update_settings_with(msg, |s| s.autostart = next);
    }

    pub fn toggle_virtual_drive(&mut self) {
        let next = !self.state.settings.virtual_drive_enabled;
        let msg = if next {
            format!(
                "Mounting virtual drive {}:\\...",
                self.state.settings.virtual_drive_letter
            )
        } else {
            "Unmounted virtual drive".into()
        };
        let res = self.bridge.update_virtual_drive(next);
        let ok = res.is_ok();
        self.apply_result(res, &msg);
        if ok {
            self.bridge.spawn_mount_sync(self.repaint_ctx.clone());
        }
    }

    pub fn open_mount_root(&mut self) {
        let target = if self.state.settings.virtual_drive_enabled
            && opendrive_platform::is_virtual_drive_supported()
        {
            format!("{}:\\", self.state.settings.virtual_drive_letter)
        } else {
            let dir = self.state.mount_root.clone();
            let _ = std::fs::create_dir_all(&dir);
            dir
        };
        if let Err(err) = (self.auth_endpoints.folder_opener)(&target) {
            self.set_err(format!("Failed to open folder: {err}"));
        }
    }

    pub fn purge_cache(&mut self) {
        self.apply_result(self.bridge.evict_cache(0), "Purged unpinned local cache");
    }

    pub(super) fn drain_tray_and_close(
        &mut self,
        ctx: &egui::Context,
        actions: &mut Vec<UiAction>,
    ) {
        if let Some(q) = &self.tray_events {
            let drained: Vec<_> = q.lock().expect("tray queue").drain(..).collect();
            for ev in drained {
                apply_tray_event(ev, actions);
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.tray_active && !self.quit_requested
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            actions.push(UiAction::WindowRequestClose);
        }
    }

    pub(super) fn start_edit_alias(&mut self, id_str: &str) {
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

    pub(super) fn save_edit_alias(&mut self) {
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

    fn set_vault_msg(&mut self, is_err: bool, msg: impl Into<String>) {
        let s = msg.into();
        self.vault_draft.feedback_is_err = is_err;
        self.vault_draft.feedback = Some(s.clone());
        if is_err {
            self.set_err(s);
        } else {
            self.set_ok(s);
        }
    }

    pub(super) fn export_vault_flow(&mut self) {
        let pw = self.vault_draft.password.clone();
        if pw != self.vault_draft.confirm_password {
            self.set_vault_msg(true, "Passwords do not match.");
            return;
        }
        let bytes = match self.bridge.export_encrypted_vault(&pw) {
            Ok(b) => b,
            Err(err) => {
                self.set_vault_msg(true, err);
                return;
            }
        };
        let Some(path) = (self.auth_endpoints.vault_save_picker)() else {
            return;
        };
        if let Err(err) = std::fs::write(&path, bytes) {
            self.set_vault_msg(true, format!("Failed to write backup: {err}"));
            return;
        }
        self.vault_draft.password.clear();
        self.vault_draft.confirm_password.clear();
        self.set_vault_msg(
            false,
            format!("Exported encrypted credentials to {}", path.display()),
        );
    }

    pub(super) fn import_vault_flow(&mut self) {
        let pw = self.vault_draft.password.clone();
        let Some(path) = (self.auth_endpoints.vault_open_picker)() else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(err) => {
                self.set_vault_msg(true, format!("Failed to read backup: {err}"));
                return;
            }
        };
        match self.bridge.import_encrypted_vault(&bytes, &pw) {
            Ok((next, count)) => {
                self.state = next;
                self.vault_draft.password.clear();
                self.vault_draft.confirm_password.clear();
                self.avatar_attempted.clear();
                self.set_vault_msg(false, format!("Imported {count} account(s) from backup."));
                self.bridge.spawn_mount_sync(self.repaint_ctx.clone());
            }
            Err(err) => self.set_vault_msg(true, err),
        }
    }
}
