//! Windows Cloud Files API frontend.
//! One sync root per account under %USERPROFILE%\OpenDrive\ (siblings).
//! Native placeholders, pin/unpin, Explorer integration. No WinFsp.

use opendrive_sync::VfsEngine;
use std::sync::Arc;

pub struct CloudFilesFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl CloudFilesFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    pub fn register_sync_root(&self, _path: &str, _account_name: &str) -> Result<(), String> {
        // cloud-filter register + callbacks → VfsEngine
        let _ = &self.engine;
        Ok(())
    }

    pub fn unregister_sync_root(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}
