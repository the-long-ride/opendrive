//! Coverage tests for `opendrive-fs-windows` error paths and root management.

use opendrive_fs_windows::{unregister_os_sync_root, CloudFilesFrontend};
use opendrive_sync::PlaceholderEngine;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn register_sync_root_fails_when_parent_is_a_file_and_unregister_missing_os_root_is_safe() {
    let base = std::env::temp_dir().join(format!("od-win-cov-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let blocker = base.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();

    let mut frontend = CloudFilesFrontend::new(Arc::new(PlaceholderEngine));
    let blocked_root = blocker.join("child");
    assert!(frontend
        .register_sync_root(blocked_root.to_str().unwrap(), "alice")
        .is_err());

    // Safe on nonexistent path: SyncRootId::from_path returns Err without mutating any OS root.
    unregister_os_sync_root(&base.join("nonexistent-sync-root"));

    let _ = std::fs::remove_dir_all(&base);
}

#[cfg(windows)]
#[tokio::test]
async fn connect_live_returns_error_early_when_root_path_is_blocked() {
    let base = std::env::temp_dir().join(format!("od-win-live-err-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let blocker = base.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();

    let mut frontend = CloudFilesFrontend::new(Arc::new(PlaceholderEngine));
    let blocked_root = blocker.join("child");
    let handle = tokio::runtime::Handle::current();
    assert!(frontend
        .connect_live(blocked_root.to_str().unwrap(), "alice", handle)
        .is_err());

    let _ = std::fs::remove_dir_all(&base);
}
