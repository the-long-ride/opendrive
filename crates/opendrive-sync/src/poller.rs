//! Remote change feed polling.

use crate::engine::SyncEngine;
use crate::{EngineEvent, Result, SyncError};
use opendrive_drive::DriveClient;
use opendrive_store::NodeState;
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

        let access = {
            let clients = self.clients.lock();
            clients
                .get(&account_id)
                .ok_or_else(|| SyncError::Other("no drive client".into()))?
                .access_token
                .clone()
        };
        let client = DriveClient::new(access);

        let page_token = match token {
            Some(t) => t,
            None => client
                .get_start_page_token()
                .await
                .map_err(|e| SyncError::Other(e.to_string()))?,
        };

        let changes = client
            .changes(&page_token)
            .await
            .map_err(|e| SyncError::Other(e.to_string()))?;

        let mut applied = 0;
        {
            let store = self.store.lock();
            for ch in &changes.changes {
                if ch.removed {
                    // mark matching drive_id trashed if present — full index later
                    debug!(file_id = %ch.file_id, "remote remove");
                    applied += 1;
                    continue;
                }
                if let Some(ref f) = ch.file {
                    let kind = if f.mime_type == "application/vnd.google-apps.folder" {
                        "folder"
                    } else if opendrive_drive::is_native_google_doc(&f.mime_type) {
                        "native_doc"
                    } else {
                        "file"
                    };
                    let id = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                        % i64::MAX as u128) as i64;
                    let n = opendrive_store::Node {
                        id,
                        account_id,
                        drive_id: f.id.clone(),
                        parent_id: None,
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

    pub fn spawn_change_poller(self: &Arc<Self>, account_id: Uuid, interval: Duration) {
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
        });
    }
}
