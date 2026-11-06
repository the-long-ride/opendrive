//! macOS localhost NFSv3 frontend.

use opendrive_sync::VfsEngine;
use std::sync::Arc;

pub struct NfsFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl NfsFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    pub fn serve_and_mount(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}
