//! Remote change feed polling.

use crate::conflict::{hostname_fallback, materialize_conflict_sibling, revision_conflict};
use crate::engine::SyncEngine;
use crate::hydrate::resolve_drive_target;
use crate::path_time::parse_rfc3339_ms;
use crate::{EngineEvent, Result, SyncError};
use opendrive_store::NodeState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};
use uuid::Uuid;

impl SyncEngine {
    /// Poll changes for one account; apply to store; emit events.
    pub async fn poll_changes(&self, account_id: Uuid) -> Result<usize> {
        let token = self
            .store
            .lock()
            .get_change_token(account_id)
            .map_err(|e| SyncError::Other(e.to_string()))?;
        let client = self.drive_client_for(account_id)?;
        let page_token = match token {
            Some(t) => t,
            None => client.get_start_page_token().await.map_err(|e| {
                self.errors.fetch_add(1, Ordering::Relaxed);
                SyncError::Other(e.to_string())
            })?,
        };
        let changes = client.changes(&page_token).await.map_err(|e| {
            self.errors.fetch_add(1, Ordering::Relaxed);
            SyncError::Other(e.to_string())
        })?;

        let real_root_id = self.ensure_real_root_id(account_id, &client).await;
        let mut applied = 0;
        {
            let store = self.store.lock();
            for ch in &changes.changes {
                if ch.removed {
                    debug!(file_id = %ch.file_id, "remote remove");
                    let _ = store.mark_drive_id_trashed(account_id, &ch.file_id);
                    applied += 1;
                    continue;
                }
                if let Some(ref f) = ch.file {
                    let (effective_id, effective_mime, kind) = resolve_drive_target(f);
                    let existing = store
                        .find_by_drive_id(account_id, &effective_id)
                        .ok()
                        .flatten();
                    if let Some(ref cur) = existing {
                        if cur.state == NodeState::Dirty {
                            if !revision_conflict(
                                cur.head_revision.as_deref(),
                                f.head_revision_id.as_deref(),
                            ) {
                                continue;
                            }
                            if let Ok(sib) =
                                materialize_conflict_sibling(&store, cur, &hostname_fallback())
                            {
                                if let Some(ref st) = *self.staging.lock() {
                                    if let Ok(bytes) = st.read_all(cur.id) {
                                        if !bytes.is_empty() {
                                            let _ = st.write_at(sib.id, 0, &bytes);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let (state, pinned, last_access_ms) = self.reconcile_cached_state(
                        &store,
                        existing.as_ref(),
                        &f.md5,
                        &f.head_revision_id,
                    );
                    let id = match existing.as_ref() {
                        Some(cur) => cur.id,
                        None => match store.next_id() {
                            Ok(n) => n,
                            Err(_) => continue,
                        },
                    };
                    let resolved_parent = self.resolve_parent_node_id(
                        &store,
                        account_id,
                        &f.parents,
                        real_root_id.as_deref(),
                    );
                    let parent_id = resolved_parent
                        .or_else(|| existing.as_ref().and_then(|c| c.parent_id))
                        .or_else(|| {
                            if !f.parents.is_empty() {
                                return None;
                            }
                            if f.shared_with_me == Some(true) {
                                self.ensure_shared_with_me_node(&store, account_id).ok()
                            } else {
                                let root_n =
                                    store.find_by_drive_id(account_id, "root").ok().flatten()?;
                                Some(root_n.id)
                            }
                        });
                    let modified_ms = f
                        .modified_time
                        .as_deref()
                        .map(parse_rfc3339_ms)
                        .or_else(|| existing.as_ref().map(|c| c.modified_ms))
                        .unwrap_or(0);
                    let n = opendrive_store::Node {
                        id,
                        account_id,
                        drive_id: effective_id,
                        parent_id,
                        name: f.name.clone(),
                        display_name: f.name.clone(),
                        kind: kind.into(),
                        mime: Some(effective_mime),
                        size: f.size.unwrap_or(0) as i64,
                        md5: f.md5.clone(),
                        modified_ms,
                        head_revision: f.head_revision_id.clone(),
                        trashed: f.trashed,
                        shared_drive_id: None,
                        state,
                        pinned,
                        last_access_ms,
                    };
                    let _ = store.upsert_node(&n);
                    applied += 1;
                }
            }
            if let Some(ref new_tok) = changes.new_start_token {
                let _ = store.set_change_token(account_id, new_tok);
            }
        }

        if applied > 0 {
            if let Some(tx) = self.events.lock().as_ref() {
                let _ = tx.try_send(EngineEvent::NodeChanged(0));
            }
        }
        Ok(applied)
    }

    pub fn spawn_change_poller(
        self: &Arc<Self>,
        account_id: Uuid,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let eng = Arc::clone(self);
        tokio::spawn(async move {
            let mut consecutive_failures = 0u32;
            loop {
                let sleep_for = match eng.poll_changes(account_id).await {
                    Ok(n) => {
                        consecutive_failures = 0;
                        if n > 0 {
                            debug!(count = n, "applied remote changes");
                        }
                        interval
                    }
                    Err(e) => {
                        consecutive_failures = (consecutive_failures + 1).min(4);
                        if e.to_string().contains("auth") {
                            warn!(
                                code = "OD-AUTH-002",
                                error = %e,
                                "change poll failed — sign in again if token was revoked"
                            );
                        } else {
                            warn!(error = %e, "change poll failed");
                        }
                        interval
                            .saturating_mul(1u32 << consecutive_failures.min(4))
                            .min(Duration::from_secs(300))
                    }
                };
                tokio::time::sleep(sleep_for).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_drive_target;
    use opendrive_drive::{DriveFile, FileJson};

    #[test]
    fn shortcut_without_target_mime_falls_back_cleanly() {
        let fj: FileJson = serde_json::from_str(
            r#"{"id":"s1","name":"k","mimeType":"application/vnd.google-apps.shortcut","shortcutDetails":{"targetId":"t1"}}"#,
        )
        .unwrap();
        let f: DriveFile = fj.into();
        assert_eq!(resolve_drive_target(&f).0, "t1");
    }

    #[tokio::test]
    async fn poll_changes_handles_shared_with_me_conflict_remove_and_errors() {
        use crate::engine::SyncEngine;
        use crate::SyncError;
        use opendrive_drive::DriveClient;
        use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
        use opendrive_store::{NodeState, Store};
        use std::sync::Arc;
        use std::time::Duration;
        use uuid::Uuid;

        let drive = Arc::new(FakeDrive::new());
        drive.add_file(FakeFile {
            id: "d1".into(),
            name: "dirty.txt".into(),
            mime_type: "text/plain".into(),
            size: 5,
            parents: vec!["root".into()],
            trashed: false,
            content: b"cloud".to_vec(),
            head_revision_id: Some("rev-1".into()),
        });
        let (addr, h) = FakeDriveServer::start(Arc::clone(&drive)).await;
        let tmp = std::env::temp_dir().join(format!("od-poll-cov-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = Store::open_in_memory().unwrap();
        let eng = Arc::new(
            SyncEngine::new(store)
                .with_staging_dir(tmp.join("stage"))
                .unwrap(),
        );
        let aid = Uuid::new_v4();
        let dc = DriveClient::with_base_urls(
            "t".into(),
            format!("http://{addr}/drive/v3"),
            format!("http://{addr}/upload/drive/v3"),
        );
        eng.set_drive_client(aid, dc);
        let root_id = eng.seed_root_account(aid, "Work").unwrap();
        eng.list_explorer_nodes(aid, root_id, true).await.unwrap();
        assert_eq!(eng.poll_changes(aid).await.unwrap(), 0);

        {
            let store = eng.store().lock();
            let mut dirty = store.find_by_drive_id(aid, "d1").unwrap().unwrap();
            dirty.state = NodeState::Dirty;
            store.upsert_node(&dirty).unwrap();
            eng.staging
                .lock()
                .as_ref()
                .unwrap()
                .write_at(dirty.id, 0, b"local-edit")
                .unwrap();
        }
        drive.update_metadata("d1", None, None, None);
        drive.add_file(FakeFile {
            id: "swm-1".into(),
            name: "orphan-shared.txt".into(),
            mime_type: "text/plain".into(),
            size: 2,
            parents: vec![],
            trashed: false,
            content: b"ok".to_vec(),
            head_revision_id: Some("rev-1".into()),
        });
        drive.mark_shared_with_me("swm-1");
        drive.trash("d1");
        assert!(eng.poll_changes(aid).await.unwrap() >= 3);

        let bad_aid = Uuid::new_v4();
        drive.push_error(500);
        eng.set_drive_client(
            bad_aid,
            DriveClient::with_base_urls(
                "t".into(),
                format!("http://{addr}/drive/v3"),
                format!("http://{addr}/upload/drive/v3"),
            ),
        );
        assert!(matches!(
            eng.poll_changes(bad_aid).await,
            Err(SyncError::Other(_))
        ));

        let poller = eng.spawn_change_poller(aid, Duration::from_millis(5));
        tokio::time::sleep(Duration::from_millis(12)).await;
        poller.abort();

        h.abort();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
