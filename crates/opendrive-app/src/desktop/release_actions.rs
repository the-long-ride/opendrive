use super::ControlCenterApp;
use crate::design::avatars::sync_avatar_textures;
use crate::design::theme::apply_studio_theme;
use crate::runtime::release_check::spawn_release_check;
use eframe::egui;

impl ControlCenterApp {
    pub(super) fn poll_bridge_and_avatars(&mut self, ctx: &egui::Context) {
        if self.bridge.take_mount_refresh() {
            self.state = self.bridge.state_snapshot();
            self.avatar_attempted.clear();
        }
        if let Some((is_err, msg)) = self.bridge.take_mount_notice() {
            if is_err {
                self.set_err(msg);
            } else {
                self.set_ok(msg);
            }
        }
        if let Some(clip) = self.pending_clipboard.take() {
            ctx.copy_text(clip);
        }
        if self.applied_theme != Some(self.theme_mode) {
            apply_studio_theme(ctx, self.theme_mode);
            self.applied_theme = Some(self.theme_mode);
        }
        sync_avatar_textures(
            ctx,
            self.bridge.data_dir(),
            &self.state.accounts,
            &mut self.avatar_textures,
            &mut self.avatar_attempted,
        );
    }

    pub(super) fn maybe_check_releases_on_startup(&mut self) {
        if self.startup_release_checked {
            return;
        }
        self.startup_release_checked = true;
        if self.state.settings.check_updates {
            self.start_release_check(false);
        }
    }

    pub fn start_release_check(&mut self, manual: bool) {
        if self.release_check.is_some() {
            return;
        }
        self.release_checking = true;
        let url = self.auth_endpoints.release_api_url.clone();
        self.release_check = Some(spawn_release_check(
            url,
            env!("CARGO_PKG_VERSION").to_string(),
            manual,
            self.repaint_ctx.clone(),
        ));
    }

    pub(super) fn poll_release_check(&mut self) {
        let Some(handle) = &self.release_check else {
            return;
        };
        let manual = handle.manual;
        let Ok(res) = handle.rx.try_recv() else {
            return;
        };
        self.release_check = None;
        self.release_checking = false;
        match res {
            Ok(Some(info)) => {
                let ignored = self.state.settings.ignored_release_version.as_deref()
                    == Some(info.tag_name.as_str());
                if manual || !ignored {
                    self.available_release = Some(info);
                }
            }
            Ok(None) => {
                if manual {
                    self.set_ok(format!(
                        "OpenDrive is up to date (v{})",
                        env!("CARGO_PKG_VERSION")
                    ));
                }
            }
            Err(err) => {
                if manual {
                    self.set_err(format!("Release check failed: {err}"));
                }
            }
        }
    }

    pub fn toggle_check_updates(&mut self) {
        let enabled = !self.state.settings.check_updates;
        let ignored = self.state.settings.ignored_release_version.clone();
        let msg = if enabled {
            "GitHub release check enabled"
        } else {
            "GitHub release check disabled"
        };
        self.apply_result(self.bridge.update_release_prefs(enabled, ignored), msg);
        if enabled {
            self.start_release_check(false);
        }
    }

    pub fn open_external_url(&mut self, url: &str) {
        if let Err(err) = (self.auth_endpoints.browser_opener)(url) {
            self.set_err(format!("Could not open browser: {err}"));
        }
    }

    pub fn open_release_page(&mut self, url: &str) {
        self.available_release = None;
        self.open_external_url(url);
    }

    pub fn ignore_release_version(&mut self, tag: &str) {
        self.available_release = None;
        let enabled = self.state.settings.check_updates;
        self.apply_result(
            self.bridge
                .update_release_prefs(enabled, Some(tag.to_string())),
            &format!("Won't remind again for {tag}"),
        );
    }
}
