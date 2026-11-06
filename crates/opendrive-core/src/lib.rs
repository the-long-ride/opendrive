//! Domain model, settings, orchestration (UI-agnostic).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub nickname: String,
    pub email: String,
    pub credential_set_id: Uuid,
    pub folder_name: String,
    pub read_only: bool,
    pub enabled: bool,
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub root_path: String,
    pub cache_dir: String,
    pub cache_max_size: u64,
    pub bandwidth_limit: Option<u64>,
    pub autostart: bool,
    pub check_updates: bool,
    pub docs_mode: DocsMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
