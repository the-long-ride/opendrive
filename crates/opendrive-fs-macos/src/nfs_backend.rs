//! Maps NFSv3 operations to `VfsEngine` (for `nfsserve` integration).

use opendrive_sync::{Kind, NodeId, VfsEngine};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::Arc;

/// File-handle opaque id used by NFS clients (`1` = root).
pub type Fh = u64;

#[derive(Clone)]
pub struct NfsBackend {
    engine: Arc<dyn VfsEngine>,
    fh_to_node: Arc<Mutex<HashMap<Fh, NodeId>>>,
    next_fh: Arc<Mutex<Fh>>,
}

impl NfsBackend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        let mut map = HashMap::new();
        map.insert(1, 0);
        Self {
            engine,
            fh_to_node: Arc::new(Mutex::new(map)),
            next_fh: Arc::new(Mutex::new(2)),
        }
    }

    pub fn is_ignored_name(name: &str) -> bool {
        name.starts_with("._") || name == ".DS_Store"
    }

    fn alloc_fh(&self, node: NodeId) -> Fh {
        let mut map = self.fh_to_node.lock();
        if let Some((&fh, _)) = map.iter().find(|(_, n)| **n == node) {
            return fh;
        }
        let mut next = self.next_fh.lock();
        let fh = *next;
        *next += 1;
        map.insert(fh, node);
        fh
    }

    fn node(&self, fh: Fh) -> Option<NodeId> {
        self.fh_to_node.lock().get(&fh).copied()
    }

    pub async fn lookup(
        &self,
        parent_fh: Fh,
        name: &str,
    ) -> Result<Option<(Fh, bool, u64)>, String> {
        if Self::is_ignored_name(name) {
            return Ok(None);
        }
        let parent = self.node(parent_fh).ok_or("bad fh")?;
        match self
            .engine
            .lookup(parent, OsStr::new(name))
            .await
            .map_err(|e| e.to_string())?
        {
            Some(a) => {
                let fh = self.alloc_fh(a.id);
                Ok(Some((fh, matches!(a.kind, Kind::Folder), a.size)))
            }
            None => Ok(None),
        }
    }

    pub async fn getattr(&self, fh: Fh) -> Result<(bool, u64), String> {
        let (is_dir, size, _) = self.getattr_meta(fh).await?;
        Ok((is_dir, size))
    }

    pub async fn getattr_meta(&self, fh: Fh) -> Result<(bool, u64, i64), String> {
        if fh == 1 {
            return Ok((true, 0, 0));
        }
        let node = self.node(fh).ok_or("bad fh")?;
        let a = self.engine.stat(node).await.map_err(|e| e.to_string())?;
        Ok((matches!(a.kind, Kind::Folder), a.size, a.modified_ms))
    }

    pub async fn readdir(&self, fh: Fh) -> Result<Vec<(String, Fh, bool)>, String> {
        let entries = self.readdir_meta(fh).await?;
        Ok(entries
            .into_iter()
            .map(|(name, cfh, is_dir, _, _)| (name, cfh, is_dir))
            .collect())
    }

    pub async fn readdir_meta(&self, fh: Fh) -> Result<Vec<(String, Fh, bool, u64, i64)>, String> {
        let node = self.node(fh).ok_or("bad fh")?;
        let page = self
            .engine
            .list(node, None)
            .await
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for e in page.entries {
            if Self::is_ignored_name(&e.name) {
                continue;
            }
            let child_fh = self.alloc_fh(e.id);
            out.push((
                e.name,
                child_fh,
                matches!(e.kind, Kind::Folder),
                e.size,
                e.modified_ms,
            ));
        }
        Ok(out)
    }

    pub async fn read(&self, fh: Fh, offset: u64, len: u32) -> Result<bytes::Bytes, String> {
        let (_, size, _) = self.getattr_meta(fh).await?;
        if offset >= size {
            return Ok(bytes::Bytes::new());
        }
        let node = self.node(fh).ok_or("bad fh")?;
        self.engine
            .read(node, offset, len)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn write(&self, fh: Fh, offset: u64, data: &[u8]) -> Result<u32, String> {
        let node = self.node(fh).ok_or("bad fh")?;
        self.engine
            .write(node, offset, data)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn create(&self, parent_fh: Fh, name: &str) -> Result<(Fh, bool, u64), String> {
        let parent = self.node(parent_fh).ok_or("bad fh")?;
        let a = self
            .engine
            .create(parent, OsStr::new(name), Kind::File)
            .await
            .map_err(|e| e.to_string())?;
        let fh = self.alloc_fh(a.id);
        Ok((fh, matches!(a.kind, Kind::Folder), a.size))
    }

    pub async fn mkdir(&self, parent_fh: Fh, name: &str) -> Result<(Fh, bool, u64), String> {
        let parent = self.node(parent_fh).ok_or("bad fh")?;
        let a = self
            .engine
            .create(parent, OsStr::new(name), Kind::Folder)
            .await
            .map_err(|e| e.to_string())?;
        let fh = self.alloc_fh(a.id);
        Ok((fh, matches!(a.kind, Kind::Folder), a.size))
    }

    pub async fn remove(&self, parent_fh: Fh, name: &str) -> Result<(), String> {
        let parent = self.node(parent_fh).ok_or("bad fh")?;
        let attr = self
            .engine
            .lookup(parent, OsStr::new(name))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        self.engine
            .remove(attr.id)
            .await
            .map_err(|e| e.to_string())?;
        self.fh_to_node.lock().retain(|_, n| *n != attr.id);
        Ok(())
    }

    pub async fn rename(
        &self,
        from_fh: Fh,
        from_name: &str,
        to_fh: Fh,
        to_name: &str,
    ) -> Result<(), String> {
        let from_parent = self.node(from_fh).ok_or("bad fh")?;
        let to_parent = self.node(to_fh).ok_or("bad fh")?;
        let attr = self
            .engine
            .lookup(from_parent, OsStr::new(from_name))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        self.engine
            .rename(attr.id, to_parent, OsStr::new(to_name))
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_store::Store;
    use opendrive_sync::SyncEngine;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn test_backend() -> (NfsBackend, PathBuf) {
        let staging = std::env::temp_dir().join(format!("od-mac-backend-{}", Uuid::new_v4()));
        let store = Store::open_in_memory().unwrap();
        let engine = SyncEngine::new(store).with_staging_dir(&staging).unwrap();
        (NfsBackend::new(Arc::new(engine)), staging)
    }

    #[tokio::test]
    async fn backend_crud_readdir_filters_macos_metadata() {
        let (backend, staging) = test_backend();
        assert_eq!(backend.getattr(1).await.unwrap(), (true, 0));

        let (dir_fh, is_dir, _) = backend.mkdir(1, "Docs").await.unwrap();
        assert!(is_dir);

        let (file_fh, is_dir, _) = backend.create(dir_fh, "notes.txt").await.unwrap();
        assert!(!is_dir);
        assert_eq!(backend.write(file_fh, 0, b"hello nfs").await.unwrap(), 9);
        assert_eq!(&backend.read(file_fh, 6, 3).await.unwrap()[..], b"nfs");
        assert!(backend.read(file_fh, 100, 10).await.unwrap().is_empty());

        let _ = backend.create(dir_fh, ".DS_Store").await.unwrap();
        let _ = backend.create(dir_fh, "._notes.txt").await.unwrap();
        assert_eq!(backend.lookup(dir_fh, ".DS_Store").await.unwrap(), None);
        assert_eq!(backend.lookup(dir_fh, "._notes.txt").await.unwrap(), None);

        let listed = backend.readdir(dir_fh).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "notes.txt");

        backend
            .rename(dir_fh, "notes.txt", 1, "renamed.txt")
            .await
            .unwrap();
        assert!(backend.lookup(dir_fh, "notes.txt").await.unwrap().is_none());
        assert!(backend.lookup(1, "renamed.txt").await.unwrap().is_some());

        backend.remove(1, "renamed.txt").await.unwrap();
        assert!(backend.lookup(1, "renamed.txt").await.unwrap().is_none());
        assert!(backend.remove(1, "renamed.txt").await.is_err());

        let _ = std::fs::remove_dir_all(staging);
    }
}
