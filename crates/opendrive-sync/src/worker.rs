//! Background upload queue worker.

use crate::engine::SyncEngine;
use crate::{Result, SyncError};
use opendrive_drive::{DriveClient, DriveFile};
use opendrive_store::{NodeState, UploadRow};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

impl SyncEngine {
    pub async fn process_one_upload(&self) -> Result<bool> {
        let row = {
            let store = self.store.lock();
            store
                .next_upload()
                .map_err(|e| SyncError::Other(e.to_string()))?
        };
        let Some(row) = row else {
            return Ok(false);
        };

        {
            let store = self.store.lock();
            let _ = store.update_upload(
                row.op_id,
                "in_progress",
                row.attempts + 1,
                now_ms(),
                row.resumable_uri.as_deref(),
            );
        }

        let (account_id, drive_id, name, parent_drive, size) = {
            let store = self.store.lock();
            let n = store
                .get_node(row.node_id)
                .map_err(|e| SyncError::Other(e.to_string()))?
                .ok_or(SyncError::NotFound)?;
            let parent_drive = n
                .parent_id
                .and_then(|pid| store.get_node(pid).ok().flatten())
                .map(|p| p.drive_id)
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "root".into());
            (
                n.account_id,
                n.drive_id.clone(),
                n.name.clone(),
                parent_drive,
                n.size as u64,
            )
        };

        let client = self.drive_client_for(account_id)?;
        let body = {
            let staging = self.staging.lock();
            staging
                .as_ref()
                .and_then(|s| s.read_all(row.node_id).ok())
                .unwrap_or_default()
        };

        let result: Result<Option<DriveFile>> = match row.op.as_str() {
            "create" | "update" => {
                let target = UploadTarget {
                    drive_id: &drive_id,
                    name: &name,
                    parent_drive: &parent_drive,
                    total: size.max(body.len() as u64),
                };
                self.upload_file(&client, &row, &target, &body).await
            }
            "rename" => {
                if drive_id.is_empty() {
                    Ok(None)
                } else {
                    client
                        .update_metadata(&drive_id, &serde_json::json!({ "name": name }))
                        .await
                        .map(Some)
                        .map_err(|e| SyncError::Other(e.to_string()))
                }
            }
            "trash" => {
                if drive_id.is_empty() {
                    Ok(None)
                } else {
                    client
                        .trash(&drive_id)
                        .await
                        .map(|()| None)
                        .map_err(|e| SyncError::Other(e.to_string()))
                }
            }
            _ => Ok(None),
        };

        match result {
            Ok(uploaded) => {
                self.transfers.fetch_add(1, Ordering::Relaxed);
                let store = self.store.lock();
                let _ = store.update_upload(row.op_id, "done", row.attempts + 1, now_ms(), None);
                if let Ok(Some(mut n)) = store.get_node(row.node_id) {
                    if let Some(df) = uploaded {
                        n.drive_id = df.id;
                        n.head_revision = df.head_revision_id;
                        if let Some(sz) = df.size {
                            n.size = sz as i64;
                        }
                    }
                    n.state = NodeState::Cached;
                    let _ = store.upsert_node(&n);
                }
                info!(op_id = row.op_id, op = %row.op, "queue op done");
                Ok(true)
            }
            Err(e) => {
                self.errors.fetch_add(1, Ordering::Relaxed);
                warn!(op_id = row.op_id, error = %e, "queue op failed");
                let backoff = (1000i64 * (1 << row.attempts.min(6))).min(300_000);
                let store = self.store.lock();
                let _ = store.update_upload(
                    row.op_id,
                    "pending",
                    row.attempts + 1,
                    now_ms() + backoff,
                    row.resumable_uri.as_deref(),
                );
                Err(e)
            }
        }
    }

    async fn upload_file(
        &self,
        client: &DriveClient,
        row: &UploadRow,
        target: &UploadTarget<'_>,
        data: &[u8],
    ) -> Result<Option<DriveFile>> {
        let uri = if let Some(ref u) = row.resumable_uri {
            u.clone()
        } else {
            let meta = if target.drive_id.is_empty() {
                serde_json::json!({ "name": target.name, "parents": [target.parent_drive] })
            } else {
                serde_json::json!({ "id": target.drive_id, "name": target.name })
            };
            let u = client
                .create_resumable(&meta)
                .await
                .map_err(|e| SyncError::Other(e.to_string()))?;
            let store = self.store.lock();
            let _ = store.update_upload(row.op_id, "in_progress", row.attempts, now_ms(), Some(&u));
            u
        };

        const CHUNK: usize = 256 * 1024;
        let total = target.total.max(data.len() as u64);
        if data.is_empty() && total == 0 {
            return client
                .upload_chunk(&uri, 0, 0, &[])
                .await
                .map_err(|e| SyncError::Other(e.to_string()));
        }
        let mut offset = 0u64;
        while offset < total {
            let start = offset as usize;
            let end = ((offset as usize) + CHUNK).min(data.len());
            let chunk = if start < data.len() {
                &data[start..end]
            } else {
                &[][..]
            };
            let done = client
                .upload_chunk(&uri, offset, total, chunk)
                .await
                .map_err(|e| SyncError::Other(e.to_string()))?;
            offset += chunk.len() as u64;
            if let Some(df) = done {
                return Ok(Some(df));
            }
            if chunk.is_empty() {
                break;
            }
        }
        Ok(None)
    }

    pub fn spawn_worker(self: &Arc<Self>, interval: Duration) {
        let eng = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                match eng.process_one_upload().await {
                    Ok(true) => continue,
                    Ok(false) | Err(_) => tokio::time::sleep(interval).await,
                }
            }
        });
    }
}

struct UploadTarget<'a> {
    drive_id: &'a str,
    name: &'a str,
    parent_drive: &'a str,
    total: u64,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
