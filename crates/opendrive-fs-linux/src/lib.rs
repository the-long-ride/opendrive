//! FUSE3 frontend (Linux).
//! Requires `fuse3` / `fusermount3` on the host. One mount at ~/OpenDrive;
//! accounts appear as folders under a synthetic root (no remount on add/remove).

use opendrive_sync::VfsEngine;
use std::sync::Arc;

pub struct FuseFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl FuseFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    /// Mount at `path`. Maps FUSE ops → VfsEngine (list/lookup/read/write/…).
    pub fn mount(&self, _path: &str) -> Result<(), String> {
        // fuser::mount2 later; invalidate via engine.subscribe()
        let _ = &self.engine;
        Ok(())
    }

    pub fn unmount(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}
