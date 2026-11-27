//! Re-authentication modal & credential-editing helpers for ControlCenterApp.

use super::ControlCenterApp;
use crate::design::formatters::{ConnectAuthMode, NewAccountDraft};
use uuid::Uuid;

impl ControlCenterApp {
    pub(super) fn open_reauth_drawer(&mut self, acct_id: Uuid, custom_error: Option<String>) {
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

    pub(super) fn open_reauth_credentials(&mut self, id: &str) {
        if let Ok(acct_id) = Uuid::parse_str(id.trim()) {
            self.open_reauth_drawer(acct_id, None);
        }
    }
}
