//! Folder hydration, real root ID resolution, and top-level item attachment.

use crate::engine::{sync_err, SyncEngine, SHARED_WITH_ME_DRIVE_ID};
use crate::path_time::parse_rfc3339_ms;
use crate::{NodeId, Result};
use opendrive_drive::{DriveClient, DriveFile};
use opendrive_store::{Node, NodeState, Store};
use std::collections::HashSet;
use uuid::Uuid;

impl SyncEngine {
    pub(crate) async fn ensure_real_root_id(
        &self,
        account_id: Uuid,
        client: &DriveClient,
    ) -> Option<String> {
        if let Some(id) = self.root_drive_ids.lock().get(&account_id).cloned() {
            return Some(id);
        }
        let id = client.root_folder_id().await.ok()?;
        self.root_drive_ids.lock().insert(account_id, id.clone());
        Some(id)
    }

    pub(crate) fn ensure_shared_with_me_node(
        &self,
        store: &Store,
        account_id: Uuid,
    ) -> Result<i64> {
        if let Some(mut swm) = store
            .find_by_drive_id(account_id, SHARED_WITH_ME_DRIVE_ID)
            .map_err(sync_err)?
        {
            if swm.parent_id != Some(0) {
                swm.parent_id = Some(0);
                store.upsert_node(&swm).map_err(sync_err)?;
            }
            return Ok(swm.id);
        }
        let swm_id = store.next_id().map_err(sync_err)?;
        store
            .upsert_node(&Node {
                id: swm_id,
                account_id,
                drive_id: SHARED_WITH_ME_DRIVE_ID.into(),
                parent_id: Some(0),
                name: "Shared with me".into(),
                display_name: "Shared with me".into(),
                kind: "folder".into(),
                mime: Some("application/vnd.google-apps.folder".into()),
                size: 0,
                md5: None,
                modified_ms: 0,
                head_revision: None,
                trashed: false,
                shared_drive_id: None,
                state: NodeState::Placeholder,
                pinned: false,
                last_access_ms: 0,
            })
            .map_err(sync_err)?;
        Ok(swm_id)
    }

    pub(crate) fn resolve_parent_node_id(
        &self,
        store: &Store,
        account_id: Uuid,
        parents: &[String],
        real_root_id: Option<&str>,
    ) -> Option<i64> {
        for pid in parents {
            let lookup_id = if pid == "root" || real_root_id == Some(pid.as_str()) {
                "root"
            } else {
                pid.as_str()
            };
            if let Some(p) = store.find_by_drive_id(account_id, lookup_id).ok().flatten() {
                return Some(p.id);
            }
        }
        None
    }

    pub async fn hydrate_folder(&self, account_id: Uuid, parent_node: NodeId) -> Result<()> {
        let drive_parent = {
            let store = self.store.lock();
            store
                .get_node(parent_node as i64)
                .map_err(sync_err)?
                .ok_or(crate::SyncError::NotFound)?
                .drive_id
                .clone()
        };
        let client = self.drive_client_for(account_id)?;
        let is_root = drive_parent == "root";
        let is_swm = drive_parent == SHARED_WITH_ME_DRIVE_ID;
        let mut all_files = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let list = if is_swm {
                client
                    .list_shared_with_me(page_token.as_deref())
                    .await
                    .map_err(sync_err)?
            } else {
                client
                    .list_files(&drive_parent, page_token.as_deref(), true)
                    .await
                    .map_err(sync_err)?
            };
            all_files.extend(list.files);
            match list.next_page_token {
                Some(tok) if !tok.is_empty() => page_token = Some(tok),
                _ => break,
            }
        }
        let (real_root_id, extra_top_ok, extra_top) = if is_root {
            let root_id = self.ensure_real_root_id(account_id, &client).await;
            match client.list_top_unparented_files().await {
                Ok(v) => (root_id, true, v),
                Err(_) => (root_id, false, Vec::new()),
            }
        } else {
            (None, true, Vec::new())
        };

