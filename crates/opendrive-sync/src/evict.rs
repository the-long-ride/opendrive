//! Cache eviction respecting safety invariant #3.

use crate::engine::SyncEngine;
use crate::{Result, SyncError};
use std::fs;
use std::path::Path;
use tracing::info;

impl SyncEngine {
    /// Evict unpinned, non-dirty cache entries until total <= max_bytes.
    /// Returns number of entries removed.
    pub fn evict_to(&self, max_bytes: u64) -> Result<usize> {
        let store = self.store.lock();
        let total = store
            .cache_bytes_total()
            .map_err(|e| SyncError::Other(e.to_string()))? as u64;
        if total <= max_bytes {
            return Ok(0);
        }

        let candidates = store
            .eviction_candidates(64)
            .map_err(|e| SyncError::Other(e.to_string()))?;

        let mut removed = 0usize;
        let mut current = total;
        for node_id in candidates {
            if current <= max_bytes {
                break;
            }
            if let Ok(Some(path)) = store.get_cache_path(node_id) {
                Self::delete_cache_file_path(Path::new(&path));
            }
            store
                .remove_cache_entry(node_id)
                .map_err(|e| SyncError::Other(e.to_string()))?;
            removed += 1;
            current = store.cache_bytes_total().unwrap_or(0) as u64;
        }
        info!(removed, "cache eviction pass");
        Ok(removed)
    }

    pub fn delete_cache_file_path(path: &Path) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_store::{now_ms, Node, NodeState, Store};
    use uuid::Uuid;

    #[test]
    fn evict_to_respects_budget_and_deletes_unpinned_files() {
        let store = Store::open_in_memory().unwrap();
        let acct = Uuid::new_v4();
        let tmp = std::env::temp_dir().join(format!("od-evict-unit-{}", Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let f1 = tmp.join("a.bin");
        let f2 = tmp.join("b.bin");
        fs::write(&f1, b"12345").unwrap();
        fs::write(&f2, b"12345").unwrap();
        for (id, name, path, pinned) in [(1, "a.bin", &f1, false), (2, "b.bin", &f2, true)] {
            store
                .upsert_node(&Node {
                    id,
                    account_id: acct,
                    drive_id: format!("d{id}"),
                    parent_id: None,
                    name: name.into(),
                    display_name: name.into(),
                    kind: "file".into(),
                    mime: None,
                    size: 500,
                    md5: None,
                    modified_ms: 1,
                    head_revision: None,
                    trashed: false,
                    shared_drive_id: None,
                    state: NodeState::Cached,
                    pinned,
                    last_access_ms: now_ms() + id,
                })
                .unwrap();
            store
                .set_cache_entry(id, &path.to_string_lossy(), 500, pinned)
                .unwrap();
        }
        let eng = SyncEngine::new(store);
        assert_eq!(eng.evict_to(1000).unwrap(), 0);
        assert_eq!(eng.evict_to(500).unwrap(), 1);
        assert!(!f1.exists() && f2.exists());
        let _ = fs::remove_dir_all(&tmp);
    }
}
