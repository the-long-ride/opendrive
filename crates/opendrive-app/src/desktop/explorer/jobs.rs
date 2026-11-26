//! Background mpsc workers for Cloud Explorer operations.

use crate::desktop::ControlCenterApp;
use crate::runtime::bridge::Bridge;
use crate::runtime::explorer_batch::BatchOutcome;
use crate::runtime::explorer_ops::{ExplorerEntry, ResolvedOpenTarget};
use eframe::egui;
use opendrive_sync::NodeId;
use std::path::PathBuf;
use std::sync::mpsc;
use uuid::Uuid;

pub(crate) enum ExplorerJobEvent {
    Listed {
        account_id: Uuid,
        folder_id: NodeId,
        entries: Vec<ExplorerEntry>,
        epoch: u64,
    },
    Mutated {
        account_id: Uuid,
        folder_id: Option<NodeId>,
        message: String,
    },
    BatchFinished {
        account_id: Uuid,
        folder_id: Option<NodeId>,
        outcome: BatchOutcome,
        deleted_ids: Vec<NodeId>,
        moved_ids: Vec<NodeId>,
    },
    Opened {
        account_id: Uuid,
        resolved: ResolvedOpenTarget,
    },
    StreamUrl {
        account_id: Uuid,
        url: String,
    },
    Failed {
        account_id: Uuid,
        error: String,
    },
}

fn send_job(
    tx: &mpsc::Sender<ExplorerJobEvent>,
    ctx: &Option<egui::Context>,
    event: ExplorerJobEvent,
) {
    let _ = tx.send(event);
    if let Some(c) = ctx {
        c.request_repaint();
    }
}

#[derive(Clone, Copy)]
enum BatchTrack {
    None,
    Deleted,
    Moved,
}

impl ControlCenterApp {
    fn spawn_explorer_job(
        &mut self,
        account_id: Uuid,
        work: impl FnOnce(Bridge) -> Result<ExplorerJobEvent, String> + Send + 'static,
    ) {
        self.explorer.inc_busy(account_id);
        let bridge = self.bridge.clone();
        let tx = self.explorer_tx.clone();
        let ctx = self.repaint_ctx.clone();
        std::thread::spawn(move || {
            let ev = match work(bridge) {
                Ok(ev) => ev,
                Err(error) => ExplorerJobEvent::Failed { account_id, error },
            };
            send_job(&tx, &ctx, ev);
        });
    }

    fn spawn_batch_job(
        &mut self,
        account_id: Uuid,
        track: BatchTrack,
        work: impl FnOnce(&Bridge) -> BatchOutcome + Send + 'static,
    ) {
        let folder_id = self.explorer.current_folder_id(account_id);
        self.spawn_explorer_job(account_id, move |bridge| {
            let outcome = work(&bridge);
            let (deleted_ids, moved_ids) = match track {
                BatchTrack::Deleted => (outcome.succeeded_ids.clone(), Vec::new()),
                BatchTrack::Moved => (Vec::new(), outcome.succeeded_ids.clone()),
                BatchTrack::None => (Vec::new(), Vec::new()),
            };
            Ok(ExplorerJobEvent::BatchFinished {
                account_id,
                folder_id,
                outcome,
                deleted_ids,
                moved_ids,
            })
        });
    }
}

impl ControlCenterApp {
    pub(crate) fn spawn_explorer_list(
        &mut self,
        account_id: Uuid,
        folder_id: Option<NodeId>,
        force_refresh: bool,
    ) {
        let epoch = self
            .explorer
            .sessions
            .get(&account_id)
            .map_or(0, |s| s.cache_epoch);
        self.spawn_explorer_job(account_id, move |bridge| {
            let (fid, entries) = bridge.explorer_list(account_id, folder_id, force_refresh)?;
            Ok(ExplorerJobEvent::Listed {
                account_id,
                folder_id: fid,
                entries,
                epoch,
            })
        });
    }

    pub(crate) fn invalidate_account_folder_cache(&mut self, account_id: Uuid) {
        if let Some(s) = self.explorer.sessions.get_mut(&account_id) {
            s.invalidate_cache();
        }
    }

    pub(crate) fn clear_all_explorer_folder_caches(&mut self) {
        for s in self.explorer.sessions.values_mut() {
            s.invalidate_cache();
        }
    }

    pub(crate) fn sync_local_folder_entries(&mut self, account_id: Uuid, folder_id: NodeId) {
        if let Ok(entries) = self.bridge.explorer_local_entries(folder_id) {
            let cache_on = self.state.settings.explorer_folder_cache;
            self.explorer
                .sessions
                .entry(account_id)
                .or_default()
                .record_folder_entries(folder_id, entries, cache_on, None);
        }
    }

