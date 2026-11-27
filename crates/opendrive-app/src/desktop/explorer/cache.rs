//! Per-account Cloud Explorer visited folder path cache state transitions.

use super::{AccountExplorerSession, ExplorerScope};
use crate::runtime::explorer_ops::ExplorerEntry;
use opendrive_sync::NodeId;
use uuid::Uuid;

impl AccountExplorerSession {
    pub(crate) fn apply_scope_switch(&mut self, root_id: NodeId, scope: ExplorerScope) {
        self.scope = scope;
        self.breadcrumbs = vec![(root_id, scope.label().into())];
        self.search_query.clear();
        self.clear_selection();
    }

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

impl super::ExplorerState {
    pub fn active_session(&self) -> Option<&AccountExplorerSession> {
        self.active_account_id.and_then(|id| self.sessions.get(&id))
    }

    pub fn active_session_mut(&mut self) -> Option<&mut AccountExplorerSession> {
        self.active_account_id
            .and_then(|id| self.sessions.get_mut(&id))
    }

    pub fn current_folder_id(&self, account_id: Uuid) -> Option<opendrive_sync::NodeId> {
        self.sessions
            .get(&account_id)
            .and_then(|s| s.breadcrumbs.last().map(|(id, _)| *id))
    }

    pub fn active_account_and_folder(&self) -> Option<(Uuid, opendrive_sync::NodeId)> {
        let aid = self.active_account_id?;
        self.current_folder_id(aid).map(|fid| (aid, fid))
    }

    pub fn is_busy(&self, account_id: Uuid) -> bool {
        self.busy_accounts.get(&account_id).copied().unwrap_or(0) > 0
    }

    pub fn inc_busy(&mut self, account_id: Uuid) {
        *self.busy_accounts.entry(account_id).or_insert(0) += 1;
    }

    pub fn dec_busy(&mut self, account_id: Uuid) {
        if let Some(count) = self.busy_accounts.get_mut(&account_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.busy_accounts.remove(&account_id);
            }
        }
    }

    pub fn prune_clipboard_ids(&mut self, account_id: Uuid, ids: &[opendrive_sync::NodeId]) {
        let Some(clip) = self
            .clipboard
            .as_mut()
            .filter(|c| c.account_id == account_id)
        else {
            return;
        };
        clip.node_ids.retain(|id| !ids.contains(id));
        if clip.node_ids.is_empty() {
            self.clipboard = None;
        } else if clip.node_ids.len() > 1 {
            clip.name = format!("{} items", clip.node_ids.len());
        }
    }
}

impl crate::desktop::ControlCenterApp {
    pub(crate) fn switch_explorer_scope(&mut self, aid: Uuid, scope: ExplorerScope) {
        let res = match scope {
            ExplorerScope::MyDrive => self.bridge.explorer_root_node(aid),
            ExplorerScope::SharedWithMe => self.bridge.explorer_shared_with_me_node(aid),
        };
        match res {
            Ok(fid) => {
                self.explorer
                    .sessions
                    .entry(aid)
                    .or_default()
                    .apply_scope_switch(fid, scope);
                self.navigate_explorer_folder(aid, fid);
            }
            Err(err) => self.set_err(err),
        }
    }
}
