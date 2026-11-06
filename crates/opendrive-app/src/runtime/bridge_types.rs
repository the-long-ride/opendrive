//! Shared bridge response structs and synchronous helper utilities.

use opendrive_core::{Account, Settings};
use opendrive_sync::{EngineStats, SyncEngine, VfsEngine};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::task::{Context, Poll, Waker};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasEditDraft {
    pub account_id: Uuid,
    pub nickname: String,
    pub rename_folder: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VaultBackupDraft {
    pub password: String,
    pub confirm_password: String,
    pub show_password: bool,
    pub feedback: Option<String>,
    pub feedback_is_err: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub transfers: u64,
    pub queue_depth: usize,
    pub errors: u64,
    pub cache_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppStateResponse {
    pub accounts: Vec<Account>,
    pub settings: Settings,
    pub stats: StatsSnapshot,
    pub platform: String,
    pub mount_root: String,
}

pub(crate) fn str_err<E: ToString>(err: E) -> String {
    err.to_string()
}

pub(crate) fn engine_stats_sync(engine: &SyncEngine) -> EngineStats {
    let mut fut = std::pin::pin!(VfsEngine::stats(engine));
    let mut cx = Context::from_waker(Waker::noop());
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(stats) => stats,
        Poll::Pending => EngineStats {
            transfers: 0,
            queue_depth: 0,
            errors: 0,
        },
    }
}
