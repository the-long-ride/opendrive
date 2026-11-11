# Changelog

## Unreleased (0.1.0)

### Added
- Cargo workspace: core, auth, drive, store, sync, fs-*, platform, fake-drive, app
- OAuth PKCE + loopback + keyring + token refresh
- Drive v3 client: about, list, changes, range download, resumable upload, trash
- SQLite store: nodes, change tokens, upload queue
- SyncEngine: VfsEngine, hydrate, staging, upload worker, change poller
- Linux FUSE read path (fuser)
- CLI: authorize, list-root, status, --self-check
- AppController (UI-agnostic)
- CFAPI/NFS op maps (stubs)
- CI, release.yml skeleton, deny.toml, docs
