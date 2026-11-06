//! macOS localhost NFSv3 frontend.
//! In-process NFS server on 127.0.0.1; system mounts ~/OpenDrive.
//! No kernel extension / macFUSE. Harden: loopback only, secret export path.

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
        // nfsserve on loopback + mount_nfs
        let _ = &self.engine;
        Ok(())
    }

    pub fn unmount(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}
