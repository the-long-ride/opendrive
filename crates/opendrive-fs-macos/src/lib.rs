//! macOS localhost NFSv3 frontend.

mod nfs_backend;

pub use nfs_backend::NfsBackend;

use opendrive_sync::VfsEngine;
use std::path::PathBuf;
use std::sync::Arc;

pub struct NfsFrontend {
    engine: Arc<dyn VfsEngine>,
    backend: Option<NfsBackend>,
    mounted: bool,
    export_name: String,
}

impl NfsFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine: engine.clone(),
            backend: Some(NfsBackend::new(engine)),
            mounted: false,
            export_name: format!("od-{}", simple_id()),
        }
    }

    pub fn serve_and_mount(&mut self, path: &str) -> Result<(), String> {
        // Live path (M3 on macOS host):
        // 1. nfsserve::Server::new on 127.0.0.1:0 with NfsBackend
        // 2. mount_nfs -o resvport,rw 127.0.0.1:/<export> path
        tracing::info!(%path, export = %self.export_name, "NFS serve_and_mount");
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        let _ = &self.backend;
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

    pub fn default_mount_path() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OpenDrive")
    }

    pub fn backend(&self) -> Option<&NfsBackend> {
        self.backend.as_ref()
    }
}

fn simple_id() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{:x}", d.as_nanos()))
        .unwrap_or_else(|_| "0".into())
}
