//! Unit and invariant tests for `opendrive-store`.

use opendrive_store::{Node, NodeState, Store};
use uuid::Uuid;

fn sample_node(id: i64, acct: Uuid, drive_id: &str, name: &str, parent_id: Option<i64>) -> Node {
    Node {
        id,
        account_id: acct,
        drive_id: drive_id.into(),
        parent_id,
        name: name.into(),
        display_name: name.into(),
        kind: "file".into(),
        mime: Some("text/plain".into()),
        size: 128,
        md5: Some("abc".into()),
        modified_ms: 100,
        head_revision: Some("rev-1".into()),
        trashed: false,
        shared_drive_id: None,
        state: NodeState::Cached,
        pinned: false,
        last_access_ms: 100,
    }
}

#[test]
fn node_crud_listing_trash_and_tokens() {
    let dir = std::env::temp_dir().join(format!("opendrive-store-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("meta.db");
    let store = Store::open(db_path.to_str().unwrap()).unwrap();
    let acct = Uuid::new_v4();

    assert_eq!(store.next_id().unwrap(), 1);
    let root = sample_node(1, acct, "root", "My Drive", Some(0));
    store.upsert_node(&root).unwrap();
    assert_eq!(store.next_id().unwrap(), 2);

    let mut b = sample_node(2, acct, "d-b", "beta.txt", Some(1));
    b.state = NodeState::Placeholder;
    let a = sample_node(3, acct, "d-a", "alpha.txt", Some(1));
    store.upsert_node(&b).unwrap();
    store.upsert_node(&a).unwrap();

    let found = store.find_by_drive_id(acct, "d-a").unwrap().unwrap();
    assert_eq!(found.id, 3);
    assert_eq!(store.get_node(3).unwrap().unwrap().name, "alpha.txt");
    assert!(store.get_node(999).unwrap().is_none());

    let kids = store.list_children(1).unwrap();
    let names: Vec<_> = kids.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["alpha.txt", "beta.txt"]);

    assert!(store.mark_drive_id_trashed(acct, "d-b").unwrap());
    assert!(!store.mark_drive_id_trashed(acct, "missing").unwrap());
    let kids_after = store.list_children(1).unwrap();
    assert_eq!(kids_after.len(), 1);

    assert!(store.get_change_token(acct).unwrap().is_none());
    store.set_change_token(acct, "tok-42").unwrap();
    assert_eq!(
        store.get_change_token(acct).unwrap().as_deref(),
        Some("tok-42")
    );
    store.set_change_token(acct, "tok-43").unwrap();
    assert_eq!(
        store.get_change_token(acct).unwrap().as_deref(),
        Some("tok-43")
    );

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upload_queue_lifecycle() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.queue_depth().unwrap(), 0);
    assert!(store.next_upload().unwrap().is_none());

    let op1 = store.enqueue_upload(10, "create", None).unwrap();
    let op2 = store.enqueue_upload(11, "update", Some("rev-1")).unwrap();
    assert_eq!(store.queue_depth().unwrap(), 2);

    let first = store.next_upload().unwrap().unwrap();
    assert_eq!(first.op_id, op1);
    assert_eq!(first.op, "create");

    store
        .update_upload(op1, "in_progress", 1, 0, Some("http://upload/1"))
        .unwrap();
    let resumed = store.next_upload().unwrap().unwrap();
    assert_eq!(resumed.resumable_uri.as_deref(), Some("http://upload/1"));

    store.update_upload(op1, "done", 1, 0, None).unwrap();
    assert_eq!(store.queue_depth().unwrap(), 1);
    let second = store.next_upload().unwrap().unwrap();
    assert_eq!(second.op_id, op2);
    assert_eq!(second.base_revision.as_deref(), Some("rev-1"));

    // Future next_try_ms defers next_upload
    store
        .update_upload(op2, "pending", 1, i64::MAX / 2, None)
        .unwrap();
    assert!(store.next_upload().unwrap().is_none());

    // Opening an existing directory as SQLite db fails cleanly
    let dir = std::env::temp_dir();
    assert!(Store::open(dir.to_str().unwrap()).is_err());
}

