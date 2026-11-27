//! `VfsEngine` trait implementation for `SyncEngine`.

use crate::engine::{sync_err, to_attr, SyncEngine};
use crate::{
    DirPage, EngineEvent, EngineStats, Kind, NodeAttr, NodeId, PageToken, Result, SyncError,
    VfsEngine, WriteHandle,
};
use async_trait::async_trait;
use bytes::Bytes;
use opendrive_store::{Node, NodeState};
use std::ffi::OsStr;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use uuid::Uuid;

#[async_trait]
impl VfsEngine for SyncEngine {
    async fn list(&self, node: NodeId, _page: Option<PageToken>) -> Result<DirPage> {
        let mut children = self
            .store
            .lock()
            .list_children(node as i64)
            .map_err(sync_err)?;
        if node > 0 {
            let maybe_folder = self.store.lock().get_node(node as i64).map_err(sync_err)?;
            if let Some(mut folder) = maybe_folder {
                let should_hydrate = folder.kind == "folder"
                    && !folder.drive_id.is_empty()
                    && (folder.state == NodeState::Placeholder
                        || children.is_empty()
                        || folder.drive_id == crate::engine::SHARED_WITH_ME_DRIVE_ID);
                if should_hydrate {
                    match self.hydrate_folder(folder.account_id, node).await {
                        Ok(()) => {
                            folder.state = NodeState::Cached;
                            let _ = self.store.lock().upsert_node(&folder);
                            children = self
                                .store
                                .lock()
                                .list_children(node as i64)
                                .map_err(sync_err)?;
                        }
                        Err(err) if children.is_empty() => return Err(err),
                        Err(err) => {
                            tracing::warn!(
                                node,
                                error = %err,
                                "using cached children after folder hydrate failure"
                            );
                        }
                    }
                }
            }
        }
        Ok(DirPage {
            entries: children.iter().map(to_attr).collect(),
            next_page: None,
        })
    }

    async fn lookup(&self, parent: NodeId, name: &OsStr) -> Result<Option<NodeAttr>> {
        let name = name.to_string_lossy();
        let children = self
            .store
            .lock()
            .list_children(parent as i64)
            .map_err(sync_err)?;
        Ok(children
            .iter()
            .find(|n| n.display_name == name || n.name == name)
            .map(to_attr))
    }

    async fn stat(&self, node: NodeId) -> Result<NodeAttr> {
        self.store
            .lock()
            .get_node(node as i64)
            .map_err(sync_err)?
            .map(|n| to_attr(&n))
            .ok_or(SyncError::NotFound)
    }

    async fn read(&self, node: NodeId, offset: u64, len: u32) -> Result<Bytes> {
        let staged_slice = {
            let guard = self.staging.lock();
            guard
                .as_ref()
                .and_then(|s| s.read_all(node as i64).ok())
                .and_then(|data| {
                    let start = offset as usize;
                    if !data.is_empty() && start < data.len() {
                        let end = (start + len as usize).min(data.len());
                        Some(Bytes::copy_from_slice(&data[start..end]))
                    } else {
                        None
                    }
                })
        };
        if let Some(slice) = staged_slice {
            self.limiter.take(slice.len() as u64).await;
            return Ok(slice);
        }

        self.read_cached_range(node, offset, len, 2).await
    }

    async fn open_write(&self, node: NodeId) -> Result<WriteHandle> {
        if let Some(ref staging) = *self.staging.lock() {
            staging
                .open_write(node as i64)
                .map_err(|e| SyncError::Io(e.to_string()))?;
        }
        Ok(WriteHandle { node })
    }

    async fn write(&self, node: NodeId, offset: u64, data: &[u8]) -> Result<u32> {
        if let Some(ref staging) = *self.staging.lock() {
            staging
                .write_at(node as i64, offset, data)
                .map_err(|e| SyncError::Io(e.to_string()))?;
        }
        let store = self.store.lock();
        if let Some(mut n) = store.get_node(node as i64).map_err(sync_err)? {
            n.size = (offset as i64 + data.len() as i64).max(n.size);
            n.state = NodeState::Dirty;
            store.upsert_node(&n).map_err(sync_err)?;
            let _ = store.enqueue_upload(node as i64, "update", n.head_revision.as_deref());
        }
        Ok(data.len() as u32)
    }

