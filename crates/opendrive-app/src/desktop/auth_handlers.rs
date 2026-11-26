use super::ControlCenterApp;
use crate::design::formatters::{
    validate_browser_oauth_draft, validate_service_account_draft, ConnectAuthMode, NewAccountDraft,
};
use crate::runtime::auth_runner::{
    start_browser_oauth, start_service_account_auth, AuthEvent, AuthSession, OAuthStartParams,
    ServiceAccountParams,
};
use opendrive_core::Account;
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
                    self.set_err(format!("Sign-in failed: {err}"));
                    if let Some(acct_id) = target.filter(|_| !self.drawer_open) {
                        let is_sa = self
                            .find_account(acct_id)
                            .is_some_and(|a| a.credential_set_id.is_nil());
                        if is_sa {
                            self.open_reauth_drawer(
                                acct_id,
                                Some(format!(
                                    "Service Account sign-in failed ({err}). Select or paste a valid JSON key below."
                                )),
                            );
                        } else if opendrive_auth::is_credential_rejection_error(&err) {
                            self.open_reauth_drawer(
                                acct_id,
                                Some(format!(
                                    "Google rejected your OAuth credentials ({err}). Check Client ID and Client Secret below."
                                )),
                            );
                        }
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

    fn launch_oauth(
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

    fn find_account(&self, acct_id: Uuid) -> Option<Account> {
        self.state
            .accounts
            .iter()
            .find(|a| a.id == acct_id)
            .cloned()
    }

    fn launch_service_account(
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

    fn open_reauth_drawer(&mut self, acct_id: Uuid, custom_error: Option<String>) {
        let Some(acct) = self.find_account(acct_id) else {
            return;
        };
        let is_sa = acct.credential_set_id.is_nil();
        self.cancel_active_auth();
        self.reauth_account_id = Some(acct_id);
        self.drawer_open = true;
        let mut draft = NewAccountDraft {
            nickname: acct.nickname,
            folder_name: acct.folder_name,
            read_only: acct.read_only,
            ..Default::default()
        };
        let default_err = if is_sa {
            draft.auth_mode = ConnectAuthMode::ServiceAccount;
            draft.service_account_input = self
                .bridge
                .load_service_account_json(acct_id)
                .unwrap_or_default();
            "Select or paste your Service Account JSON key to re-authenticate."
        } else {
            let (cid, sec) = self.bridge.credentials_for_account(Some(acct_id));
            (draft.auth_mode, draft.client_id, draft.client_secret) =
                (ConnectAuthMode::BrowserOAuth, cid, sec);
            match (
                draft.client_id.trim().is_empty(),
                draft.client_secret.trim().is_empty(),
            ) {
                (false, true) => {
                    "Enter your OAuth Client Secret to re-authenticate (it will be saved in your OS keyring)."
                }
                (true, false) => "Enter your OAuth Client ID to re-authenticate.",
                _ => "Enter your OAuth Client ID and Client Secret to re-authenticate.",
            }
        };
        self.draft = draft;
        self.form_error = custom_error.or_else(|| Some(default_err.into()));
    }

    pub(super) fn start_account_reauth(&mut self, id: &str, open_browser: bool, copy_link: bool) {
        let Some(acct) = Uuid::parse_str(id)
            .ok()
            .and_then(|aid| self.find_account(aid))
        else {
            return;
        };
        let meta = (acct.nickname, acct.folder_name, acct.read_only);
        if acct.credential_set_id.is_nil() {
            if let Some(json) = self.bridge.load_service_account_json(acct.id) {
                self.reauth_account_id = Some(acct.id);
                self.launch_service_account(Some(acct.id), json, meta);
            } else {
                self.open_reauth_drawer(acct.id, None);
            }
            return;
        }
        let (client_id, client_secret) = self.bridge.credentials_for_account(Some(acct.id));
        if !client_id.trim().is_empty() && !client_secret.trim().is_empty() {
            self.reauth_account_id = Some(acct.id);
            self.launch_oauth(
                Some(acct.id),
                (client_id, client_secret),
                meta,
                (open_browser, copy_link),
            );
        } else {
            self.open_reauth_drawer(acct.id, None);
        }
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
