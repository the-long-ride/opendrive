use super::ControlCenterApp;
use crate::design::formatters::{
    validate_browser_oauth_draft, validate_service_account_draft, ConnectAuthMode, NewAccountDraft,
};
use crate::runtime::auth_runner::{
    start_browser_oauth, start_service_account_auth, AuthEvent, AuthSession, OAuthStartParams,
    ServiceAccountParams,
};
use eframe::egui;
use uuid::Uuid;

impl ControlCenterApp {
    pub fn poll_auth(&mut self) {
        while let Some(ev) = self.auth_session.as_mut().and_then(AuthSession::try_recv) {
            match ev {
                AuthEvent::BrowserReady {
                    auth_url,
                    copy_to_clipboard,
                } => {
                    if let Some(session) = self.auth_session.as_mut() {
                        session.auth_url = Some(auth_url.clone());
                    }
                    if copy_to_clipboard {
                        self.pending_clipboard = Some(auth_url);
                        self.set_ok("Copied Google sign-in link — paste in any browser");
                    }
                }
                AuthEvent::BrowserOpenWarning(err) => {
                    if let Some(url) = self.current_auth_url() {
                        self.pending_clipboard = Some(url);
                    }
                    self.set_err(format!(
                        "Browser launch failed ({err}) — sign-in link copied to clipboard"
                    ));
                }
                AuthEvent::Completed { state, message } => {
                    self.state = *state;
                    self.auth_session = None;
                    self.reauth_account_id = None;
                    self.drawer_open = false;
                    self.form_error = None;
                    let (client_id, client_secret) = self.bridge.saved_oauth_credentials();
                    self.draft = NewAccountDraft {
                        client_id,
                        client_secret,
                        ..Default::default()
                    };
                    self.set_ok(message);
                    self.bridge.spawn_mount_sync(self.repaint_ctx.clone());
                }
                AuthEvent::Failed(err) => {
                    self.auth_session = None;
                    self.form_error = Some(err);
                }
            }
        }
    }

    pub(super) fn current_auth_url(&self) -> Option<String> {
        self.auth_session.as_ref().and_then(|s| s.auth_url.clone())
    }

    pub(super) fn cancel_active_auth(&mut self) {
        if let Some(mut session) = self.auth_session.take() {
            session.cancel();
        }
    }

    pub(super) fn set_auth_mode(&mut self, mode: ConnectAuthMode) {
        if self.draft.auth_mode != mode {
            self.cancel_active_auth();
            self.draft.auth_mode = mode;
            self.form_error = None;
        }
    }

    pub(super) fn start_browser_oauth_flow(&mut self, open_browser: bool, copy_link: bool) {
        if let Some(err) = validate_browser_oauth_draft(&self.draft) {
            self.form_error = Some(err.to_string());
            return;
        }
        if let Err(err) = self
            .bridge
            .save_oauth_client_credentials(&self.draft.client_id, &self.draft.client_secret)
        {
            self.form_error = Some(err);
            return;
        }
        self.form_error = None;
        self.cancel_active_auth();
        let params = OAuthStartParams {
            target_account_id: self.reauth_account_id,
            client_id: self.draft.client_id.trim().to_string(),
            client_secret: self.draft.client_secret.trim().to_string(),
            nickname: self.draft.nickname.trim().to_string(),
            folder_name: self.draft.folder_name.trim().to_string(),
            read_only: self.draft.read_only,
            open_browser,
            copy_link,
        };
        self.auth_session = Some(start_browser_oauth(
            self.bridge.clone(),
            self.auth_endpoints.clone(),
            params,
            self.repaint_ctx.clone(),
        ));
    }

    pub(super) fn copy_auth_link(&mut self) {
        if let Some(url) = self.current_auth_url() {
            self.pending_clipboard = Some(url);
            self.set_ok("Copied Google sign-in link to clipboard");
        }
    }

    pub(super) fn open_auth_browser(&mut self) {
        if let Some(url) = self.current_auth_url() {
            if let Err(err) = (self.auth_endpoints.browser_opener)(&url) {
                self.pending_clipboard = Some(url);
                self.set_err(format!(
                    "Browser launch failed ({err}) — sign-in link copied to clipboard"
                ));
            }
        }
    }

    pub(super) fn start_service_account_flow(&mut self) {
        if let Some(err) = validate_service_account_draft(&self.draft) {
            self.form_error = Some(err.to_string());
            return;
        }
        self.form_error = None;
        self.cancel_active_auth();
        let params = ServiceAccountParams {
            target_account_id: self.reauth_account_id,
            json_or_path: self.draft.service_account_input.clone(),
            nickname: self.draft.nickname.trim().to_string(),
            folder_name: self.draft.folder_name.trim().to_string(),
            read_only: self.draft.read_only,
        };
        self.auth_session = Some(start_service_account_auth(
            self.bridge.clone(),
            self.auth_endpoints.clone(),
            params,
            self.repaint_ctx.clone(),
        ));
    }

    pub(super) fn start_account_reauth(&mut self, id: &str, open_browser: bool, copy_link: bool) {
        let Ok(acct_id) = Uuid::parse_str(id) else {
            return;
        };
        let Some(acct) = self
            .state
            .accounts
            .iter()
            .find(|a| a.id == acct_id)
            .cloned()
        else {
            return;
        };
        let (client_id, client_secret) = self.bridge.credentials_for_account(Some(acct_id));
        if !client_id.trim().is_empty() && !client_secret.trim().is_empty() {
            self.form_error = None;
            self.cancel_active_auth();
            let params = OAuthStartParams {
                target_account_id: Some(acct_id),
                client_id,
                client_secret,
                nickname: acct.nickname,
                folder_name: acct.folder_name,
                read_only: acct.read_only,
                open_browser,
                copy_link,
            };
            self.auth_session = Some(start_browser_oauth(
                self.bridge.clone(),
                self.auth_endpoints.clone(),
                params,
                self.repaint_ctx.clone(),
            ));
        } else {
            self.cancel_active_auth();
            self.reauth_account_id = Some(acct_id);
            self.drawer_open = true;
            self.draft = NewAccountDraft {
                auth_mode: ConnectAuthMode::BrowserOAuth,
                client_id,
                client_secret,
                nickname: acct.nickname,
                folder_name: acct.folder_name,
                read_only: acct.read_only,
                ..Default::default()
            };
            self.form_error = Some(
                match (
                    self.draft.client_id.trim().is_empty(),
                    self.draft.client_secret.trim().is_empty(),
                ) {
                    (false, true) => {
                        "Enter your OAuth Client Secret to re-authenticate (it will be saved in your OS keyring)."
                    }
                    (true, false) => "Enter your OAuth Client ID to re-authenticate.",
                    _ => "Enter your OAuth Client ID and Client Secret to re-authenticate.",
                }
                .into(),
            );
        }
    }

    pub(super) fn cancel_auth(&mut self) {
        if self.auth_session.is_some() {
            self.cancel_active_auth();
            self.set_ok("Canceled sign-in");
        }
    }

    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        if !(self.drawer_open && self.draft.auth_mode == ConnectAuthMode::ServiceAccount) {
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
