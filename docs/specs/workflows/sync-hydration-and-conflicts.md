---
schema_version: 1
id: "workflow.sync-hydration-conflicts"
title: "Sync engine, folder hydration, upload queue worker, eviction, and conflict handling"
kind: "workflow"
status: "verified"
summary: "Store-backed VfsEngine implementation orchestrating folder hydration, staged writes, durable resumable uploads, 2 MiB chunked StreamCache with head+tail prewarm and read-ahead, explicit Sync Down / Sync Up / Safety-Invariant-#3 Clear Local replica operations, remote change polling, conflict-copy materialization, bandwidth limiting, cache eviction, and shutdown draining."
tags: ["workflow", "sync", "vfs", "hydration", "streaming", "stream-cache", "replica", "upload-worker", "poller", "conflict", "eviction", "staging"]
source_paths: ["crates/opendrive-sync/src/conflict.rs", "crates/opendrive-sync/src/engine.rs", "crates/opendrive-sync/src/evict.rs", "crates/opendrive-sync/src/file_copy.rs", "crates/opendrive-sync/src/file_ops.rs", "crates/opendrive-sync/src/hydrate.rs", "crates/opendrive-sync/src/lib.rs", "crates/opendrive-sync/src/limiter.rs", "crates/opendrive-sync/src/path_time.rs", "crates/opendrive-sync/src/poller.rs", "crates/opendrive-sync/src/queue.rs", "crates/opendrive-sync/src/replica_clear.rs", "crates/opendrive-sync/src/replica_ops.rs", "crates/opendrive-sync/src/replica_upload.rs", "crates/opendrive-sync/src/shutdown.rs", "crates/opendrive-sync/src/staging.rs", "crates/opendrive-sync/src/stream_cache.rs", "crates/opendrive-sync/src/stream_store.rs", "crates/opendrive-sync/src/vfs_impl.rs", "crates/opendrive-sync/src/worker.rs", "crates/opendrive-sync/tests/file_ops_tests.rs", "crates/opendrive-sync/tests/hydrate_cache_reconcile_tests.rs", "crates/opendrive-sync/tests/lazy_hydration_tests.rs", "crates/opendrive-sync/tests/root_discovery_tests.rs", "crates/opendrive-sync/tests/stream_extra_tests.rs", "crates/opendrive-sync/tests/stream_replica_tests.rs", "crates/opendrive-sync/tests/sync_branch_cov_tests.rs", "crates/opendrive-sync/tests/sync_coverage_tests.rs", "crates/opendrive-sync/tests/sync_edge_cov_tests.rs", "crates/opendrive-sync/tests/sync_final_cov_tests.rs", "crates/opendrive-sync/tests/sync_ops_coverage_tests.rs", "crates/opendrive-sync/tests/sync_tests.rs", "crates/opendrive-sync/tests/sync_worker_copy_coverage_tests.rs", "crates/opendrive-sync/tests/sync_worker_poller_tests.rs"]
related_specs: ["architecture.workspace-overview", "feature.drive-v3-client", "architecture.sqlite-store", "architecture.os-fs-adapters", "interface.cli-http-bridge"]
last_doc_update_at: "2026-11-26T07:07:19Z"
last_verified_at: "2026-11-26T07:07:19Z"
last_verified_code_commit: "aa2135c5d17ce0016d4fa42bb2c2705bbba7c709"
last_source_change_commit: "600d2df0ad0c4d97420d87fa94048e4eac1b8129"
last_source_change_at: "2026-11-26T14:29:37+07:00"
source_fingerprint: "sha256:ccfed3cc6a819f06dee3dbf4615764a6d0772d65634b1d457abee77bdcf0b0b2"
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
- `VfsEngine` defines 13 async operations (`list`, `lookup`, `stat`, `read`, `open_write`, `write`, `create`, `rename`, `remove`, `set_pinned`, `add_account`, `remove_account`, `stats`) and synchronous `subscribe`. `PlaceholderEngine` provides a no-op/empty stub implementation.
- `SyncEngine::new(store)` holds `Arc<Mutex<Store>>`, a per-account `HashMap<Uuid, DriveClient>`, cached real root Drive IDs (`root_drive_ids`), an optional `mpsc::Sender<EngineEvent>` (`subscribe` creates a bounded channel of capacity 64), an optional `StagingDir`, a `BandwidthLimiter`, atomic `transfers` / `errors` counters, and exposes `get_node(node_id)`.
- `seed_root_account(account_id, folder_name)` idempotently creates the root folder node (`drive_id: "root"`, `parent_id: Some(0)`, `state: NodeState::Cached`).
- `hydrate_folder(account_id, parent_node)` (`hydrate.rs`) dispatches `SHARED_WITH_ME_DRIVE_ID` to `DriveClient::list_shared_with_me` and normal folders to `DriveClient::list_files(&drive_parent, page_token, true)` (creating/repairing the `"Shared with me"` child under `root`), resolves canonical real root IDs via `ensure_real_root_id`, merges top-level unparented owned and link-opened items via `list_top_unparented_files` under `root` (routing `shared_with_me == Some(true)` items to `"Shared with me"`), parses RFC3339 `modifiedTime` into `modified_ms` (`path_time.rs`), resolves Google Drive shortcuts (`shortcutDetails` -> effective target `drive_id` and `mime_type`), reconciles cached state via shared `reconcile_cached_state` (preserving `NodeState::Cached` when `md5` & `head_revision` match, or deleting stale `cache_files`/disk replicas and evicting stream chunks when changed), prunes remotely disappeared clean children (while preserving `Dirty`/`Conflict`/local unsynced nodes and `sharedWithMe`), and never overwrites `NodeState::Dirty`.
- `node_rel_path(root_node, node_id)` and `resolve_remote_path(root_node, remote_path)` (exact match first, case-insensitive fallback) map between SQLite `nodes` and relative Drive paths.
- `placeholder_folder_rel_paths(root_node)` walks stored `NodeState::Placeholder` leaf folders under a `root_node` (`NodeId`) to return relative filesystem paths for Windows CFAPI on-demand reconciliation.
- `list` queries `Store::list_children` (non-trashed); if `children.is_empty()` and `node > 0`, and the target node is a `"folder"` with non-empty `drive_id` and either `state == NodeState::Placeholder` or `drive_id == SHARED_WITH_ME_DRIVE_ID`, `list` requires a registered `DriveClient` (returning `Err` rather than an empty `DirPage` if offline/missing), calls `self.hydrate_folder(folder.account_id, node)?`, updates `folder.state = NodeState::Cached` only after hydration succeeds (reverting to/preserving `Placeholder` on error so folders are never sealed empty), and re-queries `Store::list_children`. `lookup` queries `Store::list_children`; `stat` queries `Store::get_node`.
- `read(node, offset, len)` checks `StagingDir::read_all(node)` first and returns the staged slice (throttled by `BandwidthLimiter`) if non-empty and in bounds; otherwise delegates to `read_cached_range(node, offset, len, 2)`.

