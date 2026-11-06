//! Upload queue types (persisted in store.upload_queue).

use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum UploadOp {
    Create,
    Update,
    Rename,
    Trash,
}

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub op_id: i64,
    pub node_id: i64,
    pub account_id: Uuid,
    pub op: UploadOp,
    pub attempts: u32,
    pub next_try_ms: i64,
    pub resumable_uri: Option<String>,
    pub base_revision: Option<String>,
}

/// On remote head change since base_revision: upload as sibling
/// `name (conflict <host> <date>)` and notify — never silent LWW.
pub fn conflict_name(original: &str, host: &str, date: &str) -> String {
    format!("{original} (conflict {host} {date})")
}
