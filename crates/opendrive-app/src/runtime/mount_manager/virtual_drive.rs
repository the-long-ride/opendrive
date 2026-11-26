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
        let desired = (
            settings.virtual_drive_enabled,
            settings.virtual_drive_letter.clone(),
            quota,
        );
        let prev = self.last_virtual_drive.lock().expect("vd lock").clone();
        if prev.as_ref() == Some(&desired) {
            return Ok(());
        }
        if let Some((_, ref old_letter, _)) = prev {
            if old_letter != &desired.1 {
                let _ = self.backend.cleanup_legacy_subst(old_letter, root);
            }
        }
        if prev.is_none() || prev.as_ref().is_some_and(|(_, l, _)| l != &desired.1) {
            if desired.0 {
                self.backend
                    .cleanup_legacy_subst(&desired.1, root)
                    .map_err(|e| format!("Virtual drive {}: {e}", desired.1))?;
            } else {
                let _ = self.backend.cleanup_legacy_subst(&desired.1, root);
            }
        } else if desired.0 {
            opendrive_platform::normalize_drive_letter(&desired.1)
                .map_err(|e| format!("Virtual drive {}: {e}", desired.1))?;
        }
        self.backend
            .sync_this_pc_namespace(desired.0, root, desired.2)
            .map_err(|e| format!("This PC entry: {e}"))?;
        *self.last_virtual_drive.lock().expect("vd lock") = Some(desired);
        Ok(())
    }
}
