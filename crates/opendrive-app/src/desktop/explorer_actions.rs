//! UI action handlers, drop upload, and job polling for Cloud Explorer.

use super::explorer::handle_explorer_file_op_action;
use super::explorer::jobs::ExplorerJobEvent;
use super::{ControlCenterApp, UiAction};
use uuid::Uuid;

impl ControlCenterApp {
    pub(crate) fn poll_explorer_jobs(&mut self) {
        let mut events = Vec::new();
        while let Ok(ev) = self.explorer_rx.try_recv() {
            events.push(ev);
        }
        for ev in events {
            match ev {
                ExplorerJobEvent::Listed {
                    account_id,
                    folder_id,
                    entries,
                    epoch,
                } => {
                    self.explorer.dec_busy(account_id);
                    let cache_on = self.state.settings.explorer_folder_cache;
                    let session = self.explorer.sessions.entry(account_id).or_default();
                    if session.breadcrumbs.is_empty() {
                        session.breadcrumbs.push((folder_id, "My Drive".into()));
                    }
                    session.record_folder_entries(folder_id, entries, cache_on, Some(epoch));
                }
                ExplorerJobEvent::Mutated {
                    account_id,
                    folder_id,
                    message,
                } => {
                    self.explorer.dec_busy(account_id);
                    self.state = self.bridge.state_snapshot();
                    self.set_ok(message);
                    self.invalidate_account_folder_cache(account_id);
                    let target = folder_id.or_else(|| self.explorer.current_folder_id(account_id));
                    self.spawn_explorer_list(account_id, target, true);
                }
                ExplorerJobEvent::BatchFinished {
                    account_id,
                    folder_id,
                    outcome,
                    deleted_ids,
                    moved_ids,
                } => {
                    self.explorer.dec_busy(account_id);
                    if !deleted_ids.is_empty() {
                        if let Some(s) = self.explorer.sessions.get_mut(&account_id) {
                            s.remove_selected(&deleted_ids);
                        }
                        self.explorer.prune_clipboard_ids(account_id, &deleted_ids);
                    }
                    if !moved_ids.is_empty() {
                        self.explorer.prune_clipboard_ids(account_id, &moved_ids);
                    }
                    if !outcome.succeeded_ids.is_empty() || outcome.error.is_none() {
                        self.state = self.bridge.state_snapshot();
                        self.invalidate_account_folder_cache(account_id);
                        let target =
                            folder_id.or_else(|| self.explorer.current_folder_id(account_id));
                        self.spawn_explorer_list(account_id, target, true);
                    }
                    if let Some(err) = outcome.error {
                        self.set_err(err);
                    } else {
                        self.set_ok(outcome.message);
                    }
                }
                ExplorerJobEvent::Opened {
                    account_id,
                    resolved,
                } => {
                    self.explorer.dec_busy(account_id);
                    self.state = self.bridge.state_snapshot();
                    if let Some(fid) = self.explorer.current_folder_id(account_id) {
                        self.sync_local_folder_entries(account_id, fid);
                    }
                    if let Err(err) = (self.auth_endpoints.item_launcher)(
                        resolved.app_command.as_deref(),
                        &resolved.target,
                    ) {
                        self.set_err(err);
                    } else {
                        let msg = if resolved.is_stream_url {
                            "Streaming video to player"
                        } else {
                            "Opened item"
                        };
                        self.set_ok(msg);
                    }
                }
                ExplorerJobEvent::StreamUrl { account_id, url } => {
                    self.explorer.dec_busy(account_id);
                    self.pending_clipboard = Some(url);
                    self.set_ok("Copied stream URL to clipboard");
                }
                ExplorerJobEvent::Failed { account_id, error } => {
                    self.explorer.dec_busy(account_id);
                    self.set_err(error);
                }
            }
        }
    }

    fn open_account_explorer(&mut self, id_str: &str) {
        if id_str.trim().is_empty() {
            if let Some(first) = self.state.accounts.first().map(|a| a.id.to_string()) {
                return self.open_account_explorer(&first);
            }
            self.explorer.open = true;
            self.explorer.active_account_id = None;
            return;
        }
        let Ok(acct_id) = Uuid::parse_str(id_str.trim()) else {
            self.set_err("Invalid account ID");
            return;
        };
        if !self.state.accounts.iter().any(|a| a.id == acct_id) {
            self.set_err("Account not found");
            return;
        }
        self.explorer.open = true;
        self.explorer.active_account_id = Some(acct_id);
        if self
            .explorer
            .sessions
            .get(&acct_id)
            .is_some_and(|s| s.loaded)
        {
            if !self.state.settings.explorer_folder_cache {
                if let Some(fid) = self.explorer.current_folder_id(acct_id) {
                    self.navigate_explorer_folder(acct_id, fid);
                }
            }
            return;
        }
        match self.bridge.explorer_root_node(acct_id) {
            Ok(root_id) => {
                let session = self.explorer.sessions.entry(acct_id).or_default();
                if session.breadcrumbs.is_empty() {
                    session.breadcrumbs.push((root_id, "My Drive".into()));
                }
                let target = session.breadcrumbs.last().map(|(id, _)| *id);
                self.spawn_explorer_list(acct_id, target, false);
            }
            Err(err) => self.set_err(err),
        }
    }

