//! Virtual drive and Windows This PC Explorer quota synchronization helpers.

use super::MountManager;
use opendrive_core::{Account, Settings};
use opendrive_platform::{compute_explorer_quota, ExplorerQuota};
use std::path::Path;

pub(super) fn compute_accounts_quota(accounts: &[Account]) -> Option<ExplorerQuota> {
    let pairs: Vec<(u64, u64)> = accounts
        .iter()
        .map(|a| (a.storage_used, a.storage_quota))
        .collect();
    compute_explorer_quota(&pairs)
}

impl MountManager {
    pub(super) fn sync_virtual_drive(
        &self,
        settings: &Settings,
        quota: Option<ExplorerQuota>,
        root: &Path,
    ) -> Result<(), String> {
        let desired_enabled = settings.virtual_drive_enabled;
        let desired_letter = settings.virtual_drive_letter.clone();
        let prev = self.last_virtual_drive.lock().expect("vd lock").clone();
        if let Some((prev_on, ref prev_req, _, prev_quota)) = prev {
            if prev_on == desired_enabled
                && prev_quota == quota
                && desired_letter.eq_ignore_ascii_case(prev_req)
            {
                return Ok(());
            }
        }
        if let Some((_, _, ref pa, _)) = prev {
            if opendrive_platform::normalize_drive_letter(pa).is_ok() {
                let _ = self.backend.cleanup_legacy_subst(pa, root);
            }
        }
        if opendrive_platform::normalize_drive_letter(&desired_letter).is_ok() {
            let _ = self.backend.cleanup_legacy_subst(&desired_letter, root);
        }
        let target_quota = desired_enabled.then_some(quota).flatten();
        self.backend
            .sync_this_pc_namespace(desired_enabled, root, target_quota)
            .map_err(|e| format!("This PC entry: {e}"))?;
        *self.last_virtual_drive.lock().expect("vd lock") = Some((
            desired_enabled,
            desired_letter.clone(),
            desired_letter,
            quota,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::MountBackend;
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CustomVdBackend {
        fail_ns: bool,
        ns_calls: Mutex<Vec<(bool, Option<ExplorerQuota>)>>,
        subst_cleanups: Mutex<Vec<String>>,
    }

    impl MountBackend for CustomVdBackend {
        fn prepare_root(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn connect_account(
            &self,
            _: &Account,
            _: &Path,
            _: u64,
            _: Arc<dyn opendrive_sync::VfsEngine>,
            _: &tokio::runtime::Handle,
        ) -> Result<Box<dyn std::any::Any + Send + Sync>, String> {
            Ok(Box::new(()))
        }
        fn set_virtual_drive(&self, _: bool, _: &str, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn cleanup_legacy_subst(&self, letter: &str, _: &Path) -> Result<(), String> {
            self.subst_cleanups.lock().unwrap().push(letter.into());
            Ok(())
        }
        fn sync_this_pc_namespace(
            &self,
            enabled: bool,
            _: &Path,
            quota: Option<ExplorerQuota>,
        ) -> Result<(), String> {
            self.ns_calls.lock().unwrap().push((enabled, quota));
            if self.fail_ns {
                Err("blocked".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn sync_virtual_drive_covers_this_pc_quota_cleanup_and_error() {
        let tmp = std::env::temp_dir().join(format!("od-vd-unit-{}", uuid::Uuid::new_v4()));
        let be = Arc::new(CustomVdBackend::default());
        let mm = MountManager::new(Arc::clone(&be) as Arc<dyn MountBackend>)
            .with_mount_root(tmp.clone());
        let s_on = Settings {
            virtual_drive_enabled: true,
            virtual_drive_letter: "O".into(),
            ..Default::default()
        };
        let q = Some(ExplorerQuota {
            used: 10,
            total: 100,
            free: 90,
        });
        mm.sync_virtual_drive(&s_on, q, &tmp).unwrap();
        mm.sync_virtual_drive(&s_on, q, &tmp).unwrap();
        assert_eq!(be.ns_calls.lock().unwrap().len(), 1);
        assert_eq!(be.subst_cleanups.lock().unwrap().as_slice(), ["O"]);

        let s_off = Settings {
            virtual_drive_enabled: false,
            virtual_drive_letter: "P".into(),
            ..Default::default()
        };
        mm.sync_virtual_drive(&s_off, q, &tmp).unwrap();
        assert_eq!(be.ns_calls.lock().unwrap().last(), Some(&(false, None)));

        let mm_err = MountManager::new(Arc::new(CustomVdBackend {
            fail_ns: true,
            ..Default::default()
        }))
        .with_mount_root(tmp.clone());
        assert!(mm_err
            .sync_virtual_drive(&s_on, None, &tmp)
            .unwrap_err()
            .contains("This PC entry"));
    }
}
