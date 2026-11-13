//! Maps NFSv3 operations to VfsEngine (for nfsserve integration).

use opendrive_sync::{Kind, NodeId, VfsEngine};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::Arc;
use parking_lot::Mutex;

/// File-handle opaque id used by NFS clients.
pub type Fh = u64;

pub struct NfsBackend {
    engine: Arc<dyn VfsEngine>,
    fh_to_node: Mutex<HashMap<Fh, NodeId>>,
    next_fh: Mutex<Fh>,
}

impl NfsBackend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        let mut map = HashMap::new();
        map.insert(1, 0); // root
        Self {
            engine,
            fh_to_node: Mutex::new(map),
            next_fh: Mutex::new(2),
        }
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

    pub async fn lookup(&self, parent_fh: Fh, name: &str) -> Result<Option<(Fh, bool, u64)>, String> {
        if name.starts_with("._") || name == ".DS_Store" {
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
        if fh == 1 {
            return Ok((true, 0));
        }
        let node = self.node(fh).ok_or("bad fh")?;
        let a = self.engine.stat(node).await.map_err(|e| e.to_string())?;
        Ok((matches!(a.kind, Kind::Folder), a.size))
    }

    pub async fn readdir(&self, fh: Fh) -> Result<Vec<(String, Fh, bool)>, String> {
        let node = self.node(fh).ok_or("bad fh")?;
        let page = self
            .engine
            .list(node, None)
            .await
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for e in page.entries {
            if e.name.starts_with("._") || e.name == ".DS_Store" {
                continue;
            }
            let child_fh = self.alloc_fh(e.id);
            out.push((e.name, child_fh, matches!(e.kind, Kind::Folder)));
        }
        Ok(out)
    }

    pub async fn read(&self, fh: Fh, offset: u64, len: u32) -> Result<bytes::Bytes, String> {
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
}
