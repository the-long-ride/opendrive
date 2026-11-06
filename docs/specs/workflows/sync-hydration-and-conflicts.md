---
schema_version: 1
id: "workflow.sync-hydration-conflicts"
title: "Sync engine, folder hydration, upload queue worker, eviction, and conflict handling"
kind: "workflow"
status: "verified"
summary: "Store-backed VfsEngine implementation orchestrating folder hydration, staged writes, durable resumable uploads, remote change polling, conflict-copy materialization, bandwidth limiting, cache eviction, and shutdown draining."
tags: ["workflow", "sync", "vfs", "hydration", "upload-worker", "poller", "conflict", "eviction", "staging"]
source_paths: ["crates/opendrive-sync/src/conflict.rs", "crates/opendrive-sync/src/engine.rs", "crates/opendrive-sync/src/evict.rs", "crates/opendrive-sync/src/lib.rs", "crates/opendrive-sync/src/limiter.rs", "crates/opendrive-sync/src/poller.rs", "crates/opendrive-sync/src/queue.rs", "crates/opendrive-sync/src/shutdown.rs", "crates/opendrive-sync/src/staging.rs", "crates/opendrive-sync/src/vfs_impl.rs", "crates/opendrive-sync/src/worker.rs", "crates/opendrive-sync/tests/lazy_hydration_tests.rs", "crates/opendrive-sync/tests/sync_tests.rs", "crates/opendrive-sync/tests/sync_worker_poller_tests.rs"]
related_specs: ["architecture.workspace-overview", "feature.drive-v3-client", "architecture.sqlite-store", "architecture.os-fs-adapters", "interface.cli-http-bridge"]
last_doc_update_at: "2026-11-18T10:43:00Z"
last_verified_at: "2026-11-18T10:43:00Z"
last_verified_code_commit: "018d1c9aa32bb78dfe351da53e12305b69e679c8"
last_source_change_commit: "018d1c9aa32bb78dfe351da53e12305b69e679c8"
last_source_change_at: "2026-11-18T10:43:00Z"
source_fingerprint: "sha256:badf61f737dfaa0b470cbf1a539d2e7be769ed66336330171640aecebe201340"
---

# Sync engine, folder hydration, upload queue worker, eviction, and conflict handling

## Purpose and boundaries
`opendrive-sync` implements the `VfsEngine` contract that bridges platform virtual-filesystem adapters with local SQLite state (`opendrive-store`), local staging files, and Google Drive (`opendrive-drive`).

Five safety invariants govern this crate (`crates/opendrive-sync/src/lib.rs`):
1. Never delete or overwrite remotely unless user action caused it.
2. Queued uploads survive crash and resume without loss or duplication.
3. Eviction never drops dirty data.
4. Remote changes never clobber unsynced local edits (conflict copy is created instead).
5. Quit waits for or reports pending uploads.

Out of scope: OS-specific VFS kernel/provider bindings (`opendrive-fs-windows`, `opendrive-fs-linux`, `opendrive-fs-macos`), OAuth flows (`opendrive-auth`), and UI/HTTP presentation (`opendrive-app`).

## Observable behavior

### `VfsEngine` contract and `SyncEngine` (`lib.rs`, `engine.rs`, `vfs_impl.rs`)
- `VfsEngine` defines async operations `list`, `lookup`, `stat`, `read`, `open_write`, `write`, `create`, `rename`, `remove`, `set_pinned`, `subscribe`, `add_account`, `remove_account`, and `stats`. `PlaceholderEngine` provides a no-op/empty stub implementation.
- `SyncEngine::new(store)` holds `Arc<Mutex<Store>>`, a per-account `HashMap<Uuid, DriveClient>`, an optional `mpsc::Sender<EngineEvent>` (`subscribe` creates a bounded channel of capacity 64), an optional `StagingDir`, a `BandwidthLimiter`, and atomic `transfers` / `errors` counters.
- `seed_root_account(account_id, folder_name)` idempotently creates the root folder node (`drive_id: "root"`, `parent_id: Some(0)`, `state: NodeState::Cached`) and a synthetic child folder `"Shared with me"` (`SHARED_WITH_ME_DRIVE_ID = "sharedWithMe"`, `state: NodeState::Placeholder`).
- `hydrate_folder(account_id, parent_node)` dispatches `SHARED_WITH_ME_DRIVE_ID` to `DriveClient::list_shared_with_me` and normal folders to `DriveClient::list_files(&drive_parent, page_token, true)`, paginating until `next_page_token` is exhausted, resolving Google Drive shortcuts (`shortcutDetails` -> effective target `drive_id` and `mime_type`), and upserting each entry as `NodeState::Placeholder` with kind `"folder"`, `"native_doc"`, or `"file"`.
- `placeholder_folder_rel_paths(account_id)` walks stored `NodeState::Placeholder` folders under an account root to return relative filesystem paths for Windows CFAPI on-demand reconciliation.
- `list` queries `Store::list_children` (non-trashed); if `children.is_empty()` and `node > 0`, and the target node is a `"folder"` with non-empty `drive_id` and `state == NodeState::Placeholder`, `list` requires a registered `DriveClient` (returning `Err` rather than an empty `DirPage` if offline/missing), calls `self.hydrate_folder(folder.account_id, node)?`, updates `folder.state = NodeState::Cached` only after hydration succeeds (reverting to/preserving `Placeholder` on error so folders are never sealed empty), and re-queries `Store::list_children`. `lookup` queries `Store::list_children`; `stat` queries `Store::get_node`.
- `read(node, offset, len)` checks `StagingDir::read_all(node)` first and returns the staged slice (throttled by `BandwidthLimiter`) if non-empty and in bounds; otherwise touches `last_access_ms`, downloads the byte range via `DriveClient::download_range`, throttles via `BandwidthLimiter::take`, and increments `transfers` (or `errors` on failure).
- `open_write` / `write` persist bytes to `<staging_root>/<node_id>.part` (never partial remote writes), grow `node.size`, transition `node.state = NodeState::Dirty`, and enqueue an `"update"` upload with `node.head_revision` as `base_revision`.
- `create` inserts a `Dirty` node with empty `drive_id` and enqueues `"create"`.
- `rename` updates `parent_id`, `name`, `display_name`, sets `Dirty`, and enqueues `"rename"`.
- `remove` marks `trashed = true`, enqueues `"trash"`, and removes any staging file for `node_id`.
- `set_pinned` persists `node.pinned`. `add_account` seeds the root node and emits `EngineEvent::AccountAdded`; `remove_account` drops the account's `DriveClient` and emits `EngineEvent::AccountRemoved`.

