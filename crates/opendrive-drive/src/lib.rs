//! Thin Drive v3 HTTP client.

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
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn list_files(&self, _parent: &str, _page_token: Option<&str>) -> Result<FileList> {
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn changes(&self, _page_token: &str) -> Result<ChangeList> {
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn download_range(&self, _file_id: &str, _offset: u64, _len: u32) -> Result<Bytes> {
        Err(DriveError::Other("not implemented".into()))
    }
}

#[derive(Debug)]
pub struct About {
    pub email: String,
    pub storage_quota: u64,
    pub storage_used: u64,
}

#[derive(Debug)]
pub struct FileList {
    pub files: Vec<DriveFile>,
    pub next_page_token: Option<String>,
}

#[derive(Debug)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: Option<u64>,
    pub md5: Option<String>,
    pub modified_time: Option<String>,
}

#[derive(Debug)]
pub struct ChangeList {
    pub changes: Vec<Change>,
    pub new_start_token: Option<String>,
}

#[derive(Debug)]
pub struct Change {
    pub file_id: String,
    pub removed: bool,
    pub file: Option<DriveFile>,
}
