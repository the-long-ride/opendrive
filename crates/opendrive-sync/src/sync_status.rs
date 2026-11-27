//! Computes hierarchical Explorer sync status (`Cloud`, `Indexed`, `PartialLocal`, `FullLocal`).

use crate::engine::SyncEngine;
use opendrive_store::{Node, NodeState};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerSyncStatus {
    #[default]
    Cloud,
    Indexed,
    PartialLocal,
    FullLocal,
    PendingUpload,
    Conflict,
    Syncing,
}

impl SyncEngine {
    /// Classifies a file or folder for Explorer UI badges:
    /// - Folders with `NodeState::Cached` whose children are only cloud placeholders are `Indexed`.
    /// - Folders mixing local files and remaining cloud/unindexed items are `PartialLocal`.
    /// - Folders with all child files synced locally (or an explicitly downloaded empty folder) are `FullLocal`.
    pub fn compute_sync_status(&self, node: &Node) -> ExplorerSyncStatus {
        if !node.is_folder() {
            return match node.state {
                NodeState::Placeholder => ExplorerSyncStatus::Cloud,
                NodeState::Cached => ExplorerSyncStatus::FullLocal,
                NodeState::Dirty => ExplorerSyncStatus::PendingUpload,
                NodeState::Conflict => ExplorerSyncStatus::Conflict,
                NodeState::Hydrating => ExplorerSyncStatus::Syncing,
            };
        }
        match node.state {
            NodeState::Conflict => return ExplorerSyncStatus::Conflict,
            NodeState::Dirty => return ExplorerSyncStatus::PendingUpload,
            NodeState::Hydrating => return ExplorerSyncStatus::Syncing,
            NodeState::Placeholder | NodeState::Cached => {}
        }

        let store = self.store.lock();
        let mut queue = VecDeque::from([node.id]);
        let mut seen = HashSet::from([node.id]);
        let mut local_files = 0usize;
        let mut has_cloud_file = false;
        let mut has_unindexed_subfolder = false;

        while let Some(dir_id) = queue.pop_front() {
            let Ok(children) = store.list_children(dir_id) else {
                break;
            };
            for child in children {
                if child.is_folder() {
                    if child.state == NodeState::Placeholder {
                        has_unindexed_subfolder = true;
                    }
                    if seen.insert(child.id) {
                        queue.push_back(child.id);
                    }
                } else if child.kind == "file" {
                    match child.state {
                        NodeState::Cached | NodeState::Dirty => local_files += 1,
                        NodeState::Placeholder | NodeState::Hydrating => has_cloud_file = true,
                        NodeState::Conflict => return ExplorerSyncStatus::Conflict,
                    }
                }
            }
        }

        if local_files > 0 && (has_cloud_file || has_unindexed_subfolder) {
            ExplorerSyncStatus::PartialLocal
        } else if node.state == NodeState::Cached
            && !has_unindexed_subfolder
            && !has_cloud_file
            && (local_files > 0 || node.pinned)
        {
            ExplorerSyncStatus::FullLocal
        } else if local_files > 0 {
            ExplorerSyncStatus::PartialLocal
        } else if node.state == NodeState::Cached {
            ExplorerSyncStatus::Indexed
        } else {
            ExplorerSyncStatus::Cloud
        }
    }
}
