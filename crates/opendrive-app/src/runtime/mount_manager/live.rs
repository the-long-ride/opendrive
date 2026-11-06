//! Live OS mount backend (Windows CFAPI sync roots and virtual drive letter mapping).

use super::MountBackend;
use opendrive_core::Account;
use opendrive_sync::VfsEngine;
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct LiveMountBackend;

impl MountBackend for LiveMountBackend {
    fn prepare_root(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())
    }

    fn connect_account(
        &self,
        acct: &Account,
        path: &Path,
        root_node: u64,
        engine: Arc<dyn VfsEngine>,
        rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn Any + Send + Sync>, String> {
        self.prepare_root(path)?;
        #[cfg(windows)]
        {
            let mut frontend = opendrive_fs_windows::CloudFilesFrontend::new(engine);
            if let Some(ico) = opendrive_platform::data_dir()
                .ok()
                .and_then(|d| crate::design::logo::ensure_logo_ico(&d))
            {
                frontend = frontend.with_icon_resource(format!("{},0", ico.display()));
            }
            let conn = frontend.connect_live_at(
                &path.to_string_lossy(),
                &acct.nickname,
                root_node,
                rt.clone(),
            )?;
            Ok(Box::new(conn))
        }
        #[cfg(not(windows))]
        {
            let _ = (acct, root_node, engine, rt);
            Ok(Box::new(()))
        }
    }

    fn reconcile_placeholders(&self, path: &Path, rel_dirs: &[PathBuf]) {
        #[cfg(windows)]
        opendrive_fs_windows::CloudFilesFrontend::reconcile_placeholders(path, rel_dirs);
        #[cfg(not(windows))]
        let _ = (path, rel_dirs);
    }

    fn set_virtual_drive(&self, enabled: bool, letter: &str, root: &Path) -> Result<(), String> {
        if enabled {
            opendrive_platform::mount_virtual_drive(letter, root)
                .map(|_| ())
                .map_err(|e| e.to_string())
        } else {
            match opendrive_platform::unmount_virtual_drive(letter) {
                Ok(()) => Ok(()),
                Err(_) if !opendrive_platform::is_virtual_drive_supported() => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
