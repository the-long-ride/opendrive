//! Windows Cloud Files API frontend.
//! One sync root per account under %USERPROFILE%\OpenDrive\ (siblings).

mod provider;

pub use provider::ProviderBridge;

use opendrive_sync::VfsEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct CloudFilesFrontend {
    engine: Arc<dyn VfsEngine>,
    roots: Vec<PathBuf>,
    bridges: Vec<ProviderBridge>,
}

impl CloudFilesFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine,
            roots: Vec::new(),
            bridges: Vec::new(),
        }
    }

    pub fn register_sync_root(&mut self, path: &str, account_name: &str) -> Result<(), String> {
        let p = PathBuf::from(path);
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        tracing::info!(%path, account_name, "register sync root");

        // Windows + cloud-filter:
        //   let filter = SyncFilter::register(path, ProviderBridge::new(...))?;
        //   filter.connect()?;
        let bridge = ProviderBridge::new(self.engine.clone());
        self.bridges.push(bridge);
        self.roots.push(p);
        Ok(())
    }

    pub fn unregister_sync_root(&mut self, path: &str) -> Result<(), String> {
        self.roots.retain(|p| p != Path::new(path));
        Ok(())
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn parent_folder() -> PathBuf {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OpenDrive")
    }
}