    async fn create(&self, parent: NodeId, name: &OsStr, kind: Kind) -> Result<NodeAttr> {
        let name = name.to_string_lossy().to_string();
        let store = self.store.lock();
        let account_id = if parent > 0 {
            store
                .get_node(parent as i64)
                .map_err(sync_err)?
                .map(|p| p.account_id)
                .unwrap_or_else(Uuid::nil)
        } else {
            Uuid::nil()
        };
        let id = store.next_id().map_err(sync_err)?;
        let n = Node {
            id,
            account_id,
            drive_id: String::new(),
            parent_id: Some(parent as i64),
            name: name.clone(),
            display_name: name,
            kind: match kind {
                Kind::Folder => "folder",
                Kind::NativeDoc => "native_doc",
                Kind::Shortcut => "shortcut",
                Kind::File => "file",
            }
            .into(),
            mime: None,
            size: 0,
            md5: None,
            modified_ms: 0,
            head_revision: None,
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Dirty,
            pinned: false,
            last_access_ms: 0,
        };
        store.upsert_node(&n).map_err(sync_err)?;
        let _ = store.enqueue_upload(id, "create", None);
        Ok(to_attr(&n))
    }

    async fn rename(&self, node: NodeId, new_parent: NodeId, new_name: &OsStr) -> Result<()> {
        let new_name = new_name.to_string_lossy().to_string();
        let store = self.store.lock();
        let mut n = store
            .get_node(node as i64)
            .map_err(sync_err)?
            .ok_or(SyncError::NotFound)?;
        n.parent_id = Some(new_parent as i64);
        n.name = new_name.clone();
        n.display_name = new_name;
        n.state = NodeState::Dirty;
        store.upsert_node(&n).map_err(sync_err)?;
        let _ = store.enqueue_upload(node as i64, "rename", n.head_revision.as_deref());
        Ok(())
    }

    async fn remove(&self, node: NodeId) -> Result<()> {
        let store = self.store.lock();
        if let Some(mut n) = store.get_node(node as i64).map_err(sync_err)? {
            n.trashed = true;
            store.upsert_node(&n).map_err(sync_err)?;
            let _ = store.enqueue_upload(node as i64, "trash", None);
        }
        if let Some(ref staging) = *self.staging.lock() {
            let _ = staging.remove(node as i64);
        }
        Ok(())
    }

    async fn set_pinned(&self, node: NodeId, pinned: bool) -> Result<()> {
        let store = self.store.lock();
        if let Some(mut n) = store.get_node(node as i64).map_err(sync_err)? {
            n.pinned = pinned;
            store.upsert_node(&n).map_err(sync_err)?;
        }
        Ok(())
    }

    fn subscribe(&self) -> mpsc::Receiver<EngineEvent> {
        let (tx, rx) = mpsc::channel(64);
        *self.events.lock() = Some(tx);
        rx
    }

    async fn add_account(&self, acct: Uuid) -> Result<()> {
        self.seed_root_account(acct, &acct.to_string())?;
        if let Some(tx) = self.events.lock().as_ref() {
            let _ = tx.try_send(EngineEvent::AccountAdded(acct));
        }
        Ok(())
    }

    async fn remove_account(&self, acct: Uuid) -> Result<()> {
        self.clients.lock().remove(&acct);
        self.root_drive_ids.lock().remove(&acct);
        if let Some(tx) = self.events.lock().as_ref() {
            let _ = tx.try_send(EngineEvent::AccountRemoved(acct));
        }
        Ok(())
    }

    async fn stats(&self) -> EngineStats {
        EngineStats {
            transfers: self.transfers.load(Ordering::Relaxed),
            queue_depth: self.store.lock().queue_depth().unwrap_or(0),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}
