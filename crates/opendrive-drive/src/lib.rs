//! Thin Drive v3 HTTP client: paging, changes, range/resumable, backoff.

use bytes::Bytes;
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

pub struct DriveClient {
    pub access_token: String,
}

impl DriveClient {
    pub fn new(access_token: String) -> Self {
        Self { access_token }
    }

    pub async fn about(&self) -> Result<About> {
        // GET https://www.googleapis.com/drive/v3/about?fields=user,storageQuota
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn list_files(
        &self,
        _parent: &str,
        _page_token: Option<&str>,
        _supports_all_drives: bool,
    ) -> Result<FileList> {
        // files.list with restricted fields mask + paging
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn changes(&self, _page_token: &str) -> Result<ChangeList> {
        // changes.list
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn get_start_page_token(&self) -> Result<String> {
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn download_range(
        &self,
        _file_id: &str,
        _offset: u64,
        _len: u32,
    ) -> Result<Bytes> {
        // Range request + read-ahead
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn create_resumable(
        &self,
        _metadata: &serde_json::Value,
    ) -> Result<String> {
        // returns session URI
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn upload_chunk(
        &self,
        _session_uri: &str,
        _offset: u64,
        _total: u64,
        _data: &[u8],
    ) -> Result<Option<DriveFile>> {
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn trash(&self, _file_id: &str) -> Result<()> {
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn update_metadata(
        &self,
        _file_id: &str,
        _patch: &serde_json::Value,
    ) -> Result<DriveFile> {
        Err(DriveError::Other("not implemented".into()))
    }
}

#[derive(Debug, Clone)]
pub struct About {
    pub email: String,
    pub display_name: String,
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
