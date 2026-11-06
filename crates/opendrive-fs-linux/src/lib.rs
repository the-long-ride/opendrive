//! FUSE3 frontend (Linux).

#[cfg(target_os = "linux")]
mod fuse_helpers;
#[cfg(target_os = "linux")]
mod fuse_sys;
mod inode_table;

pub use inode_table::InodeTable;

use opendrive_sync::VfsEngine;
use std::sync::Arc;

#[cfg(target_os = "linux")]
pub use fuse_helpers::mount;

#[cfg(not(target_os = "linux"))]
pub fn mount(_engine: Arc<dyn VfsEngine>, _path: &str) -> Result<(), String> {
    Err("FUSE only on Linux".into())
}

pub struct FuseFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl FuseFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    pub fn mount(&self, path: &str) -> Result<(), String> {
        mount(self.engine.clone(), path)
    }

    pub fn unmount(&self, path: &str) -> Result<(), String> {
        let _ = std::process::Command::new("fusermount3")
            .args(["-u", path])
            .status();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_sync::PlaceholderEngine;

    #[test]
    fn frontend_new_and_unmount_succeed() {
        let frontend = FuseFrontend::new(Arc::new(PlaceholderEngine));
        assert!(frontend.unmount("/tmp/nonexistent-opendrive-mount").is_ok());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn mount_returns_error_on_non_linux() {
        let frontend = FuseFrontend::new(Arc::new(PlaceholderEngine));
        let err = frontend
            .mount("/tmp/nonexistent-opendrive-mount")
            .unwrap_err();
        assert!(err.contains("Linux"), "unexpected error: {err}");
    }
}