    pub(crate) fn handle_explorer_action(&mut self, action: UiAction) {
        let acct_id = self.explorer.active_account_id;
        match action {
            UiAction::OpenExplorer(id) | UiAction::ExplorerSelectAccount(id) => {
                self.open_account_explorer(&id);
            }
            UiAction::CloseExplorer => self.explorer.open = false,
            UiAction::ExplorerSetViewMode(mode) => self.explorer.view_mode = mode,
            UiAction::ExplorerToggleSort(col) => {
                if let Some(s) = self.explorer.active_session_mut() {
                    s.toggle_sort(col);
                }
            }
            UiAction::ExplorerSelectEntry(sel) => {
                if let Some(s) = self.explorer.active_session_mut() {
                    s.select_only(sel);
                }
            }
            UiAction::ExplorerToggleSelect(nid) => {
                if let Some(s) = self.explorer.active_session_mut() {
                    s.toggle_select(nid);
                }
            }
            UiAction::ExplorerSelectRange { node_id, additive } => {
                if let Some(s) = self.explorer.active_session_mut() {
                    s.select_range(node_id, additive);
                }
            }
            UiAction::ExplorerSelectAll => {
                if let Some(s) = self.explorer.active_session_mut() {
                    s.select_all_visible();
                }
            }
            UiAction::ExplorerNavigateNode(node_id) => {
                let Some(aid) = acct_id else { return };
                if let Some(s) = self.explorer.sessions.get_mut(&aid) {
                    let name = s
                        .entries
                        .iter()
                        .find(|e| e.id == node_id)
                        .map_or_else(|| "Folder".into(), |e| e.name.clone());
                    if s.breadcrumbs.last().map(|(id, _)| *id) != Some(node_id) {
                        s.breadcrumbs.push((node_id, name));
                    }
                    s.clear_selection();
                }
                self.navigate_explorer_folder(aid, node_id);
            }
            UiAction::ExplorerNavigateBreadcrumb(idx) => {
                let Some(aid) = acct_id else { return };
                let target = self.explorer.sessions.get_mut(&aid).and_then(|s| {
                    if idx < s.breadcrumbs.len() {
                        s.breadcrumbs.truncate(idx + 1);
                        s.clear_selection();
                        s.breadcrumbs.last().map(|(id, _)| *id)
                    } else {
                        None
                    }
                });
                if let Some(fid) = target {
                    self.navigate_explorer_folder(aid, fid);
                }
            }
            UiAction::ExplorerRefresh => {
                if let Some(aid) = acct_id {
                    self.invalidate_account_folder_cache(aid);
                    let fid = self.explorer.current_folder_id(aid);
                    self.spawn_explorer_list(aid, fid, true);
                }
            }
            UiAction::ExplorerSyncDown(nids) => {
                if let (Some(aid), false) = (acct_id, nids.is_empty()) {
                    self.spawn_explorer_sync_down(aid, nids);
                }
            }
            UiAction::ExplorerSyncUpItems(nids) => {
                if let (Some(aid), false) = (acct_id, nids.is_empty()) {
                    self.spawn_explorer_sync_up_items(aid, nids);
                }
            }
            UiAction::ExplorerSyncUpFiles => {
                let Some((aid, fid)) = self.explorer.active_account_and_folder() else {
                    return;
                };
                let paths = (self.auth_endpoints.upload_files_picker)();
                if !paths.is_empty() {
                    self.spawn_explorer_sync_up(aid, fid, paths);
                }
            }
            UiAction::ExplorerSyncUpFolder => {
                let Some((aid, fid)) = self.explorer.active_account_and_folder() else {
                    return;
                };
                if let Some(folder) = (self.auth_endpoints.upload_folder_picker)() {
                    self.spawn_explorer_sync_up(aid, fid, vec![folder]);
                }
            }
            UiAction::ExplorerSyncUpAndClear(nids) => {
                if let (Some(aid), false) = (acct_id, nids.is_empty()) {
                    self.spawn_explorer_clear(aid, nids);
                }
            }
            UiAction::ExplorerOpenItem(nid) => {
                let is_folder = self
                    .explorer
                    .active_session()
                    .and_then(|s| s.entries.iter().find(|e| e.id == nid))
                    .is_some_and(|e| e.is_folder);
                if is_folder {
                    self.handle_explorer_action(UiAction::ExplorerNavigateNode(nid));
                } else if let Some(aid) = acct_id {
                    self.spawn_explorer_open(aid, nid);
                }
            }
            UiAction::ExplorerCopyStreamUrl(nid) => {
                if let Some(aid) = acct_id {
                    self.spawn_explorer_stream_url(aid, nid);
                }
            }
            op @ (UiAction::ExplorerStartRename(_)
            | UiAction::ExplorerSaveRename
            | UiAction::ExplorerRequestDelete(_)
            | UiAction::ExplorerConfirmDelete
            | UiAction::ExplorerDismissModal
            | UiAction::ExplorerSetClipboard { .. }
            | UiAction::ExplorerClearClipboard
            | UiAction::ExplorerPasteClipboard(_)) => handle_explorer_file_op_action(self, op),
            _ => {}
        }
    }
}
