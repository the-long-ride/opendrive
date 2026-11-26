//! Verified local replica clearing (`sync_up_and_clear_local`) honoring Safety Invariant #3.

use crate::engine::{sync_err, SyncEngine};
use crate::replica_ops::SyncSummary;
use crate::replica_upload::node_from_drive_file;
use crate::{NodeId, Result, SyncError};
use opendrive_store::{Node, NodeState};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

#[derive(Default)]
struct ClearTargets {
    files: Vec<Node>,
    folders: Vec<i64>,
}

type VerifiedTarget = (i64, Option<PathBuf>, Option<String>);

impl SyncEngine {
    pub async fn sync_up_node(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        local_account_root: Option<&Path>,
    ) -> Result<SyncSummary> {
        let mut summary = SyncSummary::default();
        summary.uploaded_files += self.flush_account_uploads(account_id).await?;
        let targets = self.collect_clear_targets(node_id)?;
        let (phase1, _) = self
            .sync_up_targets_verified(account_id, root_node, targets.files, local_account_root)
            .await?;
        summary.uploaded_files += phase1.uploaded_files;
        summary.bytes_transferred += phase1.bytes_transferred;
        Ok(summary)
    }

    pub async fn sync_up_and_clear_local(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        local_account_root: Option<&Path>,
    ) -> Result<SyncSummary> {
        let mut summary = SyncSummary::default();
        summary.uploaded_files += self.flush_account_uploads(account_id).await?;
        let targets = self.collect_clear_targets(node_id)?;
        let (phase1, verified) = self
            .sync_up_targets_verified(account_id, root_node, targets.files, local_account_root)
            .await?;
        summary.uploaded_files += phase1.uploaded_files;
        summary.bytes_transferred += phase1.bytes_transferred;

        // Phase 2: only after remote Drive presence is verified, remove local replicas & state.
        for (id, local_replica, cache_path) in verified {
            if let Some(ref p) = local_replica {
                remove_regular_non_reparse(p);
            }
            if let Some(ref cp) = cache_path {
                remove_regular_non_reparse(Path::new(cp));
            }
            if let Some(ref staging) = *self.staging.lock() {
                let _ = staging.remove(id);
            }
            {
                let store = self.store.lock();
                store.clear_node_cache_state(id).map_err(sync_err)?;
                let _ = store.mark_node_uploads_done(id);
            }
            self.evict_node_stream_chunks(id as NodeId);
            summary.cleared_files += 1;
        }
        for folder_id in targets.folders {
            let _ =
                self.store
                    .lock()
                    .update_node_state(folder_id, NodeState::Placeholder, Some(false));
        }
        Ok(summary)
    }

    async fn sync_up_targets_verified(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        files: Vec<Node>,
        local_account_root: Option<&Path>,
    ) -> Result<(SyncSummary, Vec<VerifiedTarget>)> {
        let client = self.drive_client_for(account_id)?;
        let mut summary = SyncSummary::default();
        let mut verified = Vec::with_capacity(files.len());
        for file_node in files {
            let local_replica = local_account_root.and_then(|r| {
                self.node_rel_path(root_node, file_node.id as NodeId)
                    .map(|rel| r.join(rel))
            });
            let cache_path = self
                .store
                .lock()
                .get_cache_path(file_node.id)
                .map_err(sync_err)?;
            let staged_bytes = self
                .staging
                .lock()
                .as_ref()
                .and_then(|s| s.read_all(file_node.id).ok())
                .filter(|b| !b.is_empty());
            if let Some(data) = pick_upload_source(
                &file_node,
                staged_bytes,
                local_replica.as_deref(),
                cache_path.as_deref().map(Path::new),
            )? {
                let parent_drive = file_node
                    .parent_id
                    .and_then(|pid| self.node_drive_id(pid as NodeId).ok())
                    .unwrap_or_else(|| "root".into());
                let df = self
                    .upload_bytes_verified(
                        &client,
                        &file_node.drive_id,
                        &file_node.name,
                        &parent_drive,
                        &data,
                    )
                    .await?;
                let total = data.len() as u64;
                let mut updated = node_from_drive_file(
                    file_node.id,
                    account_id,
                    file_node.parent_id.unwrap_or(0),
                    &file_node.kind,
                    &df,
                    total,
                );
                updated.state = NodeState::Cached;
                self.store.lock().upsert_node(&updated).map_err(sync_err)?;
                summary.uploaded_files += 1;
                summary.bytes_transferred += total;
            } else if file_node.drive_id.is_empty() {
                return Err(SyncError::Other(
                    "cannot clear local replica without verified remote drive_id".into(),
                ));
            }
            verified.push((file_node.id, local_replica, cache_path));
        }
        Ok((summary, verified))
    }

    fn collect_clear_targets(&self, node_id: NodeId) -> Result<ClearTargets> {
        let store = self.store.lock();
        let root = store
            .get_node(node_id as i64)
            .map_err(sync_err)?
            .ok_or(SyncError::NotFound)?;
        let mut out = ClearTargets::default();
        if !root.is_folder() {
            out.files.push(root);
            return Ok(out);
        }
        let mut stack = vec![root.id];
        while let Some(fid) = stack.pop() {
            out.folders.push(fid);
            for child in store.list_children(fid).map_err(sync_err)? {
                if child.trashed {
                    continue;
                }
                if child.is_folder() {
                    stack.push(child.id);
                } else if child.kind == "file" {
                    out.files.push(child);
                }
            }
        }
        Ok(out)
    }
}

pub(crate) fn pick_upload_source(
    node: &Node,
    staged: Option<Vec<u8>>,
    local_replica: Option<&Path>,
    cache_path: Option<&Path>,
) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = staged {
        return Ok(Some(bytes));
    }
    let disk_path = local_replica
        .filter(|p| p.is_file() && !is_dehydrated_placeholder(p))
        .or_else(|| cache_path.filter(|p| p.is_file() && !is_dehydrated_placeholder(p)));
    let disk_meta = disk_path.and_then(|p| fs::metadata(p).ok());
    let disk_modified_or_unverified = match (disk_path, disk_meta) {
        (Some(_), Some(m)) if m.len() > 0 => {
            let mtime_ms = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            node.head_revision.is_none()
                || m.len() as i64 != node.size
                || (node.last_access_ms > 0 && mtime_ms > node.last_access_ms + 1000)
        }
        _ => false,
    };
    if node.state == NodeState::Dirty || node.drive_id.is_empty() || disk_modified_or_unverified {
        if let Some(p) = disk_path {
            if let Ok(bytes) = fs::read(p) {
                return Ok(Some(bytes));
            }
        }
        if node.drive_id.is_empty() && node.size == 0 {
            return Ok(Some(Vec::new()));
        }
        return Err(SyncError::Other(
            "dirty node has no readable local bytes to sync up".into(),
        ));
    }
    Ok(None)
}

pub(crate) fn remove_regular_non_reparse(path: &Path) {
    if path.is_file() && !is_reparse_point(path) {
        let _ = fs::remove_file(path);
    }
}

fn file_attrs_has_any(path: &Path, mask: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        fs::symlink_metadata(path).is_ok_and(|m| (m.file_attributes() & mask) != 0)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, mask);
        false
    }
}

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

fn is_dehydrated_placeholder(path: &Path) -> bool {
    file_attrs_has_any(
        path,
        FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
    )
}

pub fn is_reparse_point(path: &Path) -> bool {
    file_attrs_has_any(path, FILE_ATTRIBUTE_REPARSE_POINT)
}
