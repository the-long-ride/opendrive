//! Store-backed VfsEngine (M0/M1).

use crate::queue::{conflict_name, QueueEntry, UploadOp};
use crate::{
    DirPage, EngineEvent, EngineStats, Kind, NodeAttr, NodeId, PageToken, Result, SyncError,
    VfsEngine, WriteHandle,
};
use async_trait::async_trait;
use bytes::Bytes;
use opendrive_store::{Node, NodeState, Store};
use parking_lot::Mutex;
use std::ffi::OsStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct SyncEngine {
    store: Arc<Mutex<Store>>,
    events: Mutex<Option<mpsc::Sender<EngineEvent>>>,
}

impl SyncEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            events: Mutex::new(None),
        }
    }

    pub fn seed_root_account(&self, account_id: Uuid, folder_name: &str) -> Result<NodeId> {
        let store = self.store.lock();
        // node id 1 = account folder under synthetic root 0
        let n = Node {
            id: 1,
            account_id,
            drive_id: "root".into(),
            parent_id: Some(0),
            name: folder_name.into(),
            display_name: folder_name.into(),
            kind: "folder".into(),
            mime: Some("application/vnd.google-apps.folder".into()),
            size: 0,
            md5: None,
            modified_ms: 0,
            head_revision: None,
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Cached,
            pinned: false,
            last_access_ms: 0,
        };
        store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
        Ok(1)
    }
}

fn to_attr(n: &Node) -> NodeAttr {
    NodeAttr {
        id: n.id as NodeId,
        name: n.display_name.clone(),
        kind: match n.kind.as_str() {
            "folder" => Kind::Folder,
            "native_doc" => Kind::NativeDoc,
            "shortcut" => Kind::Shortcut,
            _ => Kind::File,
        },
        size: n.size as u64,
        modified_ms: n.modified_ms,
    }
}

#[async_trait]
impl VfsEngine for SyncEngine {
    async fn list(&self, node: NodeId, _page: Option<PageToken>) -> Result<DirPage> {
        let store = self.store.lock();
        let children = store
            .list_children(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?;
        Ok(DirPage {
            entries: children.iter().map(to_attr).collect(),
            next_page: None,
        })
    }

    async fn lookup(&self, parent: NodeId, name: &OsStr) -> Result<Option<NodeAttr>> {
        let name = name.to_string_lossy();
        let store = self.store.lock();
        let children = store
            .list_children(parent as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?;
        Ok(children
            .iter()
            .find(|n| n.display_name == name || n.name == name)
            .map(to_attr))
    }

    async fn stat(&self, node: NodeId) -> Result<NodeAttr> {
        let store = self.store.lock();
        store
            .get_node(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
            .map(|n| to_attr(&n))
            .ok_or(SyncError::NotFound)
    }

    async fn read(&self, _node: NodeId, _offset: u64, _len: u32) -> Result<Bytes> {
        // hydration via DriveClient later
        Err(SyncError::Other("hydration not implemented".into()))
    }

    async fn open_write(&self, node: NodeId) -> Result<WriteHandle> {
        Ok(WriteHandle { node })
    }

    async fn create(&self, parent: NodeId, name: &OsStr, kind: Kind) -> Result<NodeAttr> {
        let name = name.to_string_lossy().to_string();
        let store = self.store.lock();
        let id = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()%
            i64::MAX as u128) as i64;
        let n = Node {
            id,
            account_id: Uuid::nil(),
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
        store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
        Ok(to_attr(&n))
    }

    async fn rename(&self, _node: NodeId, _new_parent: NodeId, _new_name: &OsStr) -> Result<()> {
        Ok(())
    }

    async fn remove(&self, node: NodeId) -> Result<()> {
        let store = self.store.lock();
        if let Some(mut n) = store
            .get_node(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
        {
            n.trashed = true;
            store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
        }
        Ok(())
    }

    async fn set_pinned(&self, node: NodeId, pinned: bool) -> Result<()> {
        let store = self.store.lock();
        if let Some(mut n) = store
            .get_node(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
        {
            n.pinned = pinned;
            store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
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
        Ok(())
    }

    async fn remove_account(&self, _acct: Uuid) -> Result<()> {
        Ok(())
    }

    async fn stats(&self) -> EngineStats {
        EngineStats {
            transfers: 0,
            queue_depth: 0,
            errors: 0,
        }
    }
}

// silence unused imports in this module until queue is wired deeper
#[allow(dead_code)]
fn _queue_touch() {
    let _ = conflict_name("a", "h", "d");
    let _ = UploadOp::Create;
    let _ = std::mem::size_of::<QueueEntry>();
}
