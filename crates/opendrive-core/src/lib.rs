//! Domain model, settings, orchestration (UI-agnostic).

mod accounts_io;
mod controller;
mod credentials_io;
pub mod error_codes;
mod open_rules;
mod settings_io;

pub use accounts_io::{accounts_path, load_accounts, save_accounts};
pub use controller::{AppCommand, AppController, AppEvent};
pub use credentials_io::{credentials_path, load_credentials, save_credentials};
pub use error_codes::{
    infer_diagnostic_code, lookup_diagnostic, DiagnosticCode, LogSeverity, DIAGNOSTIC_CODES,
};
pub use open_rules::{
    classify_media_kind, normalize_rule_tokens, resolve_open_app, supports_http_stream_arg,
    validate_open_with_rule, MediaKind, OpenWithRule,
};
pub use settings_io::{load_settings, save_settings, settings_path};

pub fn default_account_alias(nickname: &str, email: &str) -> String {
    let trimmed = nickname.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    email
        .split('@')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Drive")
        .to_string()
}

pub fn sanitize_folder_segment(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    let clean: String = trimmed
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let final_name = clean.trim().trim_matches('.');
    (!final_name.is_empty()).then(|| final_name.to_string())
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialSet {
    pub id: Uuid,
    pub label: String,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountStatus {
    Connected,
    Paused,
    NeedsReauth,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: Uuid,
    pub nickname: String,
    pub email: String,
    pub credential_set_id: Uuid,
    pub folder_name: String,
    pub read_only: bool,
    pub enabled: bool,
    pub status: AccountStatus,
    #[serde(default)]
    pub storage_used: u64,
    #[serde(default)]
    pub storage_quota: u64,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

fn default_virtual_drive_letter() -> String {
    "O".into()
}

fn default_explorer_folder_cache() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub root_path: String,
    pub cache_dir: String,
    pub cache_max_size: u64,
    pub bandwidth_limit: Option<u64>,
    pub autostart: bool,
    pub check_updates: bool,
    pub docs_mode: DocsMode,
    #[serde(default)]
    pub virtual_drive_enabled: bool,
    #[serde(default = "default_virtual_drive_letter")]
    pub virtual_drive_letter: String,
    #[serde(default)]
    pub ignored_release_version: Option<String>,
    #[serde(default)]
    pub open_with_rules: Vec<OpenWithRule>,
    #[serde(default = "default_explorer_folder_cache")]
    pub explorer_folder_cache: bool,
    #[serde(default)]
    pub ui_font_family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum DocsMode {
    #[default]
    Link,
    ExportOffice,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root_path: String::new(),
            cache_dir: String::new(),
            cache_max_size: 10 * 1024 * 1024 * 1024,
            bandwidth_limit: None,
            autostart: false,
            check_updates: false,
            docs_mode: DocsMode::Link,
            virtual_drive_enabled: false,
            virtual_drive_letter: default_virtual_drive_letter(),
            ignored_release_version: None,
            open_with_rules: Vec::new(),
            explorer_folder_cache: true,
            ui_font_family: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("account not found: {0}")]
    AccountNotFound(Uuid),
    #[error("credential set not found: {0}")]
    CredentialNotFound(Uuid),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Default)]
pub struct AccountManager {
    accounts: Vec<Account>,
    credentials: Vec<CredentialSet>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from(&mut self, config_dir: &std::path::Path) -> Result<()> {
        self.accounts = load_accounts(config_dir)?;
        self.credentials = load_credentials(config_dir)?;
        Ok(())
    }

    pub fn save_to(&self, config_dir: &std::path::Path) -> Result<()> {
        save_accounts(config_dir, &self.accounts)?;
        save_credentials(config_dir, &self.credentials)?;
        Ok(())
    }

    pub fn add_credential(&mut self, set: CredentialSet) {
        self.credentials.push(set);
    }

    pub fn list_credentials(&self) -> &[CredentialSet] {
        &self.credentials
    }

    pub fn get_credential(&self, id: Uuid) -> Result<&CredentialSet> {
        self.credentials
            .iter()
            .find(|c| c.id == id)
            .ok_or(CoreError::CredentialNotFound(id))
    }

    pub fn add_account(&mut self, account: Account) {
        self.accounts.push(account);
    }

    pub fn get_account(&self, id: Uuid) -> Result<&Account> {
        self.accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or(CoreError::AccountNotFound(id))
    }

    pub fn get_account_mut(&mut self, id: Uuid) -> Result<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(CoreError::AccountNotFound(id))
    }

    pub fn list_accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn remove_account(&mut self, id: Uuid) -> Result<()> {
        let before = self.accounts.len();
        self.accounts.retain(|a| a.id != id);
        if self.accounts.len() == before {
            return Err(CoreError::AccountNotFound(id));
        }
        Ok(())
    }

    pub fn move_account(&mut self, id: Uuid, delta: isize) -> Result<bool> {
        let idx = self
            .accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or(CoreError::AccountNotFound(id))?;
        let target = (idx as isize + delta).clamp(0, self.accounts.len() as isize - 1) as usize;
        if target == idx {
            return Ok(false);
        }
        let account = self.accounts.remove(idx);
        self.accounts.insert(target, account);
        Ok(true)
    }
}