        let store = self.store.lock();
        let swm_id = if is_root {
            Some(self.ensure_shared_with_me_node(&store, account_id)?)
        } else {
            None
        };
        let mut seen_ids = HashSet::new();
        for f in all_files {
            let target_parent_id = if is_swm {
                f.parents
                    .iter()
                    .find_map(|pid| store.find_by_drive_id(account_id, pid).ok().flatten())
                    .filter(|p| p.drive_id != "root")
                    .map_or(parent_node as i64, |p| p.id)
            } else {
                parent_node as i64
            };
            let eff_id = self.upsert_hydrated_entry(&store, account_id, target_parent_id, f)?;
            if target_parent_id == parent_node as i64 {
                seen_ids.insert(eff_id);
            }
        }
        for f in extra_top {
            if f.id == "root" || real_root_id.as_deref() == Some(f.id.as_str()) {
                continue;
            }
            let target_parent_id = self
                .resolve_parent_node_id(&store, account_id, &f.parents, real_root_id.as_deref())
                .unwrap_or_else(|| {
                    if f.shared_with_me == Some(true) {
                        swm_id.unwrap_or(parent_node as i64)
                    } else {
                        parent_node as i64
                    }
                });
            let eff_id = self.upsert_hydrated_entry(&store, account_id, target_parent_id, f)?;
            if target_parent_id == parent_node as i64 {
                seen_ids.insert(eff_id);
            }
        }
        if !is_root || extra_top_ok {
            for child in store.list_children(parent_node as i64).map_err(sync_err)? {
                if !child.drive_id.is_empty()
                    && !seen_ids.contains(&child.drive_id)
                    && !matches!(child.state, NodeState::Dirty | NodeState::Conflict)
                {
                    if let Ok(Some(cp)) = store.get_cache_path(child.id) {
                        crate::replica_clear::remove_regular_non_reparse(std::path::Path::new(&cp));
                    }
                    self.stream_cache.evict_node(child.id as NodeId);
                    let _ = store.trash_node_and_clear(child.id);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn reconcile_cached_state(
        &self,
        store: &Store,
        existing: Option<&Node>,
        md5: &Option<String>,
        rev: &Option<String>,
    ) -> (NodeState, bool, i64) {
        match existing {
            Some(cur) if cur.state == NodeState::Cached => {
                if cur.md5 == *md5 && cur.head_revision == *rev {
                    (NodeState::Cached, cur.pinned, cur.last_access_ms)
                } else {
                    if let Ok(Some(cp)) = store.get_cache_path(cur.id) {
                        crate::replica_clear::remove_regular_non_reparse(std::path::Path::new(&cp));
                    }
                    let _ = store.remove_cache_entry(cur.id);
                    self.stream_cache.evict_node(cur.id as NodeId);
                    (NodeState::Placeholder, false, 0)
                }
            }
            _ => (NodeState::Placeholder, false, 0),
        }
    }

    fn upsert_hydrated_entry(
        &self,
        store: &Store,
        account_id: Uuid,
        target_parent_id: i64,
        f: DriveFile,
    ) -> Result<String> {
        let (effective_id, effective_mime, kind) = resolve_drive_target(&f);
        let existing = store
            .find_by_drive_id(account_id, &effective_id)
            .ok()
            .flatten();
        if existing
            .as_ref()
            .is_some_and(|ex| ex.state == NodeState::Dirty)
        {
            return Ok(effective_id);
        }
        let id = match existing.as_ref() {
            Some(ex) => ex.id,
            None => store.next_id().map_err(sync_err)?,
        };
        let (state, pinned, last_access_ms) =
            self.reconcile_cached_state(store, existing.as_ref(), &f.md5, &f.head_revision_id);
        let modified_ms = f
            .modified_time
            .as_deref()
            .map(parse_rfc3339_ms)
            .unwrap_or(0);
        store
            .upsert_node(&Node {
                id,
                account_id,
                drive_id: effective_id.clone(),
                parent_id: Some(target_parent_id),
                name: f.name.clone(),
                display_name: f.name,
                kind: kind.into(),
                mime: Some(effective_mime),
                size: f.size.unwrap_or(0) as i64,
                md5: f.md5,
                modified_ms,
                head_revision: f.head_revision_id,
                trashed: f.trashed,
                shared_drive_id: None,
                state,
                pinned,
                last_access_ms,
            })
            .map_err(sync_err)?;
        Ok(effective_id)
    }
}

pub(crate) fn resolve_drive_target(f: &DriveFile) -> (String, String, &'static str) {
    let (effective_id, effective_mime) = if f.mime_type == "application/vnd.google-apps.shortcut" {
        match f.shortcut_target_id.as_deref().filter(|s| !s.is_empty()) {
            Some(tid) => (
                tid.to_string(),
                f.shortcut_target_mime
                    .clone()
                    .unwrap_or_else(|| f.mime_type.clone()),
            ),
            None => (f.id.clone(), f.mime_type.clone()),
        }
    } else {
        (f.id.clone(), f.mime_type.clone())
    };
    let kind = match effective_mime.as_str() {
        "application/vnd.google-apps.folder" => "folder",
        "application/vnd.google-apps.shortcut" => "shortcut",
        m if opendrive_drive::is_native_google_doc(m) => "native_doc",
        _ => "file",
    };
    (effective_id, effective_mime, kind)
}
