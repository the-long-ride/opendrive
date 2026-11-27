//! Low-level runtime helpers for `explorer_ops`.

use crate::runtime::bridge::Bridge;
use crate::runtime::explorer_ops::ExplorerEntry;
use opendrive_core::{classify_media_kind, MediaKind};
use opendrive_store::{Node, NodeState};
use opendrive_sync::{is_reparse_point, ExplorerSyncStatus, NodeId, SyncEngine};
use std::future::Future;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) fn run_outside_tokio<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|s| s.spawn(f).join().expect("worker thread"))
    } else {
        f()
    }
}

pub(super) fn block_on_rt<F: Future + Send>(rt: &tokio::runtime::Handle, fut: F) -> F::Output
where
    F::Output: Send,
{
    run_outside_tokio(|| rt.block_on(fut))
}

pub(super) fn node_to_entry(n: Node) -> ExplorerEntry {
    let name = n.effective_name().to_string();
    let is_folder = n.is_folder();
    let is_native_doc = n.is_native_doc();
    let media_kind = if is_folder {
        MediaKind::Folder
    } else if is_native_doc {
        MediaKind::NativeDoc
    } else {
        classify_media_kind(&name, n.mime.as_deref())
    };
    let sync_status = match n.state {
        NodeState::Placeholder => ExplorerSyncStatus::Cloud,
        NodeState::Cached if is_folder && !n.pinned => ExplorerSyncStatus::Indexed,
        NodeState::Cached => ExplorerSyncStatus::FullLocal,
        NodeState::Dirty => ExplorerSyncStatus::PendingUpload,
        NodeState::Conflict => ExplorerSyncStatus::Conflict,
        NodeState::Hydrating => ExplorerSyncStatus::Syncing,
    };
    ExplorerEntry {
        id: n.id as NodeId,
        name,
        media_kind,
        is_folder,
        is_native_doc: is_native_doc || media_kind == MediaKind::NativeDoc,
        mime: n.mime,
        size: n.size.max(0) as u64,
        modified_ms: n.modified_ms,
        state: n.state,
        sync_status,
        pinned: n.pinned,
        drive_id: n.drive_id,
    }
}

pub(super) fn node_to_entry_for(engine: &SyncEngine, n: Node) -> ExplorerEntry {
    let sync_status = engine.compute_sync_status(&n);
    let mut entry = node_to_entry(n);
    entry.sync_status = sync_status;
    entry
}

pub(super) async fn write_http_simple(
    w: &mut tokio::net::tcp::OwnedWriteHalf,
    status: &str,
    cr: Option<&str>,
) -> std::io::Result<()> {
    write_http_head(w, status, None, 0, cr).await
}

pub(super) async fn write_http_head(
    w: &mut tokio::net::tcp::OwnedWriteHalf,
    status: &str,
    ct: Option<&str>,
    len: u64,
    cr: Option<&str>,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let ct_hdr = ct.map_or_else(String::new, |c| format!("Content-Type: {c}\r\n"));
    let cr_hdr = cr.map_or_else(String::new, |c| format!("Content-Range: {c}\r\n"));
    let hdr = format!(
        "HTTP/1.1 {status}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\n{ct_hdr}Content-Length: {len}\r\n{cr_hdr}Connection: close\r\n\r\n"
    );
    w.write_all(hdr.as_bytes()).await
}

pub(super) fn is_regular_cached_file(path: &Path, expected_size: i64) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    meta.is_file() && !is_reparse_point(path) && (meta.len() > 0 || expected_size == 0)
}

impl Bridge {
    pub(super) fn get_store_node(&self, node_id: NodeId) -> Result<Node, String> {
        self.engine
            .get_node(node_id)
            .map_err(crate::runtime::bridge_types::str_err)?
            .ok_or_else(|| "node not found".to_string())
    }

    pub(super) fn cache_root_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .or_else(|| opendrive_platform::data_dir().ok())
            .unwrap_or_else(|| std::env::temp_dir().join("opendrive-cache"))
    }

    pub(super) fn local_node_path(
        &self,
        account_id: Uuid,
        root: NodeId,
        node: NodeId,
    ) -> Option<PathBuf> {
        let rel = self.engine.node_rel_path(root, node)?;
        if let Some(acct_root) = self.mounts.mounted_account_path(account_id) {
            return Some(acct_root.join(rel));
        }
        let folder = self
            .ctrl
            .lock()
            .expect("controller lock")
            .accounts
            .get_account(account_id)
            .ok()?
            .folder_name
            .clone();
        Some(self.mounts.mount_root().ok()?.join(folder).join(rel))
    }

    pub fn update_explorer_folder_cache(
        &self,
        enabled: bool,
    ) -> Result<crate::runtime::bridge::AppStateResponse, String> {
        self.modify_settings(|s| s.explorer_folder_cache = enabled, |_| Ok(()))
    }

    pub fn update_ui_font(
        &self,
        family: Option<String>,
    ) -> Result<crate::runtime::bridge::AppStateResponse, String> {
        let normalized = family
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self.modify_settings(|s| s.ui_font_family = normalized, |_| Ok(()))
    }

    pub fn explorer_local_entries(
        &self,
        folder_node: NodeId,
    ) -> Result<Vec<ExplorerEntry>, String> {
        let children = self
            .engine
            .list_cached_children(folder_node)
            .map_err(crate::runtime::bridge_types::str_err)?;
        Ok(children
            .into_iter()
            .map(|n| node_to_entry_for(&self.engine, n))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_helper_branches_work() {
        let tmp = std::env::temp_dir().join(format!("od-exp-help-{}", Uuid::new_v4()));
        let zero = tmp.join("sub/empty.bin");
        std::fs::create_dir_all(zero.parent().unwrap()).unwrap();
        std::fs::write(&zero, b"").unwrap();
        assert!(!is_regular_cached_file(&tmp.join("missing"), 0));
        assert!(!is_regular_cached_file(zero.parent().unwrap(), 0));
        assert!(is_regular_cached_file(&zero, 0));
        assert!(!is_regular_cached_file(&zero, 12));
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert_eq!(block_on_rt(rt.handle(), async { 7 }), 7);
        assert_eq!(
            rt.block_on(async { block_on_rt(rt.handle(), async { 9 }) }),
            9
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