### Chunked `StreamCache` and read-ahead (`stream_cache.rs`, `stream_store.rs`)
- `STREAM_CHUNK_BYTES` = `2 MiB` (`2 * 1024 * 1024`). `StreamCache` stores chunks in a bounded 24-chunk in-memory LRU (`MemChunkLru`) and optionally on disk (`with_stream_cache_dir` / `set_stream_cache_dir`) under `<dir>/<node_id>/<chunk_idx>.chk` keyed by `.rev` revision fingerprint.
- `prewarm_head_and_tail(node_id)` concurrently fetches chunk `0` and the last chunk via `tokio::join!` so MP4/MOV `moov` header/trailer metadata is cached before playback begins.
- `read_cached_range(node_id, offset, len, read_ahead_chunks)` coalesces concurrent fetches per `(node_id, chunk_idx)` via `InFlightMap` + `tokio::sync::Notify`, throttles downloads with `BandwidthLimiter::take`, spawns background prefetching for the next `read_ahead_chunks` missing chunks, and falls back to uncached `download_range` when `size == 0`. `evict_node_stream_chunks(node_id)` purges memory and disk chunks.

### Explicit Explorer replica sync & Safety Invariant #3 (`replica_ops.rs`, `replica_upload.rs`, `replica_clear.rs`)
- `list_cached_children(folder_node)` returns sorted non-trashed local SQLite children without triggering remote hydration. `list_explorer_nodes(account_id, folder_node, force_refresh)` returns sorted non-trashed children (`list_children_sorted`), hydrating on demand (`Placeholder`, empty `"root"`, or empty `SHARED_WITH_ME_DRIVE_ID`), when `force_refresh` is true, or via best-effort live refresh on `"root"` and `SHARED_WITH_ME_DRIVE_ID` (falling back to cached children if offline when non-empty).
- `sync_down_node(account_id, root_node, node_id, local_account_root, cache_dir)` recursively downloads files atomically via `.tmp` rename, updates `cache_files` + `NodeState::Cached` (`pinned = true`), and skips native docs/shortcuts.
- `sync_up_local_path(account_id, parent_node, source_path, local_account_root)` uploads local files and recursive directories (creating remote folders via `create_remote_folder_node`), closes pending node uploads (`mark_node_uploads_done`) + staging files, and flushes queued account uploads (`flush_account_uploads`).
- `sync_up_node` uploads dirty/local/staged node changes in place while keeping local replicas intact; `sync_up_and_clear_local` enforces Safety Invariant #3 in two phases: Phase 1 uploads and verifies all dirty/local/staged data (refusing empty-byte overwrite when a non-empty `Dirty` node lacks readable bytes) and Phase 2 removes regular non-reparse local replicas, staging files, `cache_files` rows, and stream chunks, resetting cleared folders to `NodeState::Placeholder`.
- Cloud + Local item manipulation (`file_ops.rs`, `file_copy.rs`): `delete_node_everywhere` trashes on Drive and removes local disk files/folders, `cache_files`, staging, and stream chunks; `rename_node_everywhere` validates item names/sibling collisions, calls `update_metadata`, renames local mounted disk paths, and calls `relocate_cache_prefix`; `move_node_everywhere` calls `move_file` (`addParents`/`removeParents`) while guarding against folder cycles and relocating local disk/`cache_files` paths; `copy_node_everywhere` copies Cloud/Cached files via `DriveClient::copy_file` (or `upload_bytes_verified` for Dirty/unsynced files), hydrates and recursively copies Cloud folder subtrees, auto-disambiguates `" (copy)"` names on collision, and copies local bytes into `local_account_root` or `cache_files` when the source is cached locally. All mutations guard against virtual `"root"` and `SHARED_WITH_ME_DRIVE_ID`.
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
5. On error: increments `errors`, resets queue row to `"pending"` with `attempts + 1`, exponential backoff `(1000 * (1 << attempts.min(6))).min(300_000)` ms (1s up to 64s, capped at 300s), and preserves `resumable_uri` for crash/retry resumption.
6. `spawn_worker(interval)` runs `process_one_upload()` continuously while draining items, sleeping `interval` when idle or on error.

