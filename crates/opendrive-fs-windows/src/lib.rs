//! Windows Cloud Files API frontend.
//! One sync root per account under %USERPROFILE%\OpenDrive\ (siblings).

use opendrive_sync::VfsEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Maps CFAPI callbacks to VfsEngine (design notes for M2 implementation).
///
/// | CFAPI callback        | VfsEngine              |
/// |-----------------------|------------------------|
/// | FETCH_PLACEHOLDERS    | list / lookup          |
/// | FETCH_DATA            | read (hydrate)         |
/// | ON_CLOSE_COMPLETION   | enqueue upload if dirty|
/// | VALIDATE_DATA         | md5 / size check       |
/// | CANCEL_FETCH_*        | cancel in-flight read  |
#[allow(dead_code)]
pub struct CallbackMap;

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

    pub fn register_sync_root(&mut self, path: &str, account_name: &str) -> Result<(), String> {
        let p = PathBuf::from(path);
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        // On Windows + cloud-filter:
        //   SyncFilter::new(...).register(path, identity)
        //   identity includes account_name for Explorer pane
        tracing::info!(%path, account_name, "register sync root (stub)");
        let _ = &self.engine;
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
        dirs_fallback_home().join("OpenDrive")
    }
}

fn dirs_fallback_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
