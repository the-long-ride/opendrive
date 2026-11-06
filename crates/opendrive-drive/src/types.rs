//! Drive v3 API data types and error definitions.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriveError {
    #[error("http: {0}")]
    Http(String),
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("auth")]
    Auth,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, DriveError>;

#[derive(Debug, Clone, Deserialize)]
pub struct ShortcutDetailsJson {
    #[serde(rename = "targetId")]
    pub target_id: Option<String>,
    #[serde(rename = "targetMimeType")]
    pub target_mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileJson {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub size: Option<String>,
    #[serde(rename = "md5Checksum")]
    pub md5: Option<String>,
    #[serde(rename = "modifiedTime")]
    pub modified_time: Option<String>,
    pub parents: Option<Vec<String>>,
    pub trashed: Option<bool>,
    #[serde(rename = "headRevisionId")]
    pub head_revision_id: Option<String>,
    #[serde(rename = "shortcutDetails")]
    pub shortcut_details: Option<ShortcutDetailsJson>,
}

impl From<FileJson> for DriveFile {
    fn from(f: FileJson) -> Self {
        let (shortcut_target_id, shortcut_target_mime) = f
            .shortcut_details
            .map(|d| (d.target_id, d.target_mime_type))
            .unwrap_or((None, None));
        Self {
            id: f.id,
            name: f.name,
            mime_type: f.mime_type,
            size: f.size.and_then(|s| s.parse().ok()),
            md5: f.md5,
            modified_time: f.modified_time,
            parents: f.parents.unwrap_or_default(),
            trashed: f.trashed.unwrap_or(false),
            head_revision_id: f.head_revision_id,
            shortcut_target_id,
            shortcut_target_mime,
        }
    }
}

#[derive(Debug, Clone)]
pub struct About {
    pub email: String,
    pub display_name: String,
    pub photo_url: Option<String>,
    pub storage_quota: u64,
    pub storage_used: u64,
}

#[derive(Debug, Clone)]
pub struct FileList {
    pub files: Vec<DriveFile>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: Option<u64>,
    pub md5: Option<String>,
    pub modified_time: Option<String>,
    pub parents: Vec<String>,
    pub trashed: bool,
    pub head_revision_id: Option<String>,
    pub shortcut_target_id: Option<String>,
    pub shortcut_target_mime: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SharedDrive {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SharedDriveList {
    pub drives: Vec<SharedDrive>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChangeList {
    pub changes: Vec<Change>,
    pub new_start_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub file_id: String,
    pub removed: bool,
    pub file: Option<DriveFile>,
}

pub fn is_native_google_doc(mime: &str) -> bool {
    matches!(
        mime,
        "application/vnd.google-apps.document"
            | "application/vnd.google-apps.spreadsheet"
            | "application/vnd.google-apps.presentation"
    )
}
