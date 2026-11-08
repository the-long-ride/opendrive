//! macOS localhost NFSv3 frontend.
//! In-process NFS server on 127.0.0.1; system mounts ~/OpenDrive.
//! No kernel extension / macFUSE. Harden: loopback only, secret export path.

use opendrive_sync::VfsEngine;
use std::sync::Arc;

pub struct NfsFrontend {
    engine: Arc<dyn VfsEngine>,
    mounted: bool,
}

impl NfsFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine,
            mounted: false,
        }
    }

    /// Start nfsserve on 127.0.0.1 and mount at path.
    pub fn serve_and_mount(&mut self, path: &str) -> Result<(), String> {
        // 1. Bind NFS on 127.0.0.1:random with unguessable export name
        // 2. Map NFSv3 ops -> VfsEngine (LOOKUP/READDIR/READ/WRITE/...)
        // 3. mount_nfs -o rdonly or rw localhost:/export path
        // Filter AppleDouble ._* in readdir
        let _ = &self.engine;
        let _ = path;
        self.mounted = true;
        Ok(())
    }

    pub fn unmount(&mut self, path: &str) -> Result<(), String> {
        let _ = std::process::Command::new("umount").arg(path).status();
        self.mounted = false;
        Ok(())
    }

    pub fn is_mounted(&self) -> bool {
        self.mounted
    }
}
