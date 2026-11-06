//! Windows Cloud Files API frontend.

use opendrive_sync::VfsEngine;
use std::sync::Arc;

pub struct CloudFilesFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl CloudFilesFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    pub fn register_sync_root(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}