### Inbound change polling and conflict copies (`poller.rs`, `conflict.rs`, `queue.rs`)
1. `poll_changes(account_id)` reads `Store::get_change_token(account_id)`, or calls `DriveClient::get_start_page_token()` if none is stored yet.
2. Fetches `DriveClient::changes(&page_token)` (incrementing `errors` on failure).
3. For each change:
   - If `ch.removed`: calls `Store::mark_drive_id_trashed(account_id, &ch.file_id)`.
   - If `ch.file` is present: resolves effective target ID/MIME/kind (`resolve_drive_target`). If an existing local node is `NodeState::Dirty`, skips overwriting unless `revision_conflict(cur.head_revision, f.head_revision_id)` is true (both `Some` and unequal), in which case it calls `materialize_conflict_sibling(&store, cur, &hostname_fallback())`:
     - Creates a new sibling node named `"{display_name} (conflict {host} {YYYY-MM-DD})"` with `drive_id: ""` and `state: NodeState::Conflict`.
     - Enqueues a `"create"` upload for the sibling and copies any staged bytes from `cur.id` to `sibling.id`.
   - Upserts the remote file's metadata into `nodes`, resolving real root IDs (`root_drive_ids`) via `resolve_parent_node_id`, preserving `existing.parent_id` when `f.parents` is empty or unindexed, routing empty-parent items without existing parents to `"root"` (or `"Shared with me"` when `shared_with_me == Some(true)`), reconciling cached state via `reconcile_cached_state`, and parsing `modifiedTime` via `parse_rfc3339_ms`.
4. Persists `changes.new_start_token` via `set_change_token` and emits `EngineEvent::NodeChanged(0)` if `applied > 0`.
5. `spawn_change_poller(account_id, interval)` spawns a Tokio task returning `tokio::task::JoinHandle<()>` that repeats `poll_changes` every `interval` (allowing callers such as `MountManager` to abort per-account pollers on pause/remove).

