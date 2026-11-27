---
schema_version: 1
id: "architecture.os-fs-adapters"
title: "OS Filesystem Adapters (Linux FUSE3, macOS NFSv3, Windows CFAPI)"
kind: "architecture"
status: "verified"
summary: "Cross-platform virtual filesystem adapters bridging Linux FUSE3, macOS localhost NFSv3, and Windows Cloud Files API to VfsEngine."
tags: ["vfs", "fuse3", "nfs3", "cfapi", "linux", "macos", "windows"]
source_paths: ["crates/opendrive-fs-linux/src/fuse_helpers.rs", "crates/opendrive-fs-linux/src/fuse_sys.rs", "crates/opendrive-fs-linux/src/inode_table.rs", "crates/opendrive-fs-linux/src/lib.rs", "crates/opendrive-fs-macos/src/lib.rs", "crates/opendrive-fs-macos/src/nfs_backend.rs", "crates/opendrive-fs-macos/src/nfs_vfs.rs", "crates/opendrive-fs-macos/tests/macos_coverage_tests.rs", "crates/opendrive-fs-macos/tests/nfs_vfs_tests.rs", "crates/opendrive-fs-windows/src/cfapi_placeholders.rs", "crates/opendrive-fs-windows/src/cfapi_sys.rs", "crates/opendrive-fs-windows/src/lib.rs", "crates/opendrive-fs-windows/src/provider.rs", "crates/opendrive-fs-windows/tests/windows_coverage_tests.rs", "crates/opendrive-fs-windows/tests/windows_vfs_tests.rs"]
related_specs: ["architecture.workspace-overview", "workflow.sync-hydration-conflicts", "interface.cli-http-bridge"]
last_doc_update_at: "2026-11-27T06:40:00Z"
last_verified_at: "2026-11-27T06:40:00Z"
last_verified_code_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_commit: "375147183a37010b7d5c8bd91768ec7c3dd44fab"
last_source_change_at: "2026-11-26T19:20:18+07:00"
source_fingerprint: "sha256:7122f0a8a38759368666ea0efa9a7440fdf5ba3dd1795926cf5c36d35db6f982"
---

# OS Filesystem Adapters (Linux FUSE3, macOS NFSv3, Windows CFAPI)

## Purpose and boundaries
Translates OS-native virtual filesystem callbacks on Linux (`opendrive-fs-linux`), macOS (`opendrive-fs-macos`), and Windows (`opendrive-fs-windows`) into `opendrive_sync::VfsEngine` calls.

- **In scope**: FUSE3 inode mapping and `fuser::Filesystem` callbacks; localhost NFSv3 server (`nfsserve` 0.11) and Finder metadata filtering; Windows Cloud Files API (`cloud-filter` 0.0.7) sync root registration and `SyncFilter` placeholder/hydration callbacks.
- **Out of scope**: Sync queue scheduling, remote Drive HTTP calls, SQLite cache eviction, and account OAuth (owned by `opendrive-sync`, `opendrive-drive`, `opendrive-store`, and `opendrive-auth`).

## Observable behavior

### Linux FUSE3 (`opendrive-fs-linux`)
- `InodeTable` pre-seeds root inode `1` -> `NodeId(0)` and allocates sequential, idempotent `u64` inodes starting at `2`. `perm_for_kind` returns `0o755` for `Kind::Folder` and `0o644` for all file kinds; `blocks_for_size` rounds up to 512-byte sectors.
- Under `#[cfg(target_os = "linux")]`, `FuseFs` implements `fuser::Filesystem` (`lookup`, `getattr`, `readdir`, `read`, `write`, `create`, `mkdir`, `unlink`, `rmdir`, `rename`) using `tokio::runtime::Handle::block_on` with a 1-second `TTL`. `mount()` requires an active Tokio runtime and mounts via `fuser::mount2` with `FSName("opendrive")`, `AutoUnmount`, and `DefaultPermissions`.
- On non-Linux targets, `mount()` returns `Err("FUSE only on Linux")`. `FuseFrontend::unmount` invokes `fusermount3 -u <path>` and returns `Ok(())`.

