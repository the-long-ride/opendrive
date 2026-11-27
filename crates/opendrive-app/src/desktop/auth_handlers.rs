use super::ControlCenterApp;
use crate::design::formatters::{
    validate_browser_oauth_draft, validate_service_account_draft, ConnectAuthMode, NewAccountDraft,
};
use crate::runtime::app_logs::app_log_hub;
use crate::runtime::auth_runner::{
    start_browser_oauth, start_service_account_auth, AuthEvent, AuthSession, OAuthStartParams,
    ServiceAccountParams,
};
use opendrive_core::{Account, LogSeverity};
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
                    self.draft = NewAccountDraft::default();
                    app_log_hub().record_event(
                        LogSeverity::Info,
                        "opendrive::auth",
                        None,
                        &message,
                    );
                    self.set_ok(message);
                    self.bridge.spawn_mount_sync(self.repaint_ctx.clone());
                }
                AuthEvent::Failed(err) => {
                    let target = self
                        .auth_session
                        .as_ref()
                        .and_then(|s| s.target_account)
                        .or(self.reauth_account_id);
                    self.auth_session = None;
                    if !self.drawer_open {
                        self.reauth_account_id = None;
                    }
                    self.form_error = Some(err.clone());
                    let code = if opendrive_auth::is_credential_rejection_error(&err) {
                        "OD-AUTH-001"
                    } else if err.to_ascii_lowercase().contains("service account") {
                        "OD-AUTH-004"
                    } else {
                        "OD-AUTH-002"
                    };
                    app_log_hub().record_event(
                        LogSeverity::Error,
                        "opendrive::auth",
                        Some(code),
                        &err,
                    );
                    self.set_err_without_log(format!("Sign-in failed: {err}"));
                    if let Some(acct_id) = target.filter(|_| !self.drawer_open) {
                        let is_sa = self
                            .find_account(acct_id)
                            .is_some_and(|a| a.credential_set_id.is_nil());
                        let prompt = if is_sa {
                            format!("Service Account sign-in failed ({err}). Select or paste a valid JSON key below.")
                        } else if opendrive_auth::is_credential_rejection_error(&err) {
                            format!("Google rejected your OAuth credentials ({err}). Enter a valid Client ID and Client Secret below.")
                        } else {
                            format!("Authentication failed ({err}). Sign in again or update your Client ID and Client Secret below.")
                        };
                        self.open_reauth_drawer(acct_id, Some(prompt));
                    }
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

    pub(super) fn launch_oauth(
        &mut self,
        target_account_id: Option<Uuid>,
        (client_id, client_secret): (String, String),
        (nickname, folder_name, read_only): (String, String, bool),
        (open_browser, copy_link): (bool, bool),
    ) {
        self.form_error = None;
        self.cancel_active_auth();
        let params = OAuthStartParams {
            target_account_id,
            client_id,
            client_secret,
            nickname,
            folder_name,
            read_only,
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
        let d = &self.draft;
        let creds = (d.client_id.trim().into(), d.client_secret.trim().into());
        let meta = (
            d.nickname.trim().into(),
            d.folder_name.trim().into(),
            d.read_only,
        );
        let target = self.reauth_account_id;
        self.launch_oauth(target, creds, meta, (open_browser, copy_link));
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

    pub(super) fn find_account(&self, acct_id: Uuid) -> Option<Account> {
        self.state
            .accounts
            .iter()
            .find(|a| a.id == acct_id)
            .cloned()
    }

    pub(super) fn launch_service_account(
        &mut self,
        target_account_id: Option<Uuid>,
        json_or_path: String,
        (nickname, folder_name, read_only): (String, String, bool),
    ) {
        self.form_error = None;
        self.cancel_active_auth();
        self.auth_session = Some(start_service_account_auth(
            self.bridge.clone(),
            self.auth_endpoints.clone(),
            ServiceAccountParams {
                target_account_id,
                json_or_path,
                nickname,
                folder_name,
                read_only,
            },
            self.repaint_ctx.clone(),
        ));
    }

    pub(super) fn start_service_account_flow(&mut self) {
        if let Some(err) = validate_service_account_draft(&self.draft) {
            self.form_error = Some(err.to_string());
            return;
        }
        let d = &self.draft;
        let m = (
            d.nickname.trim().into(),
            d.folder_name.trim().into(),
            d.read_only,
        );
        self.launch_service_account(self.reauth_account_id, d.service_account_input.clone(), m);
    }

    pub(super) fn cancel_auth(&mut self) {
        if self.auth_session.is_some() {
            self.cancel_active_auth();
            if !self.drawer_open {
                self.reauth_account_id = None;
            }
            self.set_ok("Canceled sign-in");
        }
    }
}