    pub(crate) fn navigate_explorer_folder(&mut self, account_id: Uuid, folder_id: NodeId) {
        let cache_on = self.state.settings.explorer_folder_cache;
        let hit = self
            .explorer
            .sessions
            .entry(account_id)
            .or_default()
            .apply_cached_navigation(folder_id, cache_on);
        if !hit {
            self.spawn_explorer_list(account_id, Some(folder_id), true);
        }
    }

    pub(crate) fn handle_explorer_drop(&mut self, ctx: &egui::Context) {
        if !self.explorer.open {
            return;
        }
        let paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        let (Some((aid, fid)), false) =
            (self.explorer.active_account_and_folder(), paths.is_empty())
        else {
            return;
        };
        if self
            .state
            .accounts
            .iter()
            .any(|a| a.id == aid && a.read_only)
        {
            self.set_err("account is read-only");
        } else {
            self.spawn_explorer_sync_up(aid, fid, paths);
        }
    }

    pub(crate) fn spawn_explorer_sync_down(&mut self, account_id: Uuid, node_ids: Vec<NodeId>) {
        self.spawn_batch_job(account_id, BatchTrack::None, move |bridge| {
            bridge.explorer_sync_down_batch(account_id, &node_ids)
        });
    }

    pub(crate) fn spawn_explorer_sync_up_items(&mut self, account_id: Uuid, node_ids: Vec<NodeId>) {
        self.spawn_batch_job(account_id, BatchTrack::None, move |bridge| {
            bridge.explorer_sync_up_batch(account_id, &node_ids)
        });
    }

    pub(crate) fn spawn_explorer_sync_up(
        &mut self,
        account_id: Uuid,
        parent_node: NodeId,
        paths: Vec<PathBuf>,
    ) {
        self.spawn_explorer_job(account_id, move |bridge| {
            let sum = bridge.explorer_sync_up(account_id, parent_node, &paths)?;
            Ok(ExplorerJobEvent::Mutated {
                account_id,
                folder_id: Some(parent_node),
                message: format!("Uploaded {} file(s)", sum.uploaded_files),
            })
        });
    }

    pub(crate) fn spawn_explorer_clear(&mut self, account_id: Uuid, node_ids: Vec<NodeId>) {
        self.spawn_batch_job(account_id, BatchTrack::None, move |bridge| {
            bridge.explorer_sync_clear_batch(account_id, &node_ids)
        });
    }

    pub(crate) fn spawn_explorer_open(&mut self, account_id: Uuid, node_id: NodeId) {
        self.spawn_explorer_job(account_id, move |bridge| {
            let resolved = bridge.explorer_open_item(account_id, node_id, true)?;
            Ok(ExplorerJobEvent::Opened {
                account_id,
                resolved,
            })
        });
    }

    pub(crate) fn spawn_explorer_stream_url(&mut self, account_id: Uuid, node_id: NodeId) {
        self.spawn_explorer_job(account_id, move |bridge| {
            let url = bridge.explorer_stream_url(account_id, node_id)?;
            Ok(ExplorerJobEvent::StreamUrl { account_id, url })
        });
    }

    pub(crate) fn spawn_explorer_delete(&mut self, account_id: Uuid, node_ids: Vec<NodeId>) {
        self.spawn_batch_job(account_id, BatchTrack::Deleted, move |bridge| {
            bridge.explorer_delete_batch(account_id, &node_ids)
        });
    }

    pub(crate) fn spawn_explorer_rename(
        &mut self,
        account_id: Uuid,
        node_id: NodeId,
        new_name: String,
    ) {
        let folder_id = self.explorer.current_folder_id(account_id);
        self.spawn_explorer_job(account_id, move |bridge| {
            let entry = bridge.explorer_rename_item(account_id, node_id, &new_name)?;
            Ok(ExplorerJobEvent::Mutated {
                account_id,
                folder_id,
                message: format!("Renamed to '{}'", entry.name),
            })
        });
    }

    pub(crate) fn spawn_explorer_copy(
        &mut self,
        account_id: Uuid,
        node_ids: Vec<NodeId>,
        dest_folder: NodeId,
    ) {
        self.spawn_batch_job(account_id, BatchTrack::None, move |bridge| {
            bridge.explorer_copy_batch(account_id, &node_ids, dest_folder)
        });
    }

    pub(crate) fn spawn_explorer_move(
        &mut self,
        account_id: Uuid,
        node_ids: Vec<NodeId>,
        dest_folder: NodeId,
    ) {
        self.spawn_batch_job(account_id, BatchTrack::Moved, move |bridge| {
            bridge.explorer_move_batch(account_id, &node_ids, dest_folder)
        });
    }
}
