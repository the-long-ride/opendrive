//! Conflict policy: never silent LWW; upload as sibling copy.

use crate::queue::conflict_name;
use opendrive_store::{now_ms, Node, NodeState, Store};

/// If local base_revision != remote head, create a conflict sibling node locally.
pub fn materialize_conflict_sibling(
    store: &Store,
    local: &Node,
    host: &str,
) -> Result<Node, String> {
    let date = chrono_date();
    let display = conflict_name(&local.display_name, host, &date);
    let id = store.next_id().map_err(|e| e.to_string())?;
    let sibling = Node {
        id,
        account_id: local.account_id,
        drive_id: String::new(), // new upload
        parent_id: local.parent_id,
        name: display.clone(),
        display_name: display,
        kind: local.kind.clone(),
        mime: local.mime.clone(),
        size: local.size,
        md5: local.md5.clone(),
        modified_ms: now_ms(),
        head_revision: None,
        trashed: false,
        shared_drive_id: local.shared_drive_id.clone(),
        state: NodeState::Conflict,
        pinned: false,
        last_access_ms: now_ms(),
    };
    store.upsert_node(&sibling).map_err(|e| e.to_string())?;
    let _ = store.enqueue_upload(id, "create", None);
    Ok(sibling)
}

/// True when we must not overwrite remote (revision moved).
pub fn revision_conflict(local_base: Option<&str>, remote_head: Option<&str>) -> bool {
    match (local_base, remote_head) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

pub fn hostname_fallback() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "local".into())
}

fn chrono_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86400;
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn revision_mismatch() {
        assert!(revision_conflict(Some("a"), Some("b")));
        assert!(!revision_conflict(Some("a"), Some("a")));
        assert!(!revision_conflict(None, Some("a")));
        assert!(!revision_conflict(Some("a"), None));
    }

    #[test]
    fn chrono_date_is_iso_formatted() {
        let date = chrono_date();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn hostname_fallback_is_non_empty() {
        assert!(!hostname_fallback().is_empty());
        let (h, c) = (
            std::env::var_os("HOSTNAME"),
            std::env::var_os("COMPUTERNAME"),
        );
        std::env::remove_var("HOSTNAME");
        std::env::remove_var("COMPUTERNAME");
        assert_eq!(hostname_fallback(), "local");
        if let Some(v) = h {
            std::env::set_var("HOSTNAME", v);
        }
        if let Some(v) = c {
            std::env::set_var("COMPUTERNAME", v);
        }
    }

    #[test]
    fn materialize_conflict_sibling_enqueues_create() {
        let store = Store::open_in_memory().unwrap();
        let acct = Uuid::new_v4();
        let local = Node {
            id: 1,
            account_id: acct,
            drive_id: "d1".into(),
            parent_id: Some(0),
            name: "report.txt".into(),
            display_name: "report.txt".into(),
            kind: "file".into(),
            mime: Some("text/plain".into()),
            size: 42,
            md5: None,
            modified_ms: 10,
            head_revision: Some("rev-1".into()),
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Dirty,
            pinned: false,
            last_access_ms: 10,
        };
        store.upsert_node(&local).unwrap();
        let sibling = materialize_conflict_sibling(&store, &local, "workstation").unwrap();
        assert_eq!(sibling.id, 2);
        assert_eq!(sibling.state, NodeState::Conflict);
        assert!(sibling.drive_id.is_empty());
        assert!(sibling
            .display_name
            .starts_with("report.txt (conflict workstation "));
        assert_eq!(store.queue_depth().unwrap(), 1);
        let row = store.next_upload().unwrap().unwrap();
        assert_eq!(row.node_id, 2);
        assert_eq!(row.op, "create");
    }
}
