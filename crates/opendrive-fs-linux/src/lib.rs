//! FUSE3 frontend (Linux).

use opendrive_sync::VfsEngine;
use std::sync::Arc;

pub struct FuseFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl FuseFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    pub fn mount(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}