#[test]
fn cache_eviction_respects_safety_invariant_3() {
    let store = Store::open_in_memory().unwrap();
    let acct = Uuid::new_v4();

    let mut clean_old = sample_node(1, acct, "d1", "old.bin", Some(0));
    clean_old.last_access_ms = 10;

    let mut clean_new = sample_node(2, acct, "d2", "new.bin", Some(0));
    clean_new.last_access_ms = 200;

    let mut dirty = sample_node(3, acct, "d3", "dirty.bin", Some(0));
    dirty.state = NodeState::Dirty;
    dirty.last_access_ms = 1;

    let mut pinned_node = sample_node(4, acct, "d4", "pinned_node.bin", Some(0));
    pinned_node.pinned = true;
    pinned_node.last_access_ms = 2;

    let mut pinned_cache = sample_node(5, acct, "d5", "pinned_cache.bin", Some(0));
    pinned_cache.last_access_ms = 3;

    for n in [&clean_old, &clean_new, &dirty, &pinned_node, &pinned_cache] {
        store.upsert_node(n).unwrap();
    }

    store.set_cache_entry(1, "/cache/1", 100, false).unwrap();
    store.set_cache_entry(2, "/cache/2", 200, false).unwrap();
    store.set_cache_entry(3, "/cache/3", 300, false).unwrap();
    store.set_cache_entry(4, "/cache/4", 400, false).unwrap();
    store.set_cache_entry(5, "/cache/5", 500, true).unwrap();

    assert_eq!(store.cache_bytes_total().unwrap(), 1500);
    assert_eq!(
        store.get_cache_path(1).unwrap().as_deref(),
        Some("/cache/1")
    );

    let cands = store.eviction_candidates(10).unwrap();
    assert_eq!(cands, vec![1, 2]);

    store.touch_access(1).unwrap();
    let cands_after_touch = store.eviction_candidates(10).unwrap();
    assert_eq!(cands_after_touch, vec![2, 1]);

    store.remove_cache_entry(2).unwrap();
    assert!(store.get_cache_path(2).unwrap().is_none());
    assert_eq!(store.cache_bytes_total().unwrap(), 1300);
}

#[test]
fn sorted_children_account_cache_bytes_and_account_upload_queue() {
    let store = Store::open_in_memory().unwrap();
    let acct1 = Uuid::new_v4();
    let acct2 = Uuid::new_v4();

    let mut folder = sample_node(1, acct1, "f1", "zeta", Some(0));
    folder.kind = "folder".into();
    let file_b = sample_node(2, acct1, "f2", "Beta.txt", Some(0));
    let file_a = sample_node(3, acct1, "f3", "alpha.txt", Some(0));
    let other_acct = sample_node(4, acct2, "f4", "other.bin", Some(0));
    for n in [&folder, &file_b, &file_a, &other_acct] {
        store.upsert_node(n).unwrap();
    }

    let sorted = store.list_children_sorted(0).unwrap();
    let names: Vec<_> = sorted.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["zeta", "alpha.txt", "Beta.txt", "other.bin"]);

    store.set_cache_entry(2, "/c/2", 250, false).unwrap();
    store.set_cache_entry(4, "/c/4", 900, false).unwrap();
    assert_eq!(store.cache_bytes_total().unwrap(), 1150);

    assert!(store
        .update_node_state(2, NodeState::Cached, Some(true))
        .unwrap());
    assert!(!store
        .update_node_state(999, NodeState::Cached, None)
        .unwrap());
    let n2 = store.get_node(2).unwrap().unwrap();
    assert_eq!(n2.state, NodeState::Cached);
    assert!(n2.pinned);

    store.clear_node_cache_state(2).unwrap();
    assert!(store.get_cache_path(2).unwrap().is_none());
    let n2_cleared = store.get_node(2).unwrap().unwrap();
    assert_eq!(n2_cleared.state, NodeState::Placeholder);
    assert!(!n2_cleared.pinned);

    store.enqueue_upload(4, "create", None).unwrap();
    store.enqueue_upload(2, "update", None).unwrap();
    let u1 = store.next_upload_for_account(acct1).unwrap().unwrap();
    assert_eq!(u1.node_id, 2);
    assert_eq!(store.mark_node_uploads_done(2).unwrap(), 1);
    assert!(store.next_upload_for_account(acct1).unwrap().is_none());
    assert!(store.next_upload_for_account(acct2).unwrap().is_some());
}