### Bandwidth limiter and staging (`limiter.rs`, `staging.rs`)
- `BandwidthLimiter` implements a mutex-protected token bucket (`max_burst = rate * 2.0` bytes); `None` or `Some(0)` disables throttling. `take(n)` sleeps asynchronously until `n` tokens accrue.
- `StagingDir` creates `root` and stores `<node_id>.part` files (`open_write`, `read_all`, `write_at` via seek+write, `remove`).

### Cache eviction (`evict.rs`)
- `SyncEngine::evict_to(max_bytes)` returns `0` immediately if `cache_bytes_total() <= max_bytes`. Otherwise it requests up to 64 `Store::eviction_candidates` (which exclude `pinned` and `Dirty` nodes), deletes each file on disk (`fs::remove_file`), and removes its `cache_files` entry until total bytes drop to `<= max_bytes`.

### Graceful shutdown (`shutdown.rs`)
- `wait_for_uploads(timeout)` loops until `queue_depth() == 0` or `timeout` elapses, invoking `process_one_upload()` and sleeping 200ms per iteration, returning the remaining queue depth. `pending_uploads()` returns `queue_depth()`.

## Workflow

### Outbound upload worker (`worker.rs`)
1. `process_one_upload()` fetches `Store::next_upload()`. Returns `Ok(false)` if empty.
2. Marks the queue row `"in_progress"` with `attempts + 1`, resolves the node and its parent's `drive_id` (defaulting to `"root"`), looks up `DriveClient`, and loads any staged bytes from `StagingDir`.
3. Dispatches by `op`:
   - `"create"` / `"update"`: reuses `row.resumable_uri` if present, or calls `DriveClient::create_resumable` and saves the returned URI in `upload_queue` as `"in_progress"`. Streams staged bytes in `256 KiB` (`256 * 1024`) chunks via `DriveClient::upload_chunk` (or a single empty-body `0/0` chunk for zero-byte files).
   - `"rename"`: if `drive_id` is non-empty, calls `DriveClient::update_metadata` with `{"name": name}`.
   - `"trash"`: if `drive_id` is non-empty, calls `DriveClient::trash`.
4. On success: increments `transfers`, marks upload `"done"`, updates `drive_id`, `head_revision`, `size`, and sets `node.state = NodeState::Cached`.
5. On error: increments `errors`, resets queue row to `"pending"` with `attempts + 1`, exponential backoff `(1000 * (1 << attempts.min(6))).min(300_000)` ms (1s up to 300s), and preserves `resumable_uri` for crash/retry resumption.
6. `spawn_worker(interval)` runs `process_one_upload()` continuously while draining items, sleeping `interval` when idle or on error.

### Inbound change polling and conflict copies (`poller.rs`, `conflict.rs`, `queue.rs`)
1. `poll_changes(account_id)` reads `Store::get_change_token(account_id)`, or calls `DriveClient::get_start_page_token()` if none is stored yet.
2. Fetches `DriveClient::changes(&page_token)` (incrementing `errors` on failure).
3. For each change:
   - If `ch.removed`: calls `Store::mark_drive_id_trashed(account_id, &ch.file_id)`.
   - If `ch.file` is present: checks if an existing local node is `NodeState::Dirty` AND `revision_conflict(cur.head_revision, f.head_revision_id)` is true (both `Some` and unequal). If so, calls `materialize_conflict_sibling(&store, cur, &hostname_fallback())`:
     - Creates a new sibling node named `"{display_name} (conflict {host} {YYYY-MM-DD})"` with `drive_id: ""` and `state: NodeState::Conflict`.
     - Enqueues a `"create"` upload for the sibling and copies any staged bytes from `cur.id` to `sibling.id`.
   - Upserts the remote file's metadata into `nodes` with `state: NodeState::Placeholder`.
