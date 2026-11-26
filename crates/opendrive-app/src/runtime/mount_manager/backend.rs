//! Mount backend trait and no-op test backend implementation.

use crate::runtime::bridge_types::str_err;
use opendrive_core::Account;
use opendrive_sync::VfsEngine;
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub trait MountBackend: Send + Sync {
    fn prepare_root(&self, path: &Path) -> Result<(), String>;
    fn connect_account(
        &self,
        acct: &Account,
        path: &Path,
        root_node: u64,
        engine: Arc<dyn VfsEngine>,
        rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn Any + Send + Sync>, String>;
    fn unregister_account(&self, path: &Path) {
        let _ = path;
    }
    fn unregister_all_accounts(&self) {}
    fn reconcile_placeholders(&self, path: &Path, rel_dirs: &[PathBuf]) {
        let _ = (path, rel_dirs);
    }
    fn set_virtual_drive(&self, enabled: bool, letter: &str, root: &Path) -> Result<(), String>;
    fn dehydrate_path(&self, path: &Path) {
        let _ = path;
    }
    fn cleanup_legacy_subst(&self, letter: &str, root: &Path) -> Result<(), String> {
        self.set_virtual_drive(false, letter, root)
    }
    fn sync_this_pc_namespace(
        &self,
        enabled: bool,
        root: &Path,
        quota: Option<opendrive_platform::ExplorerQuota>,
    ) -> Result<(), String> {
        let _ = (enabled, root, quota);
        Ok(())
    }
}

pub struct NullMountBackend;

impl MountBackend for NullMountBackend {
    fn prepare_root(&self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    fn connect_account(
        &self,
        _acct: &Account,
        _path: &Path,
        _root_node: u64,
        _engine: Arc<dyn VfsEngine>,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn Any + Send + Sync>, String> {
        Ok(Box::new(()))
    }

    fn set_virtual_drive(&self, _enabled: bool, letter: &str, _root: &Path) -> Result<(), String> {
        opendrive_platform::normalize_drive_letter(letter)
            .map(|_| ())
            .map_err(str_err)
    }
}