### macOS Localhost NFSv3 (`opendrive-fs-macos`)
- `NfsBackend` maintains an opaque `Fh` (`u64`) table pre-seeded with `1` -> `NodeId(0)` and idempotent allocation from `2`. `is_ignored_name` suppresses `.DS_Store` and `._*` AppleDouble entries in both `lookup` and `readdir_meta`; `remove` evicts the deleted `NodeId` from the handle map.
- `nfs_vfs.rs` implements `nfsserve::vfs::NFSFileSystem` (`VFSCapabilities::ReadWrite`, `root_dir() == 1`). Errors map to `NFS3ERR_INVAL` (non-UTF-8 filenames), `NFS3ERR_NOENT` (`bad fh`/`not found`/`NotFound`), `NFS3ERR_EXIST` (`create_exclusive` duplicate), `NFS3ERR_NOTSUPP` (`symlink`/`readlink`), and `NFS3ERR_IO` (other engine failures). `readdir` paginates with `start_after` and `max_entries`.
- `NfsFrontend` generates an export name `od-<nanos_hex>`, binds `127.0.0.1:0` via `bind_loopback_nfs` (`NFSTcpListener`), provides `default_mount_path()` (`$HOME/OpenDrive`), formats `mount_command_hint` (`mount_nfs -o resvport,rw 127.0.0.1:<port>:/<export> <path>`), and creates the mount directory in `serve_and_mount` (binding loopback if called outside a Tokio runtime). `unmount` invokes `umount <path>` and aborts the listener task.

