//! Thin Drive v3 HTTP client: paging, changes, range/resumable, backoff.

use bytes::Bytes;
use serde::Deserialize;
use thiserror::Error;

const BASE: &str = "https://www.googleapis.com/drive/v3";

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
    http: reqwest::Client,
}

impl DriveClient {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            http: reqwest::Client::new(),
        }
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.access_token)
    }

    pub async fn about(&self) -> Result<About> {
        #[derive(Deserialize)]
        struct Resp {
            user: User,
            #[serde(rename = "storageQuota")]
            storage_quota: Quota,
        }
        #[derive(Deserialize)]
        struct User {
            #[serde(rename = "emailAddress")]
            email_address: String,
            #[serde(rename = "displayName")]
            display_name: Option<String>,
        }
        #[derive(Deserialize)]
        struct Quota {
            limit: Option<String>,
            usage: Option<String>,
        }

        let resp = self
            .auth(self.http.get(format!("{BASE}/about")))
            .query(&[("fields", "user,storageQuota")])
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;

        if resp.status() == 401 || resp.status() == 403 {
            return Err(DriveError::Auth);
        }
        if !resp.status().is_success() {
            return Err(DriveError::Http(resp.status().to_string()));
        }

        let body: Resp = resp.json().await.map_err(|e| DriveError::Http(e.to_string()))?;
        Ok(About {
            email: body.user.email_address,
            display_name: body.user.display_name.unwrap_or_default(),
            storage_quota: body
                .storage_quota
                .limit
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            storage_used: body
                .storage_quota
                .usage
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        })
    }

    pub async fn list_files(
        &self,
        parent: &str,
        page_token: Option<&str>,
        supports_all_drives: bool,
    ) -> Result<FileList> {
        #[derive(Deserialize)]
        struct Resp {
            files: Option<Vec<FileJson>>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }
        #[derive(Deserialize)]
        struct FileJson {
            id: String,
            name: String,
            #[serde(rename = "mimeType")]
            mime_type: String,
            size: Option<String>,
            #[serde(rename = "md5Checksum")]
            md5: Option<String>,
            #[serde(rename = "modifiedTime")]
            modified_time: Option<String>,
            parents: Option<Vec<String>>,
            trashed: Option<bool>,
            #[serde(rename = "headRevisionId")]
            head_revision_id: Option<String>,
        }

        let q = format!("'{}' in parents and trashed = false", parent);
        let mut req = self
            .auth(self.http.get(format!("{BASE}/files")))
            .query(&[
                ("q", q.as_str()),
                (
                    "fields",
                    "nextPageToken,files(id,name,mimeType,size,md5Checksum,modifiedTime,parents,trashed,headRevisionId)",
                ),
                ("pageSize", "100"),
            ]);
        if supports_all_drives {
            req = req.query(&[
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ]);
        }
        if let Some(t) = page_token {
            req = req.query(&[("pageToken", t)]);
        }

        let resp = req.send().await.map_err(|e| DriveError::Http(e.to_string()))?;
        if resp.status() == 429 {
            return Err(DriveError::RateLimited);
        }
        if !resp.status().is_success() {
            return Err(DriveError::Http(resp.status().to_string()));
        }

        let body: Resp = resp.json().await.map_err(|e| DriveError::Http(e.to_string()))?;
        let files = body
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|f| DriveFile {
                id: f.id,
                name: f.name,
                mime_type: f.mime_type,
                size: f.size.and_then(|s| s.parse().ok()),
                md5: f.md5,
                modified_time: f.modified_time,
                parents: f.parents.unwrap_or_default(),
                trashed: f.trashed.unwrap_or(false),
                head_revision_id: f.head_revision_id,
            })
            .collect();

        Ok(FileList {
            files,
            next_page_token: body.next_page_token,
        })
    }

    pub async fn changes(&self, _page_token: &str) -> Result<ChangeList> {
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
        Err(DriveError::Other("not implemented".into()))
    }

    pub async fn create_resumable(&self, _metadata: &serde_json::Value) -> Result<String> {
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

    pub fn native_doc_link(file_id: &str) -> String {
        format!("https://drive.google.com/open?id={file_id}")
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

pub fn is_native_google_doc(mime: &str) -> bool {
    matches!(
        mime,
        "application/vnd.google-apps.document"
            | "application/vnd.google-apps.spreadsheet"
            | "application/vnd.google-apps.presentation"
    )
}
