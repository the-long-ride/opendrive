//! Windows Cloud Files API frontend.
//! One sync root per account under %USERPROFILE%\OpenDrive\ (siblings).
//! Native placeholders, pin/unpin, Explorer integration. No WinFsp.

use opendrive_sync::VfsEngine;
use std::path::PathBuf;
use std::sync::Arc;

pub struct CloudFilesFrontend {
    engine: Arc<dyn VfsEngine>,
    roots: Vec<PathBuf>,
}

impl CloudFilesFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine,
            roots: Vec::new(),
        }
    }

    /// Register a per-account sync root. Callbacks will map to VfsEngine.
    pub fn register_sync_root(&mut self, path: &str, _account_name: &str) -> Result<(), String> {
        // cloud-filter::register + Provider callbacks:
        //   FETCH_DATA -> engine.read
        //   FETCH_PLACEHOLDERS -> engine.list
        //   ON_CLOSE / change notify -> enqueue upload
        let _ = &self.engine;
        self.roots.push(PathBuf::from(path));
        Ok(())
    }

    pub fn unregister_sync_root(&mut self, path: &str) -> Result<(), String> {
        self.roots.retain(|p| p != Path::new(path));
        Ok(())
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

use std::path::Path;