### Windows Cloud Files API (`opendrive-fs-windows`)
- `CloudFilesFrontend` resolves `%USERPROFILE%\OpenDrive` (fallback `HOME`, then `.`) via `parent_folder()` and sibling account roots via `account_root(folder)`. `register_sync_root` delegates to `register_sync_root_at(path, account_name, 0)`, creating the directory and tracking `(SyncRootIdentity, Arc<ProviderBridge>)` bound to a specific VFS `root_node: NodeId` without requiring shell registration. `connect_live` delegates to `connect_live_at(path, account_name, 0, rt)`, which registers the per-account root at `root_node` and invokes `cfapi_sys::connect_sync_root` for multi-account CFAPI root isolation.
- `ProviderBridge` (`ProviderBridge::new` defaults `root_node = 0`; `ProviderBridge::with_root(engine, root_node)` and `root_node()`) caches `PathBuf` -> `NodeId` mappings and wraps `VfsEngine` operations (`fetch_placeholders`, `fetch_data`, `lookup_child`, `create_entry`, `write_data`, `rename_entry`, `delete_entry`, `set_pinned`) returning `String` errors.
- Under `#[cfg(windows)]`, `CfSyncFilter` implements `cloud_filter::filter::SyncFilter`, stripping `\\?\` extended-path prefixes in `rel_path`, resolving `NodeId` from 8-byte little-endian `request.file_blob()` or walking relative path components starting from `self.bridge.root_node()` (so empty relative path resolves to `root_node()` rather than `0`). `fetch_data` aligns reads to `CF_BLOCK_ALIGN` (4096 bytes) clamped to `file_size`, streams in `CF_CHUNK_SIZE` (1 MiB) chunks via `ticket.write_at`, and fails tickets with `MetadataCorrupt`, `NetworkUnavailable`, or `Unsuccessful`. `fetch_placeholders` returns `Err(CloudErrorKind::NetworkUnavailable)` when `ProviderBridge::fetch_placeholders` fails, sanitizes invalid Windows filenames and deduplicates case-insensitive collisions (`cfapi_placeholders.rs`), converts/updates already-existing local folders/files (`ensure_existing_placeholder` opening via `.write_access()` and checking `matches!(ph.info(), Ok(Some(_)))` before choosing `UpdateOptions` vs `ConvertOptions` with `has_children()` + logging `tracing::warn!` on errors), and passes missing entries in batches of `PLACEHOLDER_BATCH_SIZE` (64). `CfSyncFilter` also implements `dehydrate`, `delete`, and `rename`. `cfapi_sys.rs` + `cfapi_placeholders.rs` expose `connect_sync_root`, `unregister_account_sync_root`, `reconcile_placeholder_dirs`, `reconcile_placeholder_entries`, and `dehydrate_placeholder` (likewise using `.write_access()` and `Ok(Some(_))`); `lib.rs` exposes `SYNC_ROOT_PROVIDER`, `try_dehydrate_placeholder`, `unregister_os_sync_root`, `unregister_os_sync_root_by_account`, and `unregister_all_opendrive_roots`.

## Code map
- `crates/opendrive-fs-linux/src/lib.rs` — `FuseFrontend`, cross-platform `mount` stub, and non-Linux unit tests.
- `crates/opendrive-fs-linux/src/inode_table.rs` — `InodeTable` (`1 -> NodeId(0)` mapping, permissions, 512-byte block calculation) and unit tests.
- `crates/opendrive-fs-linux/src/fuse_helpers.rs` — Linux `root_attr`, `attr_from`, `create_inode_attr`, `mount` entry point, and unit tests.
- `crates/opendrive-fs-linux/src/fuse_sys.rs` — Linux `FuseFs` implementing `fuser::Filesystem` (`lookup`, `getattr`, `readdir`, `read`, `write`, `create`, `mkdir`, `unlink`, `rmdir`, `rename`).
- `crates/opendrive-fs-macos/src/lib.rs` — `NfsFrontend` lifecycle (`serve_loopback`, `serve_and_mount`, `mount_command_hint`, `unmount`) and unit tests.
- `crates/opendrive-fs-macos/src/nfs_backend.rs` — `NfsBackend` handle table, AppleDouble/DS_Store filtering, async CRUD wrappers, and unit test.
- `crates/opendrive-fs-macos/src/nfs_vfs.rs` — `NFSFileSystem` trait implementation for `NfsBackend` and `bind_loopback_nfs` TCP listener spawner.
- `crates/opendrive-fs-macos/tests/{macos_coverage_tests,nfs_vfs_tests}.rs` — Integration tests for `NfsFrontend`, pagination, EOF, AppleDouble/`lookup` edge paths, and `NFS3ERR_*` error propagation.
- `crates/opendrive-fs-windows/src/lib.rs` — `SYNC_ROOT_PROVIDER`, `SyncRootIdentity`, `CloudFilesFrontend` (`with_icon_resource`, `register_sync_root_at`, `reconcile_placeholders`, `reconcile_placeholder_entries`, `connect_live_at`, `try_dehydrate_placeholder`, `unregister_sync_root`, `unregister_os_sync_root`, `unregister_os_sync_root_by_account`, `unregister_all_opendrive_roots`, `roots`, `sanitize_sync_root_account`, `is_opendrive_sync_root_id`), and unit tests.
- `crates/opendrive-fs-windows/src/provider.rs` — `ProviderBridge` (`with_root`, `root_node`, relative-path index, `VfsEngine` wrappers) and unit tests.
- `crates/opendrive-fs-windows/src/cfapi_placeholders.rs` — `sanitize_windows_entry_name`, `deduplicate_windows_names`, `ensure_existing_placeholder`, `reconcile_placeholder_dirs`, `reconcile_placeholder_entries`, and unit test.
- `crates/opendrive-fs-windows/src/cfapi_sys.rs` — Windows `CfSyncFilter` (`SyncFilter` impl using `bridge.root_node()`, `\\?\` stripping, existing-entry conversion), `connect_sync_root`, `unregister_account_sync_root`, and `dehydrate_placeholder`.
- `crates/opendrive-fs-windows/tests/{windows_coverage_tests,windows_vfs_tests}.rs` — Integration tests for `ProviderBridge` error propagation, multi-root registration, deterministic `LiveRootGuard` cleanup + non-Windows `unregister_all_opendrive_roots` / `unregister_os_sync_root_by_account` coverage, and Windows `connect_live` error paths.

## Verification
- `opendrive-fs-linux`, `opendrive-fs-macos`, and `opendrive-fs-windows`: all unit and integration suites (`cargo test --workspace --locked`) pass; `fuse_sys.rs` and `cfapi_sys.rs` are excluded from `llvm-cov` in `scripts/test-coverage-rust.mjs`.

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Sync, hydration, and conflict resolution](../workflows/sync-hydration-and-conflicts.md)
- [CLI and HTTP control bridge](../interfaces/cli-and-http-bridge.md)
