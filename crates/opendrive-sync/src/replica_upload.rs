//! Local-to-Drive upload helpers and account queue flushing.

use crate::engine::{sync_err, SyncEngine};
use crate::path_time::parse_rfc3339_ms;
use crate::replica_ops::SyncSummary;
use crate::{NodeId, Result, SyncError};
use opendrive_drive::{DriveClient, DriveFile};
use opendrive_store::{now_ms, Node, NodeState};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use uuid::Uuid;

pub(crate) const UPLOAD_CHUNK_BYTES: usize = 256 * 1024;
pub(crate) const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

impl SyncEngine {
    pub async fn flush_account_uploads(&self, account_id: Uuid) -> Result<usize> {
        let mut flushed = 0usize;
        loop {
            let next = self
                .store
                .lock()
                .next_upload_for_account(account_id)
                .map_err(sync_err)?;
            let Some(row) = next else {
                return Ok(flushed);
            };
            self.process_upload_row(row).await?;
            flushed += 1;
        }
    }

    pub async fn sync_up_local_path(
        &self,
        account_id: Uuid,
        parent_node: NodeId,
        source_path: &Path,
        local_account_root: Option<&Path>,
    ) -> Result<SyncSummary> {
        if !source_path.exists() {
            return Err(SyncError::Io(format!(
                "source path does not exist: {}",
                source_path.display()
            )));
        }
        let client = self.drive_client_for(account_id)?;
        let mut summary = SyncSummary::default();
        let mut stack = vec![(parent_node, source_path.to_path_buf())];
        while let Some((cur_parent, cur_path)) = stack.pop() {
            let name = cur_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| SyncError::Io("invalid source file name".into()))?;
            if cur_path.is_dir() {
                let folder_node = self
                    .ensure_remote_folder(account_id, &client, cur_parent, &name)
                    .await?;
                let mut entries: Vec<PathBuf> = fs::read_dir(&cur_path)
                    .map_err(|e| SyncError::Io(e.to_string()))?
                    .filter_map(|e| e.ok().map(|de| de.path()))
                    .collect();
                entries.sort();
                for child in entries.into_iter().rev() {
                    stack.push((folder_node, child));
                }
            } else {
                let data = fs::read(&cur_path).map_err(|e| SyncError::Io(e.to_string()))?;
                let total = data.len() as u64;
                let existing = self.find_child_by_name(cur_parent, &name)?;
                let parent_drive = self.node_drive_id(cur_parent)?;
                let cur_drive = existing.as_ref().map(|n| n.drive_id.as_str()).unwrap_or("");
                let df = self
                    .upload_bytes_verified(&client, cur_drive, &name, &parent_drive, &data)
                    .await?;
                let node_id = match existing {
                    Some(n) => n.id,
                    None => self.store.lock().next_id().map_err(sync_err)?,
                };
                let upserted = node_from_drive_file(
                    node_id,
                    account_id,
                    cur_parent as i64,
                    "file",
                    &df,
                    total,
                );
                let store = self.store.lock();
                store.upsert_node(&upserted).map_err(sync_err)?;
                let _ = store.mark_node_uploads_done(node_id);
                if let Some(root) = local_account_root {
                    if cur_path.starts_with(root) {
                        let _ = store.set_cache_entry(
                            node_id,
                            &cur_path.to_string_lossy(),
                            total as i64,
                            true,
                        );
                    }
                }
                drop(store);
                if let Some(ref staging) = *self.staging.lock() {
                    let _ = staging.remove(node_id);
                }
                summary.uploaded_files += 1;
                summary.bytes_transferred += total;
            }
        }
        summary.uploaded_files += self.flush_account_uploads(account_id).await?;
        Ok(summary)
    }

    async fn ensure_remote_folder(
        &self,
        account_id: Uuid,
        client: &DriveClient,
        parent_node: NodeId,
        name: &str,
    ) -> Result<NodeId> {
        if let Some(existing) = self.find_child_by_name(parent_node, name)? {
            if existing.is_folder() && !existing.drive_id.is_empty() {
                return Ok(existing.id as NodeId);
            }
        }
        Ok(self
            .create_remote_folder_node(account_id, client, parent_node, name, NodeState::Cached)
            .await?
            .id as NodeId)
    }

    pub(crate) async fn create_remote_folder_node(
        &self,
        account_id: Uuid,
        client: &DriveClient,
        parent_node: NodeId,
        name: &str,
        state: NodeState,
    ) -> Result<Node> {
        let parent_drive = self.node_drive_id(parent_node)?;
        let meta = serde_json::json!({
            "name": name,
            "parents": [parent_drive],
            "mimeType": FOLDER_MIME,
        });
        let uri = client.create_resumable(&meta).await.map_err(sync_err)?;
        let df = client
            .upload_chunk(&uri, 0, 0, &[])
            .await
            .map_err(sync_err)?
            .ok_or_else(|| SyncError::Other("folder creation returned no metadata".into()))?;
        let id = self.store.lock().next_id().map_err(sync_err)?;
        let mut node = node_from_drive_file(id, account_id, parent_node as i64, "folder", &df, 0);
        node.state = state;
        self.store.lock().upsert_node(&node).map_err(sync_err)?;
        Ok(node)
    }

    pub(crate) async fn upload_bytes_verified(
        &self,
        client: &DriveClient,
        drive_id: &str,
        name: &str,
        parent_drive: &str,
        data: &[u8],
    ) -> Result<DriveFile> {
        let meta = if drive_id.is_empty() {
            serde_json::json!({ "name": name, "parents": [parent_drive] })
        } else {
            serde_json::json!({ "id": drive_id, "name": name })
        };
        let uri = client.create_resumable(&meta).await.map_err(sync_err)?;
        let total = data.len() as u64;
        let df = if total == 0 {
            client
                .upload_chunk(&uri, 0, 0, &[])
                .await
                .map_err(sync_err)?
        } else {
            let mut out = None;
            let mut offset = 0usize;
            while offset < data.len() {
                let end = (offset + UPLOAD_CHUNK_BYTES).min(data.len());
                let chunk = &data[offset..end];
                self.limiter.take(chunk.len() as u64).await;
                if let Some(f) = client
                    .upload_chunk(&uri, offset as u64, total, chunk)
                    .await
                    .map_err(sync_err)?
                {
                    out = Some(f);
                    break;
                }
                offset = end;
            }
            out
        };
        let df =
            df.ok_or_else(|| SyncError::Other("upload completed without file metadata".into()))?;
        if df.id.is_empty() {
            return Err(SyncError::Other(
                "remote drive returned empty file id".into(),
            ));
        }
        self.transfers.fetch_add(1, Ordering::Relaxed);
        Ok(df)
    }

    pub(crate) fn find_child_by_name(
        &self,
        parent_node: NodeId,
        name: &str,
    ) -> Result<Option<Node>> {
        let children = self
            .store
            .lock()
            .list_children(parent_node as i64)
            .map_err(sync_err)?;
        Ok(children
            .into_iter()
            .find(|c| !c.trashed && (c.display_name == name || c.name == name)))
    }

    pub(crate) fn node_drive_id(&self, node_id: NodeId) -> Result<String> {
        let store = self.store.lock();
        let node = store
            .get_node(node_id as i64)
            .map_err(sync_err)?
            .ok_or(SyncError::NotFound)?;
        if node.drive_id.is_empty() {
            Ok("root".into())
        } else {
            Ok(node.drive_id)
        }
    }
}

pub(crate) fn node_from_drive_file(
    id: i64,
    account_id: Uuid,
    parent_id: i64,
    kind: &str,
    df: &DriveFile,
    fallback_size: u64,
) -> Node {
    Node {
        id,
        account_id,
        drive_id: df.id.clone(),
        parent_id: Some(parent_id),
        name: df.name.clone(),
        display_name: df.name.clone(),
        kind: kind.into(),
        mime: Some(df.mime_type.clone()),
        size: df.size.unwrap_or(fallback_size) as i64,
        md5: df.md5.clone(),
        modified_ms: df
            .modified_time
            .as_deref()
            .map(parse_rfc3339_ms)
            .unwrap_or(0),
        head_revision: df.head_revision_id.clone(),
        trashed: df.trashed,
        shared_drive_id: None,
        state: NodeState::Cached,
        pinned: false,
        last_access_ms: now_ms(),
    }
}
