//! Store-backed VfsEngine (M0/M1).

use crate::staging::StagingDir;
use crate::{
    DirPage, EngineEvent, EngineStats, Kind, NodeAttr, NodeId, PageToken, Result, SyncError,
    VfsEngine, WriteHandle,
};
use async_trait::async_trait;
use bytes::Bytes;
use opendrive_drive::DriveClient;
use opendrive_store::{Node, NodeState, Store};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct SyncEngine {
    pub(crate) store: Arc<Mutex<Store>>,
    pub(crate) clients: Mutex<HashMap<Uuid, DriveClient>>,
    pub(crate) events: Mutex<Option<mpsc::Sender<EngineEvent>>>,
    pub(crate) staging: Mutex<Option<StagingDir>>,
}

impl SyncEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            clients: Mutex::new(HashMap::new()),
            events: Mutex::new(None),
            staging: Mutex::new(None),
        }
    }

    pub fn with_staging_dir(self, path: impl Into<PathBuf>) -> Result<Self> {
        let dir = StagingDir::new(path).map_err(|e| SyncError::Io(e.to_string()))?;
        *self.staging.lock() = Some(dir);
        Ok(self)
    }

    pub fn set_drive_client(&self, account_id: Uuid, client: DriveClient) {
        self.clients.lock().insert(account_id, client);
    }

    pub fn seed_root_account(&self, account_id: Uuid, folder_name: &str) -> Result<NodeId> {
        let store = self.store.lock();
        if let Ok(Some(existing)) = store.find_by_drive_id(account_id, "root") {
            return Ok(existing.id as NodeId);
        }
        let id = store.next_id().map_err(|e| SyncError::Other(e.to_string()))?;
        let n = Node {
            id,
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
        Ok(id as NodeId)
    }

    pub async fn hydrate_folder(&self, account_id: Uuid, parent_node: NodeId) -> Result<()> {
        let drive_parent = {
            let store = self.store.lock();
            store
                .get_node(parent_node as i64)
                .map_err(|e| SyncError::Other(e.to_string()))?
                .ok_or(SyncError::NotFound)?
                .drive_id
                .clone()
        };
        let token = {
            let clients = self.clients.lock();
            clients
                .get(&account_id)
                .ok_or_else(|| SyncError::Other("no drive client".into()))?
                .access_token
                .clone()
        };
        let list = DriveClient::new(token)
            .list_files(&drive_parent, None, true)
            .await
            .map_err(|e| SyncError::Other(e.to_string()))?;
        let store = self.store.lock();
        for f in list.files {
            let kind = if f.mime_type == "application/vnd.google-apps.folder" {
                "folder"
            } else if opendrive_drive::is_native_google_doc(&f.mime_type) {
                "native_doc"
            } else {
                "file"
            };
            let id = match store.find_by_drive_id(account_id, &f.id) {
                Ok(Some(existing)) => existing.id,
                _ => store.next_id().map_err(|e| SyncError::Other(e.to_string()))?,
            };
            store
                .upsert_node(&Node {
                    id,
                    account_id,
                    drive_id: f.id,
                    parent_id: Some(parent_node as i64),
                    name: f.name.clone(),
                    display_name: f.name,
                    kind: kind.into(),
                    mime: Some(f.mime_type),
                    size: f.size.unwrap_or(0) as i64,
                    md5: f.md5,
                    modified_ms: 0,
                    head_revision: f.head_revision_id,
                    trashed: f.trashed,
                    shared_drive_id: None,
                    state: NodeState::Placeholder,
                    pinned: false,
                    last_access_ms: 0,
                })
                .map_err(|e| SyncError::Other(e.to_string()))?;
        }
        Ok(())
    }

    pub fn enqueue_upload(&self, node_id: i64, op: &str, base_revision: Option<&str>) -> Result<i64> {
        self.store
            .lock()
            .enqueue_upload(node_id, op, base_revision)
            .map_err(|e| SyncError::Other(e.to_string()))
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
        Ok(store
            .list_children(parent as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
            .iter()
            .find(|n| n.display_name == name || n.name == name)
            .map(to_attr))
    }

    async fn stat(&self, node: NodeId) -> Result<NodeAttr> {
        self.store
            .lock()
            .get_node(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
            .map(|n| to_attr(&n))
            .ok_or(SyncError::NotFound)
    }

    async fn read(&self, node: NodeId, offset: u64, len: u32) -> Result<Bytes> {
        if let Some(ref staging) = *self.staging.lock() {
            if let Ok(data) = staging.read_all(node as i64) {
                if !data.is_empty() {
                    let start = offset as usize;
                    let end = (offset as usize + len as usize).min(data.len());
                    if start < data.len() {
                        return Ok(Bytes::copy_from_slice(&data[start..end]));
                    }
                }
            }
        }
        let (account_id, drive_id) = {
            let store = self.store.lock();
            let n = store
                .get_node(node as i64)
                .map_err(|e| SyncError::Other(e.to_string()))?
                .ok_or(SyncError::NotFound)?;
            (n.account_id, n.drive_id.clone())
        };
        let token = {
            let clients = self.clients.lock();
            clients
                .get(&account_id)
                .ok_or_else(|| SyncError::Other("no drive client".into()))?
                .access_token
                .clone()
        };
        DriveClient::new(token)
            .download_range(&drive_id, offset, len)
            .await
            .map_err(|e| SyncError::Io(e.to_string()))
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
        if let Some(mut n) = store
            .get_node(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
        {
            let new_size = (offset as i64 + data.len() as i64).max(n.size);
            n.size = new_size;
            n.state = NodeState::Dirty;
            store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
            let _ = store.enqueue_upload(node as i64, "update", n.head_revision.as_deref());
        }
        Ok(data.len() as u32)
    }

    async fn create(&self, parent: NodeId, name: &OsStr, kind: Kind) -> Result<NodeAttr> {
        let name = name.to_string_lossy().to_string();
        let store = self.store.lock();
        let id = store.next_id().map_err(|e| SyncError::Other(e.to_string()))?;
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
        let _ = store.enqueue_upload(id, "create", None);
        Ok(to_attr(&n))
    }

    async fn rename(&self, node: NodeId, new_parent: NodeId, new_name: &OsStr) -> Result<()> {
        let new_name = new_name.to_string_lossy().to_string();
        let store = self.store.lock();
        let mut n = store
            .get_node(node as i64)
            .map_err(|e| SyncError::Other(e.to_string()))?
            .ok_or(SyncError::NotFound)?;
        n.parent_id = Some(new_parent as i64);
        n.name = new_name.clone();
        n.display_name = new_name;
        n.state = NodeState::Dirty;
        store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
        let _ = store.enqueue_upload(node as i64, "rename", n.head_revision.as_deref());
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
            let _ = store.enqueue_upload(node as i64, "trash", None);
        }
        if let Some(ref staging) = *self.staging.lock() {
            let _ = staging.remove(node as i64);
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
            queue_depth: self.store.lock().queue_depth().unwrap_or(0),
            errors: 0,
        }
    }
}
