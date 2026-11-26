//! Drive v3 HTTP client changes, upload, and metadata endpoints.

use crate::client::{DriveClient, FILE_FIELDS};
use crate::types::{Change, ChangeList, DriveError, DriveFile, FileJson, Result};
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
                let req = self
                    .auth(self.http.get(format!("{}/files/root", self.api_base)))
                    .query(&[("fields", "id"), ("supportsAllDrives", "true")]);
                let body: Resp = self.send_json(req).await?;
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
        let req = self
            .auth(
                self.http
                    .get(format!("{}/changes/startPageToken", self.api_base)),
            )
            .query(&[("supportsAllDrives", "true")]);
        let body: Resp = self.send_json(req).await?;
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
        let req = self
            .auth(self.http.get(format!("{}/changes", self.api_base)))
            .query(&[
                ("pageToken", page_token),
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
                ("fields", fields.as_str()),
            ]);
        let body: Resp = self.send_json(req).await?;
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
            .auth(
                self.http
                    .post(format!("{}/files", self.upload_base))
                    .query(&[("uploadType", "resumable"), ("supportsAllDrives", "true")]),
            )
            .header("Content-Type", "application/json; charset=UTF-8")
            .json(metadata)
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;
        let resp = self.check(resp).await?;
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
        let resp = self
            .auth(
                self.http
                    .patch(format!("{}/files/{file_id}", self.api_base)),
            )
            .query(&[("supportsAllDrives", "true")])
            .json(&serde_json::json!({ "trashed": true }))
            .send()
            .await
            .map_err(|e| DriveError::Http(e.to_string()))?;
        self.check(resp).await?;
        Ok(())
    }

    pub async fn update_metadata(
        &self,
        file_id: &str,
        patch: &serde_json::Value,
    ) -> Result<DriveFile> {
        let req = self
            .auth(
                self.http
                    .patch(format!("{}/files/{file_id}", self.api_base)),
            )
            .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
            .json(patch);
        let f: FileJson = self.send_json(req).await?;
        Ok(f.into())
    }

    pub async fn copy_file(&self, file_id: &str, name: &str, parent_id: &str) -> Result<DriveFile> {
        let req = self
            .auth(
                self.http
                    .post(format!("{}/files/{file_id}/copy", self.api_base)),
            )
            .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
            .json(&serde_json::json!({ "name": name, "parents": [parent_id] }));
        let f: FileJson = self.send_json(req).await?;
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
        let req = self
            .auth(
                self.http
                    .patch(format!("{}/files/{file_id}", self.api_base)),
            )
            .query(&q)
            .json(&body);
        let f: FileJson = self.send_json(req).await?;
        Ok(f.into())
    }
}
