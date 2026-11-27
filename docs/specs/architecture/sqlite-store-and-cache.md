---
schema_version: 1
id: "architecture.sqlite-store"
title: "SQLite metadata store, cache index, and upload queue"
kind: "architecture"
status: "verified"
summary: "Persistent SQLite layer via rusqlite managing hierarchical Drive node metadata, change page tokens, local cache file tracking with eviction-safe queries, and durable upload queue state."
tags: ["architecture", "sqlite", "store", "cache", "upload-queue", "eviction"]
source_paths: ["crates/opendrive-store/src/cache.rs", "crates/opendrive-store/src/lib.rs", "crates/opendrive-store/src/queue_store.rs", "crates/opendrive-store/src/types.rs", "crates/opendrive-store/tests/store_coverage_tests.rs", "crates/opendrive-store/tests/store_tests.rs"]
related_specs: ["architecture.workspace-overview", "workflow.sync-hydration-conflicts"]
last_doc_update_at: "2026-11-26T07:05:00Z"
last_verified_at: "2026-11-26T17:06:00Z"
last_verified_code_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_commit: "375147183a37010b7d5c8bd91768ec7c3dd44fab"
last_source_change_at: "2026-11-26T19:20:18+07:00"
source_fingerprint: "sha256:fb68cf2adcd65cea24922ea79443e886eef5f3b8fd8cbd67abaf6b964fa218cd"
---

# SQLite metadata store, cache index, and upload queue

## Purpose and boundaries
`opendrive-store` provides durable local persistence for filesystem metadata, Drive change-feed tokens, hydrated cache entries, and the outbound upload queue using SQLite (`rusqlite`).

In scope: schema creation/migration, CRUD and query helpers for `nodes`, `change_tokens`, `cache_files`, and `upload_queue`, and enforcement of the cache eviction safety filter.
Out of scope: on-disk cache/staging file I/O (`opendrive-sync`), Drive HTTP transfers (`opendrive-drive`), and account/settings JSON files (`opendrive-core`).

## Observable behavior

### Connection lifecycle and schema
- `Store::open(path)` opens a file-backed SQLite database, enables `PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;`, and executes `SCHEMA_SQL`.
- `Store::open_in_memory()` opens an in-memory database with `PRAGMA foreign_keys=ON;` and executes `SCHEMA_SQL`.
- `SCHEMA_SQL` defines four tables:
  - `nodes`: `id` (PK), `account_id`, `drive_id`, `parent_id`, `name`, `display_name`, `kind`, `mime`, `size`, `md5`, `modified_ms`, `head_revision`, `trashed`, `shared_drive_id`, `state` (`'placeholder'` default), `pinned` (`0` default), `last_access_ms`. Has unique partial index `idx_nodes_acct_drive ON nodes(account_id, drive_id) WHERE drive_id != ''` (allowing multiple unsynced local nodes with empty `drive_id`).
  - `change_tokens`: `account_id` (PK), `page_token`, `updated_ms`.
  - `cache_files`: `node_id` (PK), `path`, `bytes_present`, `ranges` (`BLOB`), `pinned`.
  - `upload_queue`: `op_id` (`AUTOINCREMENT` PK), `node_id`, `op`, `state`, `attempts`, `next_try_ms`, `resumable_uri`, `base_revision`.

### Node states and metadata operations (`lib.rs`, `types.rs`)
- `NodeState` maps between Rust variants and strings: `Placeholder` (`"placeholder"`), `Hydrating` (`"hydrating"`), `Cached` (`"cached"`), `Dirty` (`"dirty"`), `Conflict` (`"conflict"`). Unknown database strings parse to `Placeholder`.
- `next_id()` returns `COALESCE(MAX(id), 0) + 1` from `nodes`.
- `upsert_node(&Node)` inserts or updates on `id` conflict (note: the `ON CONFLICT(id) DO UPDATE` clause updates `drive_id`, `parent_id`, `name`, `display_name`, `kind`, `mime`, `size`, `md5`, `modified_ms`, `head_revision`, `trashed`, `state`, `pinned`, `last_access_ms`, leaving `account_id` and `shared_drive_id` unchanged).
- `find_by_drive_id(account_id, drive_id)` and `get_node(id)` return `Option<Node>`.
- `list_children(parent_id)` returns non-trashed children (`trashed=0`) ordered lexicographically by `name`; `list_children_sorted(parent_id)` orders folders first then case-insensitively by display/name for Explorer views.
- `update_node_state(node_id, state, pinned)` updates `nodes.state` (and optionally `nodes.pinned`).
- `mark_drive_id_trashed(account_id, drive_id)` sets `trashed=1` and returns whether any row changed.
- `set_change_token(account_id, token)` upserts `page_token` with current `now_ms()`; `get_change_token(account_id)` retrieves it.

