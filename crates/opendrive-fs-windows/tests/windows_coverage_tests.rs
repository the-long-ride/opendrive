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
    #[cfg(not(windows))]
    opendrive_fs_windows::unregister_all_opendrive_roots();

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
        .connect_live(blocked_root.to_str().unwrap(), "alice", handle.clone())
        .is_err());

    let rec_dir = base.join("reconcile_root");
    let sub = rec_dir.join("ExistingFolder");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("note.txt"), b"ok").unwrap();
    std::fs::write(rec_dir.join("a_b.txt"), b"sanitized").unwrap();
    CloudFilesFrontend::reconcile_placeholders(
        &rec_dir,
        &[std::path::PathBuf::from("ExistingFolder")],
    );
    CloudFilesFrontend::reconcile_placeholder_entries(
        &rec_dir,
        &[
            (std::path::PathBuf::from("ExistingFolder"), 10, true),
            (
                std::path::PathBuf::from("ExistingFolder/note.txt"),
                11,
                false,
            ),
            (std::path::PathBuf::from("a:b.txt"), 12, false),
        ],
    );
    // Unregistered directory hits convert error-log branch safely; live connect on temp sync root converts plain dirs/files!
    let live_root = base.join("live_reconcile_root");
    let nested = live_root.join("04-Audio Common").join("00-Music");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("song.mp3"), b"mp3").unwrap();
    if let Ok(_conn) = frontend.connect_live(
        live_root.to_str().unwrap(),
        &format!("cov-{}", Uuid::new_v4()),
        handle,
    ) {
        let entries = vec![
            (std::path::PathBuf::from("04-Audio Common"), 10, true),
            (
                std::path::PathBuf::from("04-Audio Common/00-Music"),
                11,
                true,
            ),
            (
                std::path::PathBuf::from("04-Audio Common/00-Music/song.mp3"),
                12,
                false,
            ),
        ];
        // First pass converts plain folders/files to CFAPI placeholders; second pass updates existing placeholders.
        CloudFilesFrontend::reconcile_placeholder_entries(&live_root, &entries);
        CloudFilesFrontend::reconcile_placeholder_entries(&live_root, &entries);
        CloudFilesFrontend::reconcile_placeholders(
            &live_root,
            &[std::path::PathBuf::from("04-Audio Common")],
        );
    }
    unregister_os_sync_root(&live_root);

    let _ = std::fs::remove_dir_all(&base);
}
