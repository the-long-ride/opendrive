//! Cloud + local copy operations on `SyncEngine`.

use crate::engine::{sync_err, SyncEngine};
use crate::file_ops::{assert_mutable_node, assert_writable_folder};
use crate::path_time::sanitize_name;
use crate::replica_clear::pick_upload_source;
use crate::replica_upload::node_from_drive_file;
use crate::{is_reparse_point, NodeId, Result, SyncError};
use opendrive_drive::DriveClient;
use opendrive_store::{Node, NodeState};
use std::fs;
use std::path::Path;
use uuid::Uuid;

impl SyncEngine {
    pub async fn copy_node_everywhere(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        dest_folder_node: NodeId,
        local_account_root: Option<&Path>,
        cache_dir: &Path,
    ) -> Result<Node> {
        let root_src = self.load_account_node(account_id, node_id)?;
        assert_mutable_node(&root_src)?;
        let dest = self.load_account_node(account_id, dest_folder_node)?;
        assert_writable_folder(&dest, account_id)?;
        if root_src.is_folder() && self.is_descendant_or_self(dest_folder_node, node_id) {
            return Err(SyncError::Other("cannot copy a folder into itself".into()));
        }
        let client = self.drive_client_for(account_id)?;
        let top_name = self.disambiguate_copy_name(
            dest_folder_node,
            root_src.effective_name(),
            root_src.is_folder(),
        )?;

        let mut created_top: Option<Node> = None;
        let mut stack = vec![(root_src, dest_folder_node, top_name)];
        while let Some((src, cur_dest, target_name)) = stack.pop() {
            if src.is_folder() {
                let children = self
                    .list_explorer_nodes(account_id, src.id as NodeId, false)
                    .await?;
                let folder_node = self
                    .create_copy_folder(
                        account_id,
                        &client,
                        cur_dest,
                        &target_name,
                        src.state,
                        root_node,
                        local_account_root,
                    )
                    .await?;
                let new_fid = folder_node.id as NodeId;
                if created_top.is_none() {
                    created_top = Some(folder_node);
                }
                for child in children.into_iter().rev() {
                    let cname = child.effective_name().to_string();
                    stack.push((child, new_fid, cname));
                }
            } else {
                let copied = self
                    .copy_file_node(
                        account_id,
                        &client,
                        &src,
                        cur_dest,
                        &target_name,
                        root_node,
                        local_account_root,
                        cache_dir,
                    )
                    .await?;
                if created_top.is_none() {
                    created_top = Some(copied);
                }
            }
        }
        created_top.ok_or(SyncError::NotFound)
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_copy_folder(
        &self,
        account_id: Uuid,
        client: &DriveClient,
        parent_node: NodeId,
        name: &str,
        src_state: NodeState,
        root_node: NodeId,
        local_account_root: Option<&Path>,
    ) -> Result<Node> {
        let state = if local_account_root.is_some() {
            NodeState::Cached
        } else {
            src_state
        };
        let node = self
            .create_remote_folder_node(account_id, client, parent_node, name, state)
            .await?;
        if let Some(root) = local_account_root {
            if let Some(rel) = self.node_rel_path(root_node, node.id as NodeId) {
                let _ = fs::create_dir_all(root.join(rel));
            }
        }
        Ok(node)
    }

    #[allow(clippy::too_many_arguments)]
    async fn copy_file_node(
        &self,
        account_id: Uuid,
        client: &DriveClient,
        src: &Node,
        dest_folder: NodeId,
        target_name: &str,
        root_node: NodeId,
        local_account_root: Option<&Path>,
        cache_dir: &Path,
    ) -> Result<Node> {
        let old_local = local_account_root.and_then(|r| {
            self.node_rel_path(root_node, src.id as NodeId)
                .map(|rel| r.join(rel))
        });
        let old_cache = self.store.lock().get_cache_path(src.id).map_err(sync_err)?;
        let staged = self
            .staging
            .lock()
            .as_ref()
            .and_then(|s| s.read_all(src.id).ok())
            .filter(|b| !b.is_empty());
        let dest_drive = self.node_drive_id(dest_folder)?;
        let df = if let Some(bytes) = pick_upload_source(
            src,
            staged,
            old_local.as_deref(),
            old_cache.as_deref().map(Path::new),
        )? {
            self.upload_bytes_verified(client, "", target_name, &dest_drive, &bytes)
                .await?
        } else {
            client
                .copy_file(&src.drive_id, target_name, &dest_drive)
                .await
                .map_err(sync_err)?
        };
        let new_id = self.store.lock().next_id().map_err(sync_err)?;
        let mut node = node_from_drive_file(
            new_id,
            account_id,
            dest_folder as i64,
            &src.kind,
            &df,
            src.size.max(0) as u64,
        );
        let src_disk = [old_local.as_deref(), old_cache.as_deref().map(Path::new)]
            .into_iter()
            .flatten()
            .find(|p| p.is_file() && !is_reparse_point(p));
        if let Some(disk) = src_disk {
            let dest_disk = match local_account_root {
                Some(r) => {
                    let rel = self
                        .node_rel_path(root_node, dest_folder)
                        .unwrap_or_default()
                        .join(sanitize_name(target_name));
                    r.join(rel)
                }
                None => cache_dir
                    .join("files")
                    .join(format!("{new_id}-{}", sanitize_name(target_name))),
            };
            if let Some(parent) = dest_disk.parent() {
                fs::create_dir_all(parent).map_err(|e| SyncError::Io(e.to_string()))?;
            }
            let copied_len =
                fs::copy(disk, &dest_disk).map_err(|e| SyncError::Io(e.to_string()))?;
            node.state = NodeState::Cached;
            node.pinned = src.pinned;
            let store = self.store.lock();
            store
                .set_cache_entry(
                    new_id,
                    &dest_disk.to_string_lossy(),
                    copied_len as i64,
                    src.pinned,
                )
                .map_err(sync_err)?;
            store.upsert_node(&node).map_err(sync_err)?;
        } else {
            node.state = NodeState::Placeholder;
            node.pinned = false;
            self.store.lock().upsert_node(&node).map_err(sync_err)?;
        }
        Ok(node)
    }

    pub(crate) fn disambiguate_copy_name(
        &self,
        parent_node: NodeId,
        desired: &str,
        is_folder: bool,
    ) -> Result<String> {
        let children = self
            .store
            .lock()
            .list_children(parent_node as i64)
            .map_err(sync_err)?;
        let taken = |cand: &str| {
            children
                .iter()
                .any(|c| !c.trashed && c.effective_name().eq_ignore_ascii_case(cand))
        };
        if !taken(desired) {
            return Ok(desired.to_string());
        }
        let (stem, ext) = if !is_folder {
            match desired.rsplit_once('.') {
                Some((s, e)) if !s.is_empty() && !e.is_empty() => (s, format!(".{e}")),
                _ => (desired, String::new()),
            }
        } else {
            (desired, String::new())
        };
        let first = format!("{stem} (copy){ext}");
        if !taken(&first) {
            return Ok(first);
        }
        let mut idx = 2usize;
        loop {
            let cand = format!("{stem} (copy {idx}){ext}");
            if !taken(&cand) {
                return Ok(cand);
            }
            idx += 1;
        }
    }
}
