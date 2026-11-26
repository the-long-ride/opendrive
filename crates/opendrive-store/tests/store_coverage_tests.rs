//! Coverage tests for `opendrive-store` cache relocation, trashing, state updates, and queue queries.

use opendrive_store::{Node, NodeState, Store, StoreError};
use uuid::Uuid;

fn mk_node(id: i64, acct: Uuid, drive_id: &str, name: &str, parent_id: Option<i64>) -> Node {
    Node {
        id,
        account_id: acct,
        drive_id: drive_id.into(),
        parent_id,
        name: name.into(),
        display_name: name.into(),
        kind: "file".into(),
        mime: Some("text/plain".into()),
        size: 64,
        md5: Some("m".into()),
        modified_ms: 100,
        head_revision: Some("r1".into()),
        trashed: false,
        shared_drive_id: None,
        state: NodeState::Cached,
        pinned: true,
        last_access_ms: 50,
    }
}

#[test]
fn relocate_cache_prefix_trash_node_and_node_state_none_pin() {
    let store = Store::open_in_memory().unwrap();
    let acct = Uuid::new_v4();

    let n1 = mk_node(1, acct, "d1", "exact.txt", Some(0));
    let n2 = mk_node(2, acct, "d2", "nested.txt", Some(0));
    let n3 = mk_node(3, acct, "d3", "other.txt", Some(0));
    for n in [&n1, &n2, &n3] {
        store.upsert_node(n).unwrap();
    }

    store.set_cache_entry(1, "/mnt/OldDir", 10, true).unwrap();
    store
        .set_cache_entry(2, "/mnt/OldDir/sub/nested.txt", 20, false)
        .unwrap();
    store
        .set_cache_entry(3, "/mnt/Unrelated/other.txt", 30, false)
        .unwrap();

    let moved = store
        .relocate_cache_prefix("/mnt/OldDir", "/mnt/NewDir")
        .unwrap();
    assert_eq!(moved, 2);
    assert_eq!(
        store.get_cache_path(1).unwrap().as_deref(),
        Some("/mnt/NewDir")
    );
    let expected_nested = std::path::Path::new("/mnt/NewDir").join("sub/nested.txt");
    assert_eq!(
        store.get_cache_path(2).unwrap().as_deref(),
        Some(expected_nested.to_string_lossy().as_ref())
    );
    assert_eq!(
        store.get_cache_path(3).unwrap().as_deref(),
        Some("/mnt/Unrelated/other.txt")
    );

    // Update node state with pinned = None preserves pinned = true.
    assert!(store.update_node_state(1, NodeState::Dirty, None).unwrap());
    let updated = store.get_node(1).unwrap().unwrap();
    assert_eq!(updated.state, NodeState::Dirty);
    assert!(updated.pinned);

    // trash_node_and_clear clears cache entry, sets trashed=1 and pinned=0.
    store.trash_node_and_clear(1).unwrap();
    assert!(store.get_cache_path(1).unwrap().is_none());
    let trashed = store.get_node(1).unwrap().unwrap();
    assert!(trashed.trashed);
    assert!(!trashed.pinned);
}

#[test]
fn upload_queue_ordering_and_noop_mark_done() {
    let store = Store::open_in_memory().unwrap();
    let acct = Uuid::new_v4();
    let n1 = mk_node(1, acct, "d1", "a.txt", Some(0));
    let n2 = mk_node(2, acct, "d2", "b.txt", Some(0));
    store.upsert_node(&n1).unwrap();
    store.upsert_node(&n2).unwrap();

    let op1 = store.enqueue_upload(1, "create", None).unwrap();
    let op2 = store.enqueue_upload(2, "update", Some("rev-0")).unwrap();
    store.update_upload(op1, "pending", 1, 500, None).unwrap();
    store.update_upload(op2, "pending", 0, 100, None).unwrap();

    let first = store.next_upload_for_account(acct).unwrap().unwrap();
    assert_eq!(first.op_id, op2);
    assert_eq!(first.base_revision.as_deref(), Some("rev-0"));

    assert_eq!(store.mark_node_uploads_done(999).unwrap(), 0);
    assert_eq!(store.mark_node_uploads_done(2).unwrap(), 1);
    let second = store.next_upload_for_account(acct).unwrap().unwrap();
    assert_eq!(second.op_id, op1);

    let err = StoreError::from(rusqlite::Error::InvalidQuery);
    assert!(!err.to_string().is_empty());
}
