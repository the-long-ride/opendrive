//! Background upload queue worker.

use crate::engine::SyncEngine;
use crate::Result;
use opendrive_drive::DriveClient;
use opendrive_store::NodeState;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

impl SyncEngine {
    pub async fn process_one_upload(&self) -> Result<bool> {
        let row = {
            let store = self.store.lock();
            store
                .next_upload()
                .map_err(|e| crate::SyncError::Other(e.to_string()))?
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
                .map_err(|e| crate::SyncError::Other(e.to_string()))?
                .ok_or(crate::SyncError::NotFound)?;
            let parent_drive = n
                .parent_id
                .and_then(|pid| store.get_node(pid).ok().flatten())
                .map(|p| p.drive_id)
                .unwrap_or_else(|| "root".into());
            (
                n.account_id,
                n.drive_id.clone(),
                n.name.clone(),
                parent_drive,
                n.size as u64,
            )
        };

        let token = {
            let clients = self.clients.lock();
            clients
                .get(&account_id)
                .map(|c| c.access_token.clone())
                .ok_or_else(|| crate::SyncError::Other("no drive client".into()))?
        };
        let client = DriveClient::new(token);

        let body = {
            let staging = self.staging.lock();
            staging
                .as_ref()
                .and_then(|s| s.read_all(row.node_id).ok())
                .unwrap_or_default()
        };

        let result = match row.op.as_str() {
            "create" | "update" => {
                self.upload_file(
                    &client,
                    &row,
                    &drive_id,
                    &name,
                    &parent_drive,
                    size.max(body.len() as u64),
                    &body,
                )
                .await
            }
            "rename" => {
                if drive_id.is_empty() {
                    Ok(())
                } else {
                    client
                        .update_metadata(&drive_id, &serde_json::json!({ "name": name }))
                        .await
                        .map(|_| ())
                        .map_err(|e| crate::SyncError::Other(e.to_string()))
                }
            }
            "trash" => {
                if drive_id.is_empty() {
                    Ok(())
                } else {
                    client
                        .trash(&drive_id)
                        .await
                        .map_err(|e| crate::SyncError::Other(e.to_string()))
                }
            }
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                let store = self.store.lock();
                let _ = store.update_upload(row.op_id, "done", row.attempts + 1, now_ms(), None);
                if let Ok(Some(mut n)) = store.get_node(row.node_id) {
                    n.state = NodeState::Cached;
                    let _ = store.upsert_node(&n);
                }
                info!(op_id = row.op_id, op = %row.op, "queue op done");
                Ok(true)
            }
            Err(e) => {
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
        row: &opendrive_store::UploadRow,
        drive_id: &str,
        name: &str,
        parent_drive: &str,
        total: u64,
        data: &[u8],
    ) -> Result<()> {
        let uri = if let Some(ref u) = row.resumable_uri {
            u.clone()
        } else {
            let meta = if drive_id.is_empty() {
                serde_json::json!({ "name": name, "parents": [parent_drive] })
            } else {
                serde_json::json!({ "name": name })
            };
            let u = client
                .create_resumable(&meta)
                .await
                .map_err(|e| crate::SyncError::Other(e.to_string()))?;
            let store = self.store.lock();
            let _ = store.update_upload(row.op_id, "in_progress", row.attempts, now_ms(), Some(&u));
            u
        };

        // Chunk at 256 KiB
        const CHUNK: usize = 256 * 1024;
        let total = total.max(data.len() as u64);
        if data.is_empty() && total == 0 {
            let _ = client
                .upload_chunk(&uri, 0, 0, &[])
                .await
                .map_err(|e| crate::SyncError::Other(e.to_string()))?;
            return Ok(());
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
                .map_err(|e| crate::SyncError::Other(e.to_string()))?;
            offset += chunk.len() as u64;
            if done.is_some() {
                break;
            }
            if chunk.is_empty() {
                break;
            }
        }
        Ok(())
    }

    pub fn spawn_worker(self: &Arc<Self>, interval: Duration) {
        let eng = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                match eng.process_one_upload().await {
                    Ok(true) => continue,
                    Ok(false) => tokio::time::sleep(interval).await,
                    Err(_) => tokio::time::sleep(interval).await,
                }
            }
        });
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
