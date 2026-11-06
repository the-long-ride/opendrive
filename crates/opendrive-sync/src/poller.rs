//! Remote change feed polling.

use crate::conflict::{hostname_fallback, materialize_conflict_sibling, revision_conflict};
use crate::engine::SyncEngine;
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
        let token = {
            let store = self.store.lock();
            store
                .get_change_token(account_id)
                .map_err(|e| SyncError::Other(e.to_string()))?
        };

        let client = self.drive_client_for(account_id)?;
        let page_token = match token {
            Some(t) => t,
            None => match client.get_start_page_token().await {
                Ok(t) => t,
                Err(e) => {
                    self.errors.fetch_add(1, Ordering::Relaxed);
                    return Err(SyncError::Other(e.to_string()));
                }
            },
        };

        let changes = match client.changes(&page_token).await {
            Ok(c) => c,
            Err(e) => {
                self.errors.fetch_add(1, Ordering::Relaxed);
                return Err(SyncError::Other(e.to_string()));
            }
        };

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
                    let existing = store.find_by_drive_id(account_id, &f.id).ok().flatten();
                    if let Some(ref cur) = existing {
                        if cur.state == NodeState::Dirty
                            && revision_conflict(
                                cur.head_revision.as_deref(),
                                f.head_revision_id.as_deref(),
                            )
                        {
                            if let Ok(sibling) =
                                materialize_conflict_sibling(&store, cur, &hostname_fallback())
                            {
                                if let Some(ref staging) = *self.staging.lock() {
                                    if let Ok(bytes) = staging.read_all(cur.id) {
                                        if !bytes.is_empty() {
                                            let _ = staging.write_at(sibling.id, 0, &bytes);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let id = match existing {
                        Some(cur) => cur.id,
                        None => match store.next_id() {
                            Ok(n) => n,
                            Err(_) => continue,
                        },
                    };
                    let parent_id = f
                        .parents
                        .first()
                        .and_then(|pid| store.find_by_drive_id(account_id, pid).ok().flatten())
                        .map(|p| p.id);
                    let kind = if f.mime_type == "application/vnd.google-apps.folder" {
                        "folder"
                    } else if opendrive_drive::is_native_google_doc(&f.mime_type) {
                        "native_doc"
                    } else {
                        "file"
                    };
                    let n = opendrive_store::Node {
                        id,
                        account_id,
                        drive_id: f.id.clone(),
                        parent_id,
                        name: f.name.clone(),
                        display_name: f.name.clone(),
                        kind: kind.into(),
                        mime: Some(f.mime_type.clone()),
                        size: f.size.unwrap_or(0) as i64,
                        md5: f.md5.clone(),
                        modified_ms: 0,
                        head_revision: f.head_revision_id.clone(),
                        trashed: f.trashed,
                        shared_drive_id: None,
                        state: NodeState::Placeholder,
                        pinned: false,
                        last_access_ms: 0,
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
            loop {
                match eng.poll_changes(account_id).await {
                    Ok(n) if n > 0 => debug!(count = n, "applied remote changes"),
                    Ok(_) => {}
                    Err(e) => warn!(error = %e, "change poll failed"),
                }
                tokio::time::sleep(interval).await;
            }
        })
    }
}
