//! Cloud + local delete, rename, and move operations on `SyncEngine`.

use crate::engine::{sync_err, SyncEngine, SHARED_WITH_ME_DRIVE_ID};
use crate::replica_clear::remove_regular_non_reparse;
use crate::{NodeId, Result, SyncError};
use opendrive_store::Node;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub(crate) fn validate_item_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
    {
        return Err(SyncError::Other("invalid item name".into()));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn assert_mutable_node(node: &Node) -> Result<()> {
    if node.drive_id == "root" || node.drive_id == SHARED_WITH_ME_DRIVE_ID {
        return Err(SyncError::Other("cannot modify system root folder".into()));
    }
    Ok(())
}

pub(crate) fn assert_writable_folder(dest: &Node, account_id: Uuid) -> Result<()> {
    if dest.account_id != account_id || dest.trashed || !dest.is_folder() {
        return Err(SyncError::Other(
            "destination must be a folder in the same account".into(),
        ));
    }
    if dest.drive_id == SHARED_WITH_ME_DRIVE_ID {
        return Err(SyncError::Other(
            "cannot place items directly inside Shared with me".into(),
        ));
    }
    Ok(())
}

impl SyncEngine {
    pub(crate) fn load_account_node(&self, account_id: Uuid, node_id: NodeId) -> Result<Node> {
        let store = self.store.lock();
        let node = store
            .get_node(node_id as i64)
            .map_err(sync_err)?
            .ok_or(SyncError::NotFound)?;
        if node.account_id != account_id || node.trashed {
            return Err(SyncError::NotFound);
        }
        Ok(node)
    }

    pub(crate) fn is_descendant_or_self(&self, candidate: NodeId, ancestor: NodeId) -> bool {
        let store = self.store.lock();
        let mut cur = candidate as i64;
        let mut seen = HashSet::new();
        while cur > 0 && seen.insert(cur) {
            if cur == ancestor as i64 {
                return true;
            }
            cur = match store.get_node(cur).ok().flatten() {
                Some(n) => n.parent_id.unwrap_or(0),
                None => break,
            };
        }
        false
    }

    pub async fn delete_node_everywhere(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        local_account_root: Option<&Path>,
    ) -> Result<()> {
        let node = self.load_account_node(account_id, node_id)?;
        assert_mutable_node(&node)?;
        let local_target = local_account_root.and_then(|r| {
            self.node_rel_path(root_node, node_id)
                .map(|rel| r.join(rel))
        });
        let targets = self.collect_subtree_cache_entries(node.id)?;

        if !node.drive_id.is_empty() {
            self.drive_client_for(account_id)?
                .trash(&node.drive_id)
                .await
                .map_err(sync_err)?;
        }

        for (_, cp) in &targets {
            if let Some(p) = cp {
                remove_regular_non_reparse(Path::new(p));
            }
        }
        if let Some(ref p) = local_target {
            if p.is_dir() {
                let _ = fs::remove_dir_all(p);
            } else if fs::symlink_metadata(p).is_ok() {
                let _ = fs::remove_file(p);
            }
        }
        for (id, _) in targets {
            if let Some(ref staging) = *self.staging.lock() {
                let _ = staging.remove(id);
            }
            self.evict_node_stream_chunks(id as NodeId);
            let store = self.store.lock();
            let _ = store.mark_node_uploads_done(id);
            store.trash_node_and_clear(id).map_err(sync_err)?;
        }
        Ok(())
    }

    pub async fn rename_node_everywhere(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        new_name: &str,
        local_account_root: Option<&Path>,
    ) -> Result<Node> {
        let clean = validate_item_name(new_name)?;
        let mut node = self.load_account_node(account_id, node_id)?;
        assert_mutable_node(&node)?;
        if node.effective_name() == clean {
            return Ok(node);
        }
        self.ensure_no_sibling_conflict(
            node.parent_id.unwrap_or(root_node as i64) as NodeId,
            &clean,
            Some(node.id),
        )?;
        let old_local = local_account_root.and_then(|r| {
            self.node_rel_path(root_node, node_id)
                .map(|rel| r.join(rel))
        });

        if !node.drive_id.is_empty() {
            let df = self
                .drive_client_for(account_id)?
                .update_metadata(&node.drive_id, &serde_json::json!({ "name": clean }))
                .await
                .map_err(sync_err)?;
            node.head_revision = df.head_revision_id.or(node.head_revision);
        }
        node.name = clean.clone();
        node.display_name = clean;
        self.store.lock().upsert_node(&node).map_err(sync_err)?;

        let new_local = local_account_root.and_then(|r| {
            self.node_rel_path(root_node, node_id)
                .map(|rel| r.join(rel))
        });
        self.relocate_local_and_cache(old_local.as_deref(), new_local.as_deref())?;
        Ok(node)
    }

    pub async fn move_node_everywhere(
        &self,
        account_id: Uuid,
        root_node: NodeId,
        node_id: NodeId,
        dest_folder_node: NodeId,
        local_account_root: Option<&Path>,
    ) -> Result<Node> {
        let mut node = self.load_account_node(account_id, node_id)?;
        assert_mutable_node(&node)?;
        let dest = self.load_account_node(account_id, dest_folder_node)?;
        assert_writable_folder(&dest, account_id)?;
        if node.is_folder() && self.is_descendant_or_self(dest_folder_node, node_id) {
            return Err(SyncError::Other("cannot move a folder into itself".into()));
        }
        if node.parent_id == Some(dest_folder_node as i64) {
            return Ok(node);
        }
        self.ensure_no_sibling_conflict(dest_folder_node, node.effective_name(), Some(node.id))?;
        let old_local = local_account_root.and_then(|r| {
            self.node_rel_path(root_node, node_id)
                .map(|rel| r.join(rel))
        });
        let dest_drive = self.node_drive_id(dest_folder_node)?;
        let remove_parents = match node.parent_id {
            Some(pid) if pid > 0 => self
                .store
                .lock()
                .get_node(pid)
                .ok()
                .flatten()
                .map(|p| p.drive_id)
                .filter(|d| !d.is_empty() && d != SHARED_WITH_ME_DRIVE_ID)
                .unwrap_or_default(),
            _ => String::new(),
        };

        if !node.drive_id.is_empty() {
            let df = self
                .drive_client_for(account_id)?
                .move_file(&node.drive_id, &dest_drive, &remove_parents, None)
                .await
                .map_err(sync_err)?;
            node.head_revision = df.head_revision_id.or(node.head_revision);
        }
        node.parent_id = Some(dest_folder_node as i64);
        self.store.lock().upsert_node(&node).map_err(sync_err)?;

        let new_local = local_account_root.and_then(|r| {
            self.node_rel_path(root_node, node_id)
                .map(|rel| r.join(rel))
        });
        self.relocate_local_and_cache(old_local.as_deref(), new_local.as_deref())?;
        Ok(node)
    }

    fn ensure_no_sibling_conflict(
        &self,
        parent_node: NodeId,
        name: &str,
        ignore_id: Option<i64>,
    ) -> Result<()> {
        let children = self
            .store
            .lock()
            .list_children(parent_node as i64)
            .map_err(sync_err)?;
        if children.iter().any(|c| {
            Some(c.id) != ignore_id && !c.trashed && c.effective_name().eq_ignore_ascii_case(name)
        }) {
            return Err(SyncError::Other(format!(
                "an item named '{name}' already exists here"
            )));
        }
        Ok(())
    }

    fn collect_subtree_cache_entries(&self, root_id: i64) -> Result<Vec<(i64, Option<String>)>> {
        let store = self.store.lock();
        let mut stack = vec![root_id];
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            let cp = store.get_cache_path(id).map_err(sync_err)?;
            out.push((id, cp));
            for child in store.list_children(id).map_err(sync_err)? {
                stack.push(child.id);
            }
        }
        Ok(out)
    }

    fn relocate_local_and_cache(&self, old: Option<&Path>, new: Option<&Path>) -> Result<()> {
        let (Some(o), Some(n)) = (old, new) else {
            return Ok(());
        };
        if o == n {
            return Ok(());
        }
        if fs::symlink_metadata(o).is_ok() {
            if let Some(parent) = n.parent() {
                fs::create_dir_all(parent).map_err(|e| SyncError::Io(e.to_string()))?;
            }
            fs::rename(o, n).map_err(|e| SyncError::Io(e.to_string()))?;
            let _ = self
                .store
                .lock()
                .relocate_cache_prefix(&o.to_string_lossy(), &n.to_string_lossy());
        }
        Ok(())
    }
}
