//! Cloud Files provider callbacks mapped to VfsEngine.
//!
//! When `cloud-filter` is enabled on Windows, implement the SyncFilter
//! trait by forwarding to these helpers.

use opendrive_sync::{Kind, NodeId, VfsEngine};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ProviderBridge {
    engine: Arc<dyn VfsEngine>,
    /// relative path under sync root -> NodeId
    path_index: std::sync::Mutex<std::collections::HashMap<PathBuf, NodeId>>,
}

impl ProviderBridge {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine,
            path_index: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn index_path(&self, rel: PathBuf, node: NodeId) {
        self.path_index.lock().unwrap().insert(rel, node);
    }

    pub fn node_for_path(&self, rel: &Path) -> Option<NodeId> {
        self.path_index.lock().unwrap().get(rel).copied()
    }

    /// FETCH_PLACEHOLDERS equivalent.
    pub async fn fetch_placeholders(
        &self,
        parent: NodeId,
    ) -> Result<Vec<(String, NodeId, bool, u64)>, String> {
        let page = self
            .engine
            .list(parent, None)
            .await
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for e in page.entries {
            let is_dir = matches!(e.kind, Kind::Folder);
            out.push((e.name, e.id, is_dir, e.size));
        }
        Ok(out)
    }

    /// FETCH_DATA equivalent.
    pub async fn fetch_data(
        &self,
        node: NodeId,
        offset: u64,
        len: u32,
    ) -> Result<bytes::Bytes, String> {
        self.engine
            .read(node, offset, len)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn lookup_child(
        &self,
        parent: NodeId,
        name: &OsStr,
    ) -> Result<Option<(NodeId, bool, u64)>, String> {
        match self
            .engine
            .lookup(parent, name)
            .await
            .map_err(|e| e.to_string())?
        {
            Some(a) => Ok(Some((
                a.id,
                matches!(a.kind, Kind::Folder),
                a.size,
            ))),
            None => Ok(None),
        }
    }
}
