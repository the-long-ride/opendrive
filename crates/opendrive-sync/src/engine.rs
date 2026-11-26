//! Store-backed VfsEngine (M0/M1).

use crate::limiter::BandwidthLimiter;
use crate::staging::StagingDir;
use crate::stream_cache::StreamCache;
use crate::{EngineEvent, Kind, NodeAttr, NodeId, Result, SyncError};
use opendrive_drive::DriveClient;
use opendrive_store::{Node, NodeState, Store};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

pub const SHARED_WITH_ME_DRIVE_ID: &str = "sharedWithMe";

pub struct SyncEngine {
    pub(crate) store: Arc<Mutex<Store>>,
    pub(crate) clients: Mutex<HashMap<Uuid, DriveClient>>,
    pub(crate) root_drive_ids: Mutex<HashMap<Uuid, String>>,
    pub(crate) events: Mutex<Option<mpsc::Sender<EngineEvent>>>,
    pub(crate) staging: Mutex<Option<StagingDir>>,
    pub(crate) stream_cache: Arc<StreamCache>,
    pub(crate) limiter: Arc<BandwidthLimiter>,
    pub(crate) transfers: Arc<AtomicU64>,
    pub(crate) errors: Arc<AtomicU64>,
}

impl SyncEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            clients: Mutex::new(HashMap::new()),
            root_drive_ids: Mutex::new(HashMap::new()),
            events: Mutex::new(None),
            staging: Mutex::new(None),
            stream_cache: Arc::new(StreamCache::new()),
            limiter: Arc::new(BandwidthLimiter::new(None)),
            transfers: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_staging_dir(self, path: impl Into<PathBuf>) -> Result<Self> {
        let dir = StagingDir::new(path).map_err(|e| SyncError::Io(e.to_string()))?;
        *self.staging.lock() = Some(dir);
        Ok(self)
    }

    pub fn set_bandwidth_limit(&self, bytes_per_sec: Option<u64>) {
        self.limiter.set_limit(bytes_per_sec);
    }

    pub fn set_drive_client(&self, account_id: Uuid, client: DriveClient) {
        self.clients.lock().insert(account_id, client);
    }

    pub fn store(&self) -> &Arc<Mutex<Store>> {
        &self.store
    }

    pub fn get_node(&self, node_id: NodeId) -> Result<Option<Node>> {
        self.store.lock().get_node(node_id as i64).map_err(sync_err)
    }

    pub fn drive_client_for(&self, account_id: Uuid) -> Result<DriveClient> {
        self.clients
            .lock()
            .get(&account_id)
            .cloned()
            .ok_or_else(|| SyncError::Other("no drive client".into()))
    }

    pub fn seed_root_account(&self, account_id: Uuid, folder_name: &str) -> Result<NodeId> {
        let store = self.store.lock();
        if let Ok(Some(existing)) = store.find_by_drive_id(account_id, "root") {
            return Ok(existing.id as NodeId);
        }
        let id = store
            .next_id()
            .map_err(|e| SyncError::Other(e.to_string()))?;
        store
            .upsert_node(&Node {
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
            })
            .map_err(|e| SyncError::Other(e.to_string()))?;
        Ok(id as NodeId)
    }

    pub fn enqueue_upload(
        &self,
        node_id: i64,
        op: &str,
        base_revision: Option<&str>,
    ) -> Result<i64> {
        self.store
            .lock()
            .enqueue_upload(node_id, op, base_revision)
            .map_err(sync_err)
    }
}

pub(crate) fn sync_err<E: ToString>(e: E) -> SyncError {
    SyncError::Other(e.to_string())
}

pub(crate) fn to_attr(n: &Node) -> NodeAttr {
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
