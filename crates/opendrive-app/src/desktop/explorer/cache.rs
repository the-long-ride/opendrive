//! Per-account Cloud Explorer visited folder path cache state transitions.

use super::AccountExplorerSession;
use crate::runtime::explorer_ops::ExplorerEntry;
use opendrive_sync::NodeId;

impl AccountExplorerSession {
    pub(crate) fn invalidate_cache(&mut self) {
        self.cache_epoch = self.cache_epoch.wrapping_add(1);
        self.folder_cache.clear();
    }

    pub(crate) fn record_folder_entries(
        &mut self,
        folder_id: NodeId,
        entries: Vec<ExplorerEntry>,
        cache_on: bool,
        epoch: Option<u64>,
    ) {
        if !cache_on {
            self.folder_cache.clear();
        } else if epoch.is_none_or(|e| e == self.cache_epoch) {
            self.folder_cache.insert(folder_id, entries.clone());
        }
        if self.breadcrumbs.last().map(|(id, _)| *id) == Some(folder_id) {
            (self.entries, self.loaded) = (entries, true);
            self.prune_to_entries();
        }
    }

    pub(crate) fn apply_cached_navigation(&mut self, folder_id: NodeId, cache_on: bool) -> bool {
        if cache_on {
            if let Some(entries) = self.folder_cache.get(&folder_id).cloned() {
                (self.entries, self.loaded) = (entries, true);
                self.prune_to_entries();
                return true;
            }
        } else {
            self.folder_cache.clear();
        }
        self.entries.clear();
        self.loaded = false;
        false
    }
}