### Content cache index and eviction safety (`cache.rs`)
- `set_cache_entry(node_id, path, bytes_present, pinned)` upserts a `cache_files` row.
- `get_cache_path(node_id)`, `cache_bytes_total()` (`SUM(bytes_present)`), `remove_cache_entry(node_id)`, `clear_node_cache_state(node_id)` (removes `cache_files` row and resets node to `Placeholder` + `pinned=0`), `trash_node_and_clear(node_id)` (removes `cache_files` row and sets `trashed=1, pinned=0`), `relocate_cache_prefix(old_path, new_path)` (updates `cache_files.path` entries matching a renamed/moved local file or folder prefix), and `touch_access(node_id)` (updates `nodes.last_access_ms = now_ms()`) manage cache metadata.
- `eviction_candidates(limit)` enforces Safety Invariant #3 by joining `cache_files c` with `nodes n` and selecting only rows where `c.pinned = 0 AND n.pinned = 0 AND n.state != 'dirty'`, ordered by `n.last_access_ms ASC` up to `limit`.

### Durable upload queue (`queue_store.rs`)
- `enqueue_upload(node_id, op, base_revision)` inserts a `'pending'` row with `attempts = 0` and `next_try_ms = now_ms()`, returning `op_id`.
- `next_upload()` returns the oldest eligible row (`state IN ('pending', 'in_progress') AND next_try_ms <= now_ms()`) ordered by `next_try_ms ASC LIMIT 1`; `next_upload_for_account(account_id)` scopes the next eligible upload to one account.
- `mark_node_uploads_done(node_id)` closes pending queue rows after verified replica upload/clear.
- `update_upload(op_id, state, attempts, next_try_ms, resumable_uri)` updates queue row state, retry scheduling, and resumable session URI.
- `queue_depth()` counts rows in `'pending'` or `'in_progress'`.

## Interfaces and data
- Exposed types: `Store`, `Node`, `NodeState`, `UploadRow`, `StoreError`, `Result<T>`, `SCHEMA_SQL`.
- All `rusqlite::Error` values convert into `StoreError::Db(String)`.

## Code map
- `crates/opendrive-store/src/lib.rs` — `Store::open`, `open_in_memory`, `migrate`, `next_id`, `upsert_node`, `find_by_drive_id`, `get_node`, `list_children`, `list_children_sorted`, `update_node_state`, `mark_drive_id_trashed`, `set_change_token`, `get_change_token`.
- `crates/opendrive-store/src/types.rs` — `StoreError`, `NodeState`, `Node` (`Node::effective_name`, `Node::is_folder`, `Node::is_native_doc`), `UploadRow`, `row_to_node`, `now_ms`, `SCHEMA_SQL`.
- `crates/opendrive-store/src/cache.rs` — `set_cache_entry`, `get_cache_path`, `cache_bytes_total`, `eviction_candidates`, `remove_cache_entry`, `clear_node_cache_state`, `trash_node_and_clear`, `relocate_cache_prefix`, `touch_access`.
- `crates/opendrive-store/src/queue_store.rs` — `enqueue_upload`, `next_upload`, `next_upload_for_account`, `mark_node_uploads_done`, `update_upload`, `queue_depth`.
- `crates/opendrive-store/tests/{store_coverage_tests,store_tests}.rs` — Integration tests covering node/token CRUD, sorted listing, per-account upload queue scheduling/deferral, cache state clearing, migration/error paths, and eviction candidate invariants.

## Verification
- `cargo test -p opendrive-store --locked`: all integration tests in `crates/opendrive-store/tests/{store_tests,store_coverage_tests}.rs` pass.

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Sync, hydration, and conflicts](../workflows/sync-hydration-and-conflicts.md)
