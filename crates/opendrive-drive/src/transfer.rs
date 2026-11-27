//! Drive v3 HTTP client changes, upload, and metadata endpoints.

use crate::client::{DriveClient, FILE_FIELDS};
use crate::types::{Change, ChangeList, DriveError, DriveFile, FileJson, Result};
use bytes::Bytes;
use serde::Deserialize;

impl DriveClient {
    /// Resolves the canonical remote ID for the user's `"root"` ("My Drive") folder.
    pub async fn root_folder_id(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Resp {
            id: String,
        }
        crate::retry::with_backoff(
            || async move {
                let body: Resp = self
                    .send_authed_json(|tok| {
                        self.http
                            .get(format!("{}/files/root", self.api_base))
                            .bearer_auth(tok)
                            .query(&[("fields", "id"), ("supportsAllDrives", "true")])
                    })
                    .await?;
                Ok(body.id)
            },
            5,
        )
        .await
    }

    /// Queries top-level unparented items omitted from `'root' in parents` (orphans / link-opened files).
    pub async fn list_top_unparented_files(&self) -> Result<Vec<DriveFile>> {
        let list = crate::retry::with_backoff(
            || async move {
                self.list_query_once(
                    "trashed = false and not 'root' in parents",
                    None,
                    true,
                    Some("viewedByMeTime desc,modifiedTime desc"),
                )
                .await
            },
            5,
        )
        .await?;
        Ok(list
            .files
            .into_iter()
            .filter(|f| f.parents.is_empty())
            .collect())
    }

    pub async fn get_start_page_token(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(rename = "startPageToken")]
            start_page_token: String,
        }
        let body: Resp = self
            .send_authed_json(|tok| {
                self.http
                    .get(format!("{}/changes/startPageToken", self.api_base))
                    .bearer_auth(tok)
                    .query(&[("supportsAllDrives", "true")])
            })
            .await?;
        Ok(body.start_page_token)
    }

    pub async fn changes(&self, page_token: &str) -> Result<ChangeList> {
        #[derive(Deserialize)]
        struct Resp {
            changes: Option<Vec<ChangeJson>>,
            #[serde(rename = "newStartPageToken")]
            new_start_page_token: Option<String>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }
        #[derive(Deserialize)]
        struct ChangeJson {
            #[serde(rename = "fileId")]
            file_id: Option<String>,
            removed: Option<bool>,
            file: Option<FileJson>,
        }
        let fields =
            format!("nextPageToken,newStartPageToken,changes(fileId,removed,file({FILE_FIELDS}))");
        let body: Resp = self
            .send_authed_json(|tok| {
                self.http
                    .get(format!("{}/changes", self.api_base))
                    .bearer_auth(tok)
                    .query(&[
                        ("pageToken", page_token),
                        ("supportsAllDrives", "true"),
                        ("includeItemsFromAllDrives", "true"),
                        ("fields", fields.as_str()),
                    ])
            })
            .await?;
        let changes = body
            .changes
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| {
                let file_id = c
                    .file_id
                    .or_else(|| c.file.as_ref().map(|f| f.id.clone()))?;
                Some(Change {
                    file_id,
                    removed: c.removed.unwrap_or(false),
                    file: c.file.map(Into::into),
                })
            })
            .collect();
        Ok(ChangeList {
            changes,
            new_start_token: body.new_start_page_token.or(body.next_page_token),
        })
    }

    pub async fn create_resumable(&self, metadata: &serde_json::Value) -> Result<String> {
        let resp = self
            .send_authed(|tok| {
                self.http
                    .post(format!("{}/files", self.upload_base))
                    .bearer_auth(tok)
                    .query(&[("uploadType", "resumable"), ("supportsAllDrives", "true")])
                    .header("Content-Type", "application/json; charset=UTF-8")
                    .json(metadata)
            })
            .await?;
        resp.headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| DriveError::Other("missing Location header".into()))
    }

    pub async fn upload_chunk(
        &self,
        session_uri: &str,
        offset: u64,
        total: u64,
        data: &[u8],
    ) -> Result<Option<DriveFile>> {
        let range = if data.is_empty() {
            format!("bytes */{total}")
        } else {
            let end = offset + data.len() as u64 - 1;
            format!("bytes {offset}-{end}/{total}")
        };
        let resp = self
            .http
            .put(session_uri)
            .header("Content-Length", data.len().to_string())
            .header("Content-Range", range)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        if status == 308 {
            return Ok(None);
        }
        if status == 200 || status == 201 {
            let f: FileJson = resp
                .json()
                .await
                .map_err(|e| DriveError::Http(e.to_string()))?;
            return Ok(Some(f.into()));
        }
        self.check(resp).await?;
        Err(DriveError::Http(format!("unexpected status {status}")))
    }

    pub async fn trash(&self, file_id: &str) -> Result<()> {
        self.send_authed(|tok| {
            self.http
                .patch(format!("{}/files/{file_id}", self.api_base))
                .bearer_auth(tok)
                .query(&[("supportsAllDrives", "true")])
                .json(&serde_json::json!({ "trashed": true }))
        })
        .await?;
        Ok(())
    }

    pub async fn update_metadata(
        &self,
        file_id: &str,
        patch: &serde_json::Value,
    ) -> Result<DriveFile> {
        let f: FileJson = self
            .send_authed_json(|tok| {
                self.http
                    .patch(format!("{}/files/{file_id}", self.api_base))
                    .bearer_auth(tok)
                    .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
                    .json(patch)
            })
            .await?;
        Ok(f.into())
    }

    pub async fn copy_file(&self, file_id: &str, name: &str, parent_id: &str) -> Result<DriveFile> {
        let f: FileJson = self
            .send_authed_json(|tok| {
                self.http
                    .post(format!("{}/files/{file_id}/copy", self.api_base))
                    .bearer_auth(tok)
                    .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
                    .json(&serde_json::json!({ "name": name, "parents": [parent_id] }))
            })
            .await?;
        Ok(f.into())
    }

    pub async fn move_file(
        &self,
        file_id: &str,
        add_parent: &str,
        remove_parents: &str,
        name: Option<&str>,
    ) -> Result<DriveFile> {
        let mut q = vec![
            ("supportsAllDrives", "true"),
            ("fields", FILE_FIELDS),
            ("addParents", add_parent),
        ];
        if !remove_parents.is_empty() {
            q.push(("removeParents", remove_parents));
        }
        let body = match name {
            Some(n) => serde_json::json!({ "name": n }),
            None => serde_json::json!({}),
        };
        let f: FileJson = self
            .send_authed_json(|tok| {
                self.http
                    .patch(format!("{}/files/{file_id}", self.api_base))
                    .bearer_auth(tok)
                    .query(&q)
                    .json(&body)
            })
            .await?;
        Ok(f.into())
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
            .send_authed(|tok| {
                self.http
                    .get(format!("{}/files/{file_id}", self.api_base))
                    .bearer_auth(tok)
                    .query(&[("alt", "media")])
                    .header("Range", format!("bytes={offset}-{end}"))
            })
            .await?;
        let status = resp.status().as_u16();
        if status != 200 && status != 206 {
            return Ok(Bytes::new());
        }
        resp.bytes()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))
    }
}
