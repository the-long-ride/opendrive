//! Store-backed VfsEngine (M0/M1).

use crate::limiter::BandwidthLimiter;
use crate::staging::StagingDir;
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
    pub(crate) events: Mutex<Option<mpsc::Sender<EngineEvent>>>,
    pub(crate) staging: Mutex<Option<StagingDir>>,
    pub(crate) limiter: BandwidthLimiter,
    pub(crate) transfers: AtomicU64,
    pub(crate) errors: AtomicU64,
}

impl SyncEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            clients: Mutex::new(HashMap::new()),
            events: Mutex::new(None),
            staging: Mutex::new(None),
            limiter: BandwidthLimiter::new(None),
            transfers: AtomicU64::new(0),
            errors: AtomicU64::new(0),
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
        let client = self.drive_client_for(account_id)?;
        let mut all_files = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let list = if drive_parent == SHARED_WITH_ME_DRIVE_ID {
                client
                    .list_shared_with_me(page_token.as_deref())
                    .await
                    .map_err(sync_err)?
            } else {
                client
                    .list_files(&drive_parent, page_token.as_deref(), true)
                    .await
                    .map_err(sync_err)?
            };
            all_files.extend(list.files);
            match list.next_page_token {
                Some(tok) if !tok.is_empty() => page_token = Some(tok),
                _ => break,
            }
        }
        let store = self.store.lock();
        if drive_parent == "root"
            && store
                .find_by_drive_id(account_id, SHARED_WITH_ME_DRIVE_ID)
                .map_err(sync_err)?
                .is_none()
        {
            let swm_id = store.next_id().map_err(sync_err)?;
            store
                .upsert_node(&Node {
                    id: swm_id,
                    account_id,
                    drive_id: SHARED_WITH_ME_DRIVE_ID.into(),
                    parent_id: Some(parent_node as i64),
                    name: "Shared with me".into(),
                    display_name: "Shared with me".into(),
                    kind: "folder".into(),
                    mime: Some("application/vnd.google-apps.folder".into()),
                    size: 0,
                    md5: None,
                    modified_ms: 0,
                    head_revision: None,
                    trashed: false,
                    shared_drive_id: None,
                    state: NodeState::Placeholder,
                    pinned: false,
                    last_access_ms: 0,
                })
                .map_err(sync_err)?;
        }
        for f in all_files {
            let (effective_id, effective_mime) =
                if f.mime_type == "application/vnd.google-apps.shortcut" {
                    match f.shortcut_target_id.filter(|s| !s.is_empty()) {
                        Some(tid) => (tid, f.shortcut_target_mime.unwrap_or(f.mime_type)),
                        None => (f.id, f.mime_type),
                    }
                } else {
                    (f.id, f.mime_type)
                };
            let kind = if effective_mime == "application/vnd.google-apps.folder" {
                "folder"
            } else if effective_mime == "application/vnd.google-apps.shortcut" {
                "shortcut"
            } else if opendrive_drive::is_native_google_doc(&effective_mime) {
                "native_doc"
            } else {
                "file"
            };
            let id = match store.find_by_drive_id(account_id, &effective_id) {
                Ok(Some(existing)) => existing.id,
                _ => store.next_id().map_err(sync_err)?,
            };
            store
                .upsert_node(&Node {
                    id,
                    account_id,
                    drive_id: effective_id,
                    parent_id: Some(parent_node as i64),
                    name: f.name.clone(),
                    display_name: f.name,
                    kind: kind.into(),
                    mime: Some(effective_mime),
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
                .map_err(sync_err)?;
        }
        Ok(())
    }

    pub fn placeholder_folder_rel_paths(&self, root_node: NodeId) -> Vec<PathBuf> {
        let store = self.store.lock();
        let mut out = Vec::new();
        let mut stack = vec![(root_node as i64, PathBuf::new())];
        while let Some((pid, rel)) = stack.pop() {
            let Ok(children) = store.list_children(pid) else {
                continue;
            };
            for child in children {
                if child.kind != "folder" {
                    continue;
                }
                let child_rel = rel.join(&child.display_name);
                let has_grand = store
                    .list_children(child.id)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                if child.state == NodeState::Placeholder && !has_grand {
                    out.push(child_rel);
                } else {
                    stack.push((child.id, child_rel));
                }
            }
        }
        out
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
