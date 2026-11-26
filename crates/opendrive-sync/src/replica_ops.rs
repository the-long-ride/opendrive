//! Explorer listing and replica download (`sync_down_node`) operations.

use crate::engine::{sync_err, SyncEngine, SHARED_WITH_ME_DRIVE_ID};
use crate::path_time::sanitize_name;
use crate::stream_cache::STREAM_CHUNK_BYTES;
use crate::{NodeId, Result, SyncError};
use opendrive_store::{now_ms, Node, NodeState};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncSummary {
    pub downloaded_files: usize,
    pub uploaded_files: usize,
    pub cleared_files: usize,
    pub bytes_transferred: u64,
}

impl SyncEngine {
    pub fn list_cached_children(&self, folder_node: NodeId) -> Result<Vec<Node>> {
        let rows = self
            .store
            .lock()
            .list_children_sorted(folder_node as i64)
            .map_err(sync_err)?;
        Ok(rows.into_iter().filter(|c| !c.trashed).collect())
    }

    pub async fn list_explorer_nodes(
        &self,
        account_id: Uuid,
        folder_node: NodeId,
        force_refresh: bool,
    ) -> Result<Vec<Node>> {
        let (folder, children) = {
            let store = self.store.lock();
            let folder = store
                .get_node(folder_node as i64)
                .map_err(sync_err)?
                .ok_or(SyncError::NotFound)?;
            let children = store
                .list_children_sorted(folder_node as i64)
                .map_err(sync_err)?;
            (folder, children)
        };
        let is_root = folder.drive_id == "root";
        let is_swm = folder.drive_id == SHARED_WITH_ME_DRIVE_ID;
        let is_empty_root = is_root && children.is_empty();
        let must_hydrate = force_refresh
            || is_empty_root
            || ((folder.state == NodeState::Placeholder || is_swm) && children.is_empty());
        if must_hydrate {
            self.hydrate_folder(account_id, folder_node).await?;
        } else if (is_root || is_swm) && self.drive_client_for(account_id).is_ok() {
            if self.hydrate_folder(account_id, folder_node).await.is_err() {
                return Ok(children.into_iter().filter(|c| !c.trashed).collect());
            }
        } else {
            return Ok(children.into_iter().filter(|c| !c.trashed).collect());
        }
        {
            let store = self.store.lock();
            let _ = store.update_node_state(folder_node as i64, NodeState::Cached, None);
        }
        self.list_cached_children(folder_node)
    }

    pub async fn sync_down_node(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        local_account_root: Option<&Path>,
        cache_dir: &Path,
    ) -> Result<(SyncSummary, Option<PathBuf>)> {
        let node = {
            let store = self.store.lock();
            store
                .get_node(node_id as i64)
                .map_err(sync_err)?
                .ok_or(SyncError::NotFound)?
        };
        if node.is_folder() {
            let mut summary = SyncSummary::default();
            if let Some(root) = local_account_root {
                if let Some(rel) = self.node_rel_path(root_node, node_id) {
                    let _ = fs::create_dir_all(root.join(rel));
                }
            }
            self.sync_down_folder_rec(
                account_id,
                root_node,
                node_id,
                local_account_root,
                cache_dir,
                &mut summary,
            )
            .await?;
            let local_folder = local_account_root.and_then(|r| {
                self.node_rel_path(root_node, node_id)
                    .map(|rel| r.join(rel))
            });
            return Ok((summary, local_folder));
        }
        if node.is_native_doc() || node.kind == "shortcut" {
            return Ok((SyncSummary::default(), None));
        }
        self.sync_down_file(&node, root_node, local_account_root, cache_dir)
            .await
    }

    async fn sync_down_folder_rec(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        folder_id: NodeId,
        local_account_root: Option<&Path>,
        cache_dir: &Path,
        summary: &mut SyncSummary,
    ) -> Result<()> {
        let mut stack = vec![folder_id];
        while let Some(dir_id) = stack.pop() {
            let children = self.list_explorer_nodes(account_id, dir_id, false).await?;
            for child in children {
                if child.is_folder() {
                    if let Some(root) = local_account_root {
                        if let Some(rel) = self.node_rel_path(root_node, child.id as NodeId) {
                            let _ = fs::create_dir_all(root.join(rel));
                        }
                    }
                    stack.push(child.id as NodeId);
                } else if child.kind == "file" {
                    let (one, _) = self
                        .sync_down_file(&child, root_node, local_account_root, cache_dir)
                        .await?;
                    summary.downloaded_files += one.downloaded_files;
                    summary.bytes_transferred += one.bytes_transferred;
                }
            }
        }
        Ok(())
    }

    async fn sync_down_file(
        &self,
        node: &Node,
        root_node: NodeId,
        local_account_root: Option<&Path>,
        cache_dir: &Path,
    ) -> Result<(SyncSummary, Option<PathBuf>)> {
        let node_id = node.id as NodeId;
        let local_path = match local_account_root {
            Some(root) => {
                let rel = self
                    .node_rel_path(root_node, node_id)
                    .unwrap_or_else(|| PathBuf::from(sanitize_name(&node.display_name)));
                root.join(rel)
            }
            None => cache_dir
                .join("files")
                .join(format!("{node_id}-{}", sanitize_name(&node.display_name))),
        };
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).map_err(|e| SyncError::Io(e.to_string()))?;
        }
        let tmp_name = format!(
            ".{}.{}.tmp",
            local_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into()),
            Uuid::new_v4()
        );
        let tmp_path = local_path.with_file_name(tmp_name);
        let write_res = self
            .write_node_chunks_to_tmp(node_id, node.size.max(0) as u64, &tmp_path)
            .await;
        let bytes_written = match write_res {
            Ok(n) => n,
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(e);
            }
        };
        if let Err(e) = fs::rename(&tmp_path, &local_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(SyncError::Io(e.to_string()));
        }

        let mut updated = node.clone();
        updated.state = NodeState::Cached;
        updated.pinned = true;
        updated.last_access_ms = now_ms();
        let store = self.store.lock();
        store
            .set_cache_entry(
                node.id,
                &local_path.to_string_lossy(),
                bytes_written as i64,
                true,
            )
            .map_err(sync_err)?;
        store.upsert_node(&updated).map_err(sync_err)?;
        Ok((
            SyncSummary {
                downloaded_files: 1,
                bytes_transferred: bytes_written,
                ..SyncSummary::default()
            },
            Some(local_path),
        ))
    }

    async fn write_node_chunks_to_tmp(
        &self,
        node_id: NodeId,
        size: u64,
        tmp_path: &Path,
    ) -> Result<u64> {
        let mut f = File::create(tmp_path).map_err(|e| SyncError::Io(e.to_string()))?;
        if size == 0 {
            let data = self.read_cached_range(node_id, 0, 0, 0).await?;
            if !data.is_empty() {
                f.write_all(&data)
                    .map_err(|e| SyncError::Io(e.to_string()))?;
            }
            f.flush().map_err(|e| SyncError::Io(e.to_string()))?;
            return Ok(data.len() as u64);
        }
        let mut offset = 0u64;
        let mut total = 0u64;
        while offset < size {
            let req_len = (size - offset).min(STREAM_CHUNK_BYTES) as u32;
            let chunk = self.read_cached_range(node_id, offset, req_len, 2).await?;
            if chunk.is_empty() {
                break;
            }
            f.write_all(&chunk)
                .map_err(|e| SyncError::Io(e.to_string()))?;
            offset += chunk.len() as u64;
            total += chunk.len() as u64;
        }
        f.flush().map_err(|e| SyncError::Io(e.to_string()))?;
        Ok(total)
    }
}
