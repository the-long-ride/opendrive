# Status

## Done

- [x] OAuth PKCE + loopback + keyring + refresh
- [x] Drive client (about, list, changes, range, resumable, trash, metadata)
- [x] SQLite store + upload queue + cache eviction helpers
- [x] SyncEngine + staging + write + rename + workers + change poller
- [x] Linux FUSE read **and write** (create/mkdir/write/unlink/rename)
- [x] CLI: authorize, list-root, status, **mount**, --self-check
- [x] Platform dirs, release.yml, docs, deny.toml

## Remaining (need target OS / heavy deps)

- [ ] Windows Cloud Files **live** (`cloud-filter` on Windows host)
- [ ] macOS NFS **live** (`nfsserve` + mount_nfs on macOS host)
- [ ] GPUI window + system tray

## Try on Linux

```bash
cargo run -p opendrive-app -- authorize <id> <secret>
cargo run -p opendrive-app -- mount <access_token> ~/OpenDrive
```
