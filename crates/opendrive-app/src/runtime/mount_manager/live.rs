//! Live OS mount backend (Windows CFAPI sync roots and virtual drive letter mapping).

use super::watchdog::unregister_all_os_roots;
pub use super::watchdog::{
    cleanup_os_mounts, cleanup_os_mounts_for_pid, install_process_mount_guards,
    is_foreign_mount_owner_alive, spawn_mount_watchdog,
};
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
            spawn_mount_watchdog(path.parent().unwrap_or(path), None);
            Ok(Box::new(conn))
        }
        #[cfg(not(windows))]
        {
            let _ = (acct, root_node, engine, rt);
            Ok(Box::new(()))
        }
    }

    fn unregister_account(&self, path: &Path) {
        #[cfg(windows)]
        opendrive_fs_windows::unregister_os_sync_root(path);
        #[cfg(not(windows))]
        let _ = path;
        opendrive_platform::notify_shell_mounts_changed();
    }

    fn unregister_all_accounts(&self) {
        if is_foreign_mount_owner_alive() {
            return;
        }
        unregister_all_os_roots();
        opendrive_platform::notify_shell_mounts_changed();
    }

    fn reconcile_placeholders(&self, path: &Path, rel_dirs: &[PathBuf]) {
        #[cfg(windows)]
        opendrive_fs_windows::CloudFilesFrontend::reconcile_placeholders(path, rel_dirs);
        #[cfg(not(windows))]
        let _ = (path, rel_dirs);
    }

    fn reconcile_placeholder_entries(&self, path: &Path, entries: &[(PathBuf, u64, bool)]) {
        #[cfg(windows)]
        opendrive_fs_windows::CloudFilesFrontend::reconcile_placeholder_entries(path, entries);
        #[cfg(not(windows))]
        let _ = (path, entries);
    }

    fn set_virtual_drive(&self, enabled: bool, letter: &str, root: &Path) -> Result<(), String> {
        self.set_virtual_drive_with_quota(enabled, letter, root, None)
            .map(|_| ())
    }

    fn set_virtual_drive_with_quota(
        &self,
        enabled: bool,
        letter: &str,
        root: &Path,
        quota: Option<opendrive_platform::ExplorerQuota>,
    ) -> Result<Option<char>, String> {
        opendrive_platform::normalize_drive_letter(letter).map_err(|e| e.to_string())?;
        let chosen = if enabled {
            let _ = opendrive_platform::data_dir()
                .ok()
                .and_then(|d| crate::design::logo::ensure_logo_ico(&d));
            let ch = opendrive_platform::mount_virtual_drive_with_quota(letter, root, quota)
                .map_err(|e| e.to_string())?;
            spawn_mount_watchdog(root, Some(&ch.to_string()));
            Some(ch)
        } else {
            if opendrive_platform::is_virtual_drive_supported() {
                opendrive_platform::unmount_virtual_drive_for_target(letter, root)
                    .map_err(|e| e.to_string())?;
            }
            None
        };
        if cfg!(windows) {
            let _ = opendrive_platform::sync_this_pc_namespace(false, root, None, None);
        }
        Ok(chosen)
    }

    fn dehydrate_path(&self, path: &Path) {
        #[cfg(windows)]
        dehydrate_tree_windows(path);
        #[cfg(not(windows))]
        let _ = path;
    }

    fn cleanup_legacy_subst(&self, letter: &str, root: &Path) -> Result<(), String> {
        opendrive_platform::normalize_drive_letter(letter).map_err(|e| e.to_string())?;
        if !opendrive_platform::is_virtual_drive_supported() {
            return Ok(());
        }
        opendrive_platform::unmount_virtual_drive_for_target(letter, root)
            .map_err(|e| e.to_string())
    }

    fn sync_this_pc_namespace(
        &self,
        enabled: bool,
        root: &Path,
        quota: Option<opendrive_platform::ExplorerQuota>,
    ) -> Result<(), String> {
        let ico = opendrive_platform::data_dir()
            .ok()
            .and_then(|d| crate::design::logo::ensure_logo_ico(&d));
        opendrive_platform::sync_this_pc_namespace(enabled, root, ico.as_deref(), quota)
            .map_err(|e| e.to_string())?;
        if enabled {
            spawn_mount_watchdog(root, None);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn dehydrate_tree_windows(path: &Path) {
    if path.is_file() {
        let _ = opendrive_fs_windows::try_dehydrate_placeholder(path);
    } else if let Ok(rd) = std::fs::read_dir(path) {
        rd.flatten().for_each(|e| dehydrate_tree_windows(&e.path()));
    }
}
