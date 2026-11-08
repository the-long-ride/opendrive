//! Store-backed VfsEngine (M0/M1).

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
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct SyncEngine {
    store: Arc<Mutex<Store>>,
    /// account_id -> DriveClient (access token)
    clients: Mutex<HashMap<Uuid, DriveClient>>,
    events: Mutex<Option<mpsc::Sender<EngineEvent>>>,
}

impl SyncEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            clients: Mutex::new(HashMap::new()),
            events: Mutex::new(None),
        }
    }

    pub fn set_drive_client(&self, account_id: Uuid, client: DriveClient) {
        self.clients.lock().insert(account_id, client);
    }

    pub fn seed_root_account(&self, account_id: Uuid, folder_name: &str) -> Result<NodeId> {
        let store = self.store.lock();
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

    /// Pull one folder page from Drive into the store.
    pub async fn hydrate_folder(&self, account_id: Uuid, parent_node: NodeId) -> Result<()> {
        let (drive_parent, next_id_base) = {
            let store = self.store.lock();
            let n = store
                .get_node(parent_node as i64)
                .map_err(|e| SyncError::Other(e.to_string()))?
                .ok_or(SyncError::NotFound)?;
            (n.drive_id.clone(), parent_node as i64 * 1000 + 10)
        };

        let client = {
            let clients = self.clients.lock();
            clients
                .get(&account_id)
                .ok_or_else(|| SyncError::Other("no drive client".into()))?
                .access_token
                .clone()
        };
        let client = DriveClient::new(client);

        let list = client
            .list_files(&drive_parent, None, true)
            .await
            .map_err(|e| SyncError::Other(e.to_string()))?;

        let store = self.store.lock();
        for (i, f) in list.files.into_iter().enumerate() {
            let kind = if f.mime_type == "application/vnd.google-apps.folder" {
                "folder"
            } else if opendrive_drive::is_native_google_doc(&f.mime_type) {
                "native_doc"
            } else {
                "file"
            };
            let n = Node {
                id: next_id_base + i as i64,
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
            };
            store.upsert_node(&n).map_err(|e| SyncError::Other(e.to_string()))?;
        }
        Ok(())
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

    async fn read(&self, node: NodeId, offset: u64, len: u32) -> Result<Bytes> {
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
        let client = DriveClient::new(token);
        client
            .download_range(&drive_id, offset, len)
            .await
            .map_err(|e| SyncError::Io(e.to_string()))
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
            .as_nanos()
            % i64::MAX as u128) as i64;
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