4. Persists `changes.new_start_token` via `set_change_token` and emits `EngineEvent::NodeChanged(0)` if `applied > 0`.
5. `spawn_change_poller(account_id, interval)` spawns a Tokio task returning `tokio::task::JoinHandle<()>` that repeats `poll_changes` every `interval` (allowing callers such as `MountManager` to abort per-account pollers on pause/remove).

## Code map
- `crates/opendrive-sync/src/lib.rs` — `VfsEngine` trait, `PlaceholderEngine`, `Kind`, `NodeAttr`, `DirPage`, `WriteHandle`, `EngineStats`, `EngineEvent`, `SyncError`, and safety invariants.
- `crates/opendrive-sync/src/engine.rs` — `SyncEngine` struct, `SHARED_WITH_ME_DRIVE_ID`, client/staging/limiter setup, `seed_root_account`, `hydrate_folder` (with `sharedWithMe` + shortcut resolution), `placeholder_folder_rel_paths`, `enqueue_upload`, `to_attr`.
- `crates/opendrive-sync/src/vfs_impl.rs` — `VfsEngine` implementation for `SyncEngine`, propagating errors instead of returning empty `DirPage` when hydrating uncached placeholder folders.
- `crates/opendrive-sync/src/worker.rs` — `process_one_upload`, chunked `upload_file`, `spawn_worker`.
- `crates/opendrive-sync/src/poller.rs` — `poll_changes`, `spawn_change_poller` (returning `JoinHandle<()>`).
- `crates/opendrive-sync/src/conflict.rs` — `materialize_conflict_sibling`, `revision_conflict`, `hostname_fallback`, `chrono_date`, and 4 unit tests.
- `crates/opendrive-sync/src/queue.rs` — `UploadOp`, `QueueEntry`, `conflict_name`.
- `crates/opendrive-sync/src/staging.rs` — `StagingDir`, `StagingError`, and 1 unit test.
- `crates/opendrive-sync/src/limiter.rs` — `BandwidthLimiter` token-bucket rate limiter.
- `crates/opendrive-sync/src/evict.rs` — `evict_to`, `delete_cache_file_path`.
- `crates/opendrive-sync/src/shutdown.rs` — `wait_for_uploads`, `pending_uploads`.
- `crates/opendrive-sync/tests/lazy_hydration_tests.rs` — 2 integration tests (`list_lazily_hydrates_placeholder_subfolder_and_marks_cached`, `shared_with_me_shortcuts_and_offline_placeholder_errors`).
- `crates/opendrive-sync/tests/sync_tests.rs` — 3 integration tests (`placeholder_engine_and_bandwidth_limiter`, `sync_engine_vfs_upload_and_remote_changes`, `poller_conflict_sibling_remote_remove_and_eviction`).
- `crates/opendrive-sync/tests/sync_worker_poller_tests.rs` — 3 integration tests (`hydrate_kinds_rehydrate_and_poller_errors_and_new_items`, `worker_multichunk_noop_empty_drive_id_errors_and_shutdown_timeout`, `eviction_partial_stop_and_staging_error`).

## Verification
- Existing tests observed in source (not executed during this documentation pass):
  - `crates/opendrive-sync/src/conflict.rs`: 4 unit tests (`revision_mismatch`, `chrono_date_is_iso_formatted`, `hostname_fallback_is_non_empty`, `materialize_conflict_sibling_enqueues_create`).
  - `crates/opendrive-sync/src/staging.rs`: 1 unit test (`staging_write_read_remove_lifecycle`).
  - `crates/opendrive-sync/tests/lazy_hydration_tests.rs`: 1 integration test (`list_lazily_hydrates_placeholder_subfolder_and_marks_cached`).
  - `crates/opendrive-sync/tests/sync_tests.rs`: 3 integration tests (`placeholder_engine_and_bandwidth_limiter`, `sync_engine_vfs_upload_and_remote_changes`, `poller_conflict_sibling_remote_remove_and_eviction`).
  - `crates/opendrive-sync/tests/sync_worker_poller_tests.rs`: 3 integration tests (`hydrate_kinds_rehydrate_and_poller_errors_and_new_items`, `worker_multichunk_noop_empty_drive_id_errors_and_shutdown_timeout`, `eviction_partial_stop_and_staging_error`).

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Google Drive v3 client](../features/drive-v3-client.md)
- [SQLite store and cache](../architecture/sqlite-store-and-cache.md)
- [OS filesystem adapters](../architecture/os-filesystem-adapters.md)
- [CLI and HTTP bridge](../interfaces/cli-and-http-bridge.md)
