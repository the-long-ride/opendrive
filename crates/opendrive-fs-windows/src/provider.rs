//! Cloud Files provider callbacks mapped to `VfsEngine`.

use opendrive_sync::{Kind, NodeId, VfsEngine};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct ProviderBridge {
    engine: Arc<dyn VfsEngine>,
    root_node: NodeId,
    /// Relative path under sync root -> NodeId.
    path_index: Mutex<HashMap<PathBuf, NodeId>>,
}

impl ProviderBridge {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self::with_root(engine, 0)
    }

    pub fn with_root(engine: Arc<dyn VfsEngine>, root_node: NodeId) -> Self {
        Self {
            engine,
            root_node,
            path_index: Mutex::new(HashMap::new()),
        }
    }

    pub fn root_node(&self) -> NodeId {
        self.root_node
    }

    pub fn index_path(&self, rel: PathBuf, node: NodeId) {
        self.path_index.lock().unwrap().insert(rel, node);
    }

    pub fn node_for_path(&self, rel: &Path) -> Option<NodeId> {
        self.path_index.lock().unwrap().get(rel).copied()
    }

    pub fn remove_path(&self, rel: &Path) -> Option<NodeId> {
        self.path_index.lock().unwrap().remove(rel)
    }

    /// FETCH_PLACEHOLDERS equivalent: returns `(name, node_id, is_dir, size)`.
    pub async fn fetch_placeholders(
        &self,
        parent: NodeId,
    ) -> Result<Vec<(String, NodeId, bool, u64)>, String> {
        let page = self
            .engine
            .list(parent, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(page
            .entries
            .into_iter()
            .map(|e| (e.name, e.id, matches!(e.kind, Kind::Folder), e.size))
            .collect())
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
            Some(a) => Ok(Some((a.id, matches!(a.kind, Kind::Folder), a.size))),
            None => Ok(None),
        }
    }

    pub async fn create_entry(
        &self,
        parent: NodeId,
        name: &OsStr,
        is_dir: bool,
    ) -> Result<(NodeId, bool, u64), String> {
        let kind = if is_dir { Kind::Folder } else { Kind::File };
        let attr = self
            .engine
            .create(parent, name, kind)
            .await
            .map_err(|e| e.to_string())?;
        Ok((attr.id, matches!(attr.kind, Kind::Folder), attr.size))
    }

    pub async fn write_data(&self, node: NodeId, offset: u64, data: &[u8]) -> Result<u32, String> {
        self.engine
            .write(node, offset, data)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn rename_entry(
        &self,
        node: NodeId,
        new_parent: NodeId,
        new_name: &OsStr,
    ) -> Result<(), String> {
        self.engine
            .rename(node, new_parent, new_name)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete_entry(&self, node: NodeId) -> Result<(), String> {
        self.engine.remove(node).await.map_err(|e| e.to_string())
    }

    pub async fn set_pinned(&self, node: NodeId, pinned: bool) -> Result<(), String> {
        self.engine
            .set_pinned(node, pinned)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_store::Store;
    use opendrive_sync::SyncEngine;
    use uuid::Uuid;

    fn test_bridge() -> (ProviderBridge, PathBuf) {
        let staging = std::env::temp_dir().join(format!("od-win-provider-{}", Uuid::new_v4()));
        let store = Store::open_in_memory().unwrap();
        let engine = SyncEngine::new(store).with_staging_dir(&staging).unwrap();
        (ProviderBridge::new(Arc::new(engine)), staging)
    }

    #[test]
    fn path_index_insert_lookup_and_remove() {
        let (bridge, staging) = test_bridge();
        let rel = PathBuf::from("docs/report.txt");
        bridge.index_path(rel.clone(), 17);
        assert_eq!(bridge.node_for_path(&rel), Some(17));
        assert_eq!(bridge.remove_path(&rel), Some(17));
        assert_eq!(bridge.node_for_path(&rel), None);
        let _ = std::fs::remove_dir_all(staging);
    }

    #[tokio::test]
    async fn bridge_crud_write_fetch_and_pin() {
        let (bridge, staging) = test_bridge();

        let (dir_id, is_dir, _) = bridge
            .create_entry(0, OsStr::new("Work"), true)
            .await
            .unwrap();
        assert!(is_dir);

        let (file_id, is_dir, _) = bridge
            .create_entry(dir_id, OsStr::new("hello.txt"), false)
            .await
            .unwrap();
        assert!(!is_dir);

        let written = bridge.write_data(file_id, 0, b"cloud-data").await.unwrap();
        assert_eq!(written, 10);

        let bytes = bridge.fetch_data(file_id, 6, 4).await.unwrap();
        assert_eq!(&bytes[..], b"data");

        let placeholders = bridge.fetch_placeholders(dir_id).await.unwrap();
        assert_eq!(placeholders.len(), 1);
        assert_eq!(placeholders[0].0, "hello.txt");
        assert_eq!(placeholders[0].1, file_id);
        assert_eq!(placeholders[0].3, 10);

        let looked = bridge
            .lookup_child(dir_id, OsStr::new("hello.txt"))
            .await
            .unwrap();
        assert_eq!(looked, Some((file_id, false, 10)));

        bridge.set_pinned(file_id, true).await.unwrap();
        bridge
            .rename_entry(file_id, 0, OsStr::new("moved.txt"))
            .await
            .unwrap();
        assert_eq!(
            bridge
                .lookup_child(0, OsStr::new("moved.txt"))
                .await
                .unwrap()
                .map(|v| v.0),
            Some(file_id)
        );

        bridge.delete_entry(file_id).await.unwrap();
        assert_eq!(
            bridge
                .lookup_child(0, OsStr::new("moved.txt"))
                .await
                .unwrap(),
            None
        );

        let _ = std::fs::remove_dir_all(staging);
    }
}
