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
