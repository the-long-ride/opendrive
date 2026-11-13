# Changelog

## Unreleased (0.1.0)

### Added
- Full workspace: core, auth, drive, store, sync, fs-*, platform, fake-drive, app
- OAuth PKCE + loopback + keyring
- Drive v3 client + resumable upload
- SQLite store, upload queue, cache eviction
- SyncEngine with staging writes, workers, change poller
- Linux FUSE: lookup/read/write/create/mkdir/unlink/rename
- CLI: authorize, list-root, status, mount, --self-check
- Platform config/cache/data/mount paths via `dirs`
- CI + release.yml skeleton
