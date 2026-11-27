//! Bridge explorer operations: listing, sync-down, sync-up, verified clear, streaming, and open-with resolution.

use crate::runtime::bridge::{AppStateResponse, Bridge};
use crate::runtime::bridge_types::str_err;
use crate::runtime::explorer_helpers::{block_on_rt, node_to_entry_for, run_outside_tokio};
use crate::runtime::media_server::MediaStreamServer;
use opendrive_core::OpenWithRule;
use opendrive_sync::{ExplorerSyncStatus, NodeId, SyncSummary};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerEntry {
    pub id: opendrive_sync::NodeId,
    pub name: String,
    pub media_kind: opendrive_core::MediaKind,
    pub is_folder: bool,
    pub is_native_doc: bool,
    pub mime: Option<String>,
    pub size: u64,
    pub modified_ms: i64,
    pub state: opendrive_store::NodeState,
    pub sync_status: ExplorerSyncStatus,
    pub pinned: bool,
    pub drive_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOpenTarget {
    pub app_command: Option<String>,
    pub target: String,
    pub is_stream_url: bool,
    pub stream_url: Option<String>,
}

impl Bridge {
    pub(crate) fn ensure_account_drive_client(
        &self,
        account_id: Uuid,
        rt: &tokio::runtime::Handle,
    ) -> Result<(), String> {
        if self.engine.drive_client_for(account_id).is_ok() {
            return Ok(());
        }
        run_outside_tokio(|| {
            self.mounts.refresh_drive_client_for(
                account_id,
                &self.ctrl,
                &self.engine,
                &self.secrets,
                rt,
            )
        })
    }

    pub fn explorer_root_node(&self, account_id: Uuid) -> Result<NodeId, String> {
        let folder = self
            .ctrl
            .lock()
            .expect("controller lock")
            .accounts
            .get_account(account_id)
            .map_err(|_| "account not found".to_string())?
            .folder_name
            .clone();
        self.engine
            .seed_root_account(account_id, &folder)
            .map_err(str_err)
    }

    pub fn explorer_shared_with_me_node(&self, account_id: Uuid) -> Result<NodeId, String> {
        self.ctrl
            .lock()
            .expect("controller lock")
            .accounts
            .get_account(account_id)
            .map_err(|_| "account not found".to_string())?;
        self.engine
            .seed_shared_with_me_root(account_id)
            .map_err(str_err)
    }

    pub fn explorer_list(
        &self,
        account_id: Uuid,
        folder_node: Option<NodeId>,
        force_refresh: bool,
    ) -> Result<(NodeId, Vec<ExplorerEntry>), String> {
        let root = self.explorer_root_node(account_id)?;
        let target = folder_node.unwrap_or(root);
        let rt = self.mounts.ensure_runtime()?;
        if force_refresh {
            self.ensure_account_drive_client(account_id, rt.handle())?;
        } else {
            let _ = self.ensure_account_drive_client(account_id, rt.handle());
        }
        let nodes = block_on_rt(
            rt.handle(),
            self.engine
                .list_explorer_nodes(account_id, target, force_refresh),
        )
        .map_err(str_err)?;
        if self.mounts.is_account_mounted(account_id) {
            let entries = self.engine.local_reconcile_entries(root);
            self.mounts.reconcile_account_entries(account_id, &entries);
        }
        Ok((
            target,
            nodes
                .into_iter()
                .map(|n| node_to_entry_for(&self.engine, n))
                .collect(),
        ))
    }

    pub fn explorer_sync_down(
        &self,
        account_id: Uuid,
        node_id: NodeId,
    ) -> Result<(SyncSummary, Option<PathBuf>), String> {
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        let cache_dir = self.cache_root_dir();
        let res = block_on_rt(
            rt.handle(),
            self.engine.sync_down_node(
                account_id,
                root_node,
                node_id,
                mounted_root.as_deref(),
                &cache_dir,
            ),
        )
        .map_err(str_err)?;
        if self.mounts.is_account_mounted(account_id) {
            let entries = self.engine.local_reconcile_entries(root_node);
            self.mounts.reconcile_account_entries(account_id, &entries);
        }
        Ok(res)
    }

    pub fn explorer_sync_up(
        &self,
        account_id: Uuid,
        parent_node: NodeId,
        local_paths: &[PathBuf],
    ) -> Result<SyncSummary, String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let local_root = self.mounts.mounted_account_path(account_id);
        let mut total = SyncSummary::default();
        for path in local_paths {
            let one = block_on_rt(
                rt.handle(),
                self.engine.sync_up_local_path(
                    account_id,
                    parent_node,
                    path,
                    local_root.as_deref(),
                ),
            )
            .map_err(str_err)?;
            total.uploaded_files += one.uploaded_files;
            total.bytes_transferred += one.bytes_transferred;
        }
        total.uploaded_files +=
            block_on_rt(rt.handle(), self.engine.flush_account_uploads(account_id))
                .map_err(str_err)?;
        Ok(total)
    }

    pub fn explorer_sync_up_node(
        &self,
        account_id: Uuid,
        node_id: NodeId,
    ) -> Result<SyncSummary, String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        block_on_rt(
            rt.handle(),
            self.engine
                .sync_up_node(account_id, root_node, node_id, mounted_root.as_deref()),
        )
        .map_err(str_err)
    }

    pub fn explorer_sync_up_and_clear(
        &self,
        account_id: Uuid,
        node_id: NodeId,
    ) -> Result<SyncSummary, String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        let summary = block_on_rt(
            rt.handle(),
            self.engine.sync_up_and_clear_local(
                account_id,
                root_node,
                node_id,
                mounted_root.as_deref(),
            ),
        )
        .map_err(str_err)?;
        if let Some(local_p) = self.local_node_path(account_id, root_node, node_id) {
            if local_p.exists() {
                self.mounts.dehydrate_path(&local_p);
            }
        }
        Ok(summary)
    }

    pub fn explorer_stream_url(&self, account_id: Uuid, node_id: NodeId) -> Result<String, String> {
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let node = self.get_store_node(node_id)?;
        if node.is_folder() || node.is_native_doc() {
            return Err("only binary files can be streamed".into());
        }
        let name = node.effective_name().to_string();
        block_on_rt(rt.handle(), self.engine.prewarm_head_and_tail(node_id)).map_err(str_err)?;
        let mut guard = self.media_server.lock().expect("media server lock");
        if guard.is_none() {
            *guard = Some(MediaStreamServer::start(
                Arc::clone(&self.engine),
                rt.handle(),
            )?);
        }
        Ok(guard
            .as_ref()
            .expect("media server")
            .stream_url(node_id, &name))
    }

    pub fn update_open_with_rules(
        &self,
        rules: Vec<OpenWithRule>,
    ) -> Result<AppStateResponse, String> {
        let mut normalized = Vec::with_capacity(rules.len());
        for rule in &rules {
            let valid = opendrive_core::validate_open_with_rule(
                &rule.pattern,
                &rule.app_command,
                &normalized,
                None,
            )?;
            normalized.push(valid);
        }
        self.modify_settings(|s| s.open_with_rules = normalized, |_| Ok(()))
    }

    pub fn remove_open_with_rule(&self, index: usize) -> Result<AppStateResponse, String> {
        self.modify_settings(
            |s| {
                let _ = (index < s.open_with_rules.len()).then(|| s.open_with_rules.remove(index));
            },
            |_| Ok(()),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::ui::build_bridge_in_dirs;
    use opendrive_core::{AppController, Settings};

    #[test]
    fn open_item_and_stream_error_and_native_branches() {
        let br = build_bridge_in_dirs(AppController::new(Settings::default()), None, None).unwrap();
        assert!(br.explorer_root_node(uuid::Uuid::new_v4()).is_err());
        let snap = br
            .add_account("W".into(), "w@x.com".into(), "w".into(), false)
            .unwrap();
        let id = snap.accounts[0].id;
        let root = br.explorer_root_node(id).unwrap();
        assert!(br.explorer_open_item(id, 99999, true).is_err());
        assert!(br.explorer_open_item(id, root, true).is_err());
        assert!(br.explorer_stream_url(id, root).is_err());
        let st = br.engine().store();
        let mut n = st.lock().get_node(root as i64).unwrap().unwrap();
        (n.id, n.parent_id, n.kind, n.drive_id) =
            (0, Some(root as i64), "native_doc".into(), String::new());
        let nid = st.lock().upsert_node(&n).unwrap() as u64;
        assert!(br.explorer_open_item(id, nid, true).is_err());
        n.drive_id = "doc-1".into();
        let nid2 = st.lock().upsert_node(&n).unwrap() as u64;
        let res = br.explorer_open_item(id, nid2, true).unwrap();
        assert!(res.target.contains("doc-1"));
    }
}