## Code map
- `crates/opendrive-sync/src/lib.rs` — `VfsEngine` trait, `PlaceholderEngine`, `Kind`, `NodeAttr`, `DirPage`, `WriteHandle`, `EngineStats`, `EngineEvent`, `SyncError`, `SyncSummary`, and safety invariants.
- `crates/opendrive-sync/src/engine.rs` — `SyncEngine` struct, `SHARED_WITH_ME_DRIVE_ID`, client/staging/stream-cache/limiter setup, `seed_root_account`, `enqueue_upload`, `to_attr`.
- `crates/opendrive-sync/src/hydrate.rs` — `ensure_real_root_id`, `ensure_shared_with_me_node`, `resolve_parent_node_id`, `hydrate_folder`, `upsert_hydrated_entry`, and `resolve_drive_target`.
- `crates/opendrive-sync/src/path_time.rs` — `parse_rfc3339_ms`, `sanitize_name`, `placeholder_folder_rel_paths`, `node_rel_path`, `resolve_remote_path`.
- `crates/opendrive-sync/src/stream_cache.rs` — `STREAM_CHUNK_BYTES` (2 MiB), `StreamCache`, `prewarm_head_and_tail`, `read_cached_range`, `spawn_ahead`, `evict_node_stream_chunks`.
- `crates/opendrive-sync/src/stream_store.rs` — `MemChunkLru`, disk `.chk`/`.rev` chunk persistence, `InFlightMap`, `FlightGuard` single-flight coalescing.
- `crates/opendrive-sync/src/replica_ops.rs` — `SyncSummary`, `list_cached_children`, `list_explorer_nodes`, `sync_down_node`, recursive folder sync-down.
- `crates/opendrive-sync/src/replica_upload.rs` — `sync_up_local_path`, `flush_account_uploads`, remote folder creation, `upload_bytes_verified`.
- `crates/opendrive-sync/src/replica_clear.rs` — `sync_up_node`, `sync_up_and_clear_local` (two-phase Safety Invariant #3 upload verification + reparse-safe local clearing), `is_dehydrated_placeholder`, `is_reparse_point`.
- `crates/opendrive-sync/src/file_ops.rs` & `crates/opendrive-sync/src/file_copy.rs` — `delete_node_everywhere`, `rename_node_everywhere`, `move_node_everywhere`, `copy_node_everywhere`, name/system-folder/cycle guards, and local/`cache_files` lockstep synchronization.
- `crates/opendrive-sync/src/vfs_impl.rs` — `VfsEngine` implementation for `SyncEngine` (routing `read` through `read_cached_range`).
- `crates/opendrive-sync/src/worker.rs` — `process_one_upload`, `process_upload_row`, chunked `upload_file`, `spawn_worker`.
- `crates/opendrive-sync/src/poller.rs` — `poll_changes`, `spawn_change_poller` (returning `JoinHandle<()>`), and unit tests.
- `crates/opendrive-sync/src/conflict.rs` — `materialize_conflict_sibling`, `revision_conflict`, `hostname_fallback`, `chrono_date`, and unit tests.
- `crates/opendrive-sync/src/queue.rs` — `UploadOp`, `QueueEntry`, `conflict_name`.
- `crates/opendrive-sync/src/staging.rs` — `StagingDir`, `StagingError`, and unit tests.
- `crates/opendrive-sync/src/limiter.rs` — `BandwidthLimiter` token-bucket rate limiter.
- `crates/opendrive-sync/src/evict.rs` — `evict_to`, `delete_cache_file_path`, and unit tests.
- `crates/opendrive-sync/src/shutdown.rs` — `wait_for_uploads`, `pending_uploads`.
- `crates/opendrive-sync/tests/{file_ops_tests,hydrate_cache_reconcile_tests,lazy_hydration_tests,root_discovery_tests,stream_extra_tests,stream_replica_tests,sync_branch_cov_tests,sync_coverage_tests,sync_edge_cov_tests,sync_final_cov_tests,sync_ops_coverage_tests,sync_tests,sync_worker_copy_coverage_tests,sync_worker_poller_tests}.rs` — Integration tests verifying remote deletion pruning, lazy hydration, root discovery, streaming & replica ops, upload workers, conflict siblings, eviction, file operations, and error-path recovery.

## Verification
- Verified in `opendrive-sync` (`cargo test -p opendrive-sync --locked`):
  - Unit tests in `src/{conflict,evict,path_time,poller,staging,stream_store}.rs` and all 14 integration test suites in `crates/opendrive-sync/tests/*.rs` pass.

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Google Drive v3 client](../features/drive-v3-client.md)
- [SQLite store and cache](../architecture/sqlite-store-and-cache.md)
- [OS filesystem adapters](../architecture/os-filesystem-adapters.md)
- [CLI and HTTP bridge](../interfaces/cli-and-http-bridge.md)
