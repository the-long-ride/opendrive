//! Drive v3 HTTP client implementation (about, drives, files, download).

use crate::retry::with_backoff;
use crate::types::{About, DriveError, FileJson, FileList, Result, SharedDrive, SharedDriveList};
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Deserialize;

const BASE: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";
pub(crate) const FILE_FIELDS: &str =
    "id,name,mimeType,size,md5Checksum,modifiedTime,parents,trashed,ownedByMe,sharedWithMeTime,headRevisionId,shortcutDetails(targetId,targetMimeType)";

#[derive(Clone)]
pub struct DriveClient {
    pub access_token: String,
    pub(crate) http: reqwest::Client,
    pub(crate) api_base: String,
    pub(crate) upload_base: String,
}

impl DriveClient {
    pub fn new(access_token: String) -> Self {
        Self::with_base_urls(access_token, BASE, UPLOAD_BASE)
    }

    pub fn with_base_urls(
        access_token: String,
        api_base: impl Into<String>,
        upload_base: impl Into<String>,
    ) -> Self {
        Self {
            access_token,
            http: reqwest::Client::new(),
            api_base: api_base.into().trim_end_matches('/').to_string(),
            upload_base: upload_base.into().trim_end_matches('/').to_string(),
        }
    }

    pub(crate) fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.access_token)
    }

    pub(crate) async fn check(&self, resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        if status == 401 || status == 403 {
            return Err(DriveError::Auth);
        }
        if status == 429 {
            return Err(DriveError::RateLimited);
        }
        if status == 404 {
            return Err(DriveError::NotFound);
        }
        if !status.is_success() && status.as_u16() != 308 {
            let body = resp.text().await.unwrap_or_default();
            return Err(DriveError::Http(format!("{status}: {body}")));
        }
        Ok(resp)
    }

    pub(crate) async fn send_json<T: DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T> {
        let resp = req
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;
        let resp = self.check(resp).await?;
        resp.json()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))
    }

    pub async fn about(&self) -> Result<About> {
        with_backoff(|| self.about_once(), 5).await
    }

    async fn about_once(&self) -> Result<About> {
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
            #[serde(rename = "photoLink")]
            photo_link: Option<String>,
        }
        #[derive(Deserialize)]
        struct Quota {
            limit: Option<String>,
            usage: Option<String>,
        }
        let req = self
            .auth(self.http.get(format!("{}/about", self.api_base)))
            .query(&[(
                "fields",
                "user(emailAddress,displayName,photoLink),storageQuota",
            )]);
        let body: Resp = self.send_json(req).await?;
        Ok(About {
            email: body.user.email_address,
            display_name: body.user.display_name.unwrap_or_default(),
            photo_url: body.user.photo_link.filter(|s| !s.trim().is_empty()),
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

    pub async fn list_shared_drives(&self, page_token: Option<&str>) -> Result<SharedDriveList> {
        #[derive(Deserialize)]
        struct Resp {
            drives: Option<Vec<DriveJson>>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }
        #[derive(Deserialize)]
        struct DriveJson {
            id: String,
            name: String,
        }
        let mut req = self
            .auth(self.http.get(format!("{}/drives", self.api_base)))
            .query(&[
                ("pageSize", "100"),
                ("fields", "nextPageToken,drives(id,name)"),
            ]);
        if let Some(t) = page_token {
            req = req.query(&[("pageToken", t)]);
        }
        let body: Resp = self.send_json(req).await?;
        Ok(SharedDriveList {
            drives: body
                .drives
                .unwrap_or_default()
                .into_iter()
                .map(|d| SharedDrive {
                    id: d.id,
                    name: d.name,
                })
                .collect(),
            next_page_token: body.next_page_token,
        })
    }

    pub async fn list_shared_with_me(&self, page_token: Option<&str>) -> Result<FileList> {
        let page_token = page_token.map(|s| s.to_string());
        with_backoff(
            || {
                let page_token = page_token.clone();
                async move {
                    self.list_query_once(
                        "sharedWithMe = true and trashed = false",
                        page_token.as_deref(),
                        true,
                        None,
                    )
                    .await
                }
            },
            5,
        )
        .await
    }

    pub async fn list_files(
        &self,
        parent: &str,
        page_token: Option<&str>,
        supports_all_drives: bool,
    ) -> Result<FileList> {
        let parent = parent.to_string();
        let page_token = page_token.map(|s| s.to_string());
        with_backoff(
            || {
                let parent = parent.clone();
                let page_token = page_token.clone();
                async move {
                    let q = format!("'{parent}' in parents and trashed = false");
                    self.list_query_once(&q, page_token.as_deref(), supports_all_drives, None)
                        .await
                }
            },
            5,
        )
        .await
    }

    pub(crate) async fn list_query_once(
        &self,
        q: &str,
        page_token: Option<&str>,
        supports_all_drives: bool,
        order_by: Option<&str>,
    ) -> Result<FileList> {
        #[derive(Deserialize)]
        struct Resp {
            files: Option<Vec<FileJson>>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let mut req = self
            .auth(self.http.get(format!("{}/files", self.api_base)))
            .query(&[("q", q), ("fields", fields.as_str()), ("pageSize", "100")]);
        if let Some(ord) = order_by {
            req = req.query(&[("orderBy", ord)]);
        }
        if supports_all_drives {
            req = req.query(&[
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ]);
        }
        if let Some(t) = page_token {
            req = req.query(&[("pageToken", t)]);
        }
        let body: Resp = self.send_json(req).await?;
        Ok(FileList {
            files: body
                .files
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            next_page_token: body.next_page_token,
        })
    }

    pub async fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;
        let resp = self.check(resp).await?;
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| DriveError::Http(e.to_string()))
    }

    pub async fn download_range(&self, file_id: &str, offset: u64, len: u32) -> Result<Bytes> {
        if len == 0 {
            return Ok(Bytes::new());
        }
        let end = offset.saturating_add(len as u64).saturating_sub(1);
        let resp = self
            .auth(self.http.get(format!("{}/files/{file_id}", self.api_base)))
            .query(&[("alt", "media")])
            .header("Range", format!("bytes={offset}-{end}"))
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        if status != 200 && status != 206 {
            self.check(resp).await?;
            return Ok(Bytes::new());
        }
        resp.bytes()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))
    }

    pub fn native_doc_link(file_id: &str) -> String {
        format!("https://drive.google.com/open?id={file_id}")
    }
}
