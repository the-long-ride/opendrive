# Architecture (one-pager)

```
ui/ (TypeScript + CSS bundle)
        ↕ HTTP loopback (127.0.0.1)
opendrive-app (ControlServer + Bridge + CLI + optional GPUI/tray)
        ↓ channels & Arc<SyncEngine>
opendrive-core          accounts, settings, orchestration
opendrive-auth          OAuth PKCE + loopback + keychain
opendrive-drive         Drive v3 HTTP client (tested via opendrive-fake-drive)
opendrive-store         SQLite nodes / cache / upload queue
opendrive-sync          VfsEngine (change track, hydrate, write-back, conflicts, eviction)
        ↓
fs-windows (CFAPI) | fs-linux (FUSE3) | fs-macos (NFSv3)
opendrive-platform      paths, tray model, autostart, single-instance
```

Rules:

- `core`/`auth`/`drive`/`store`/`sync` must not depend on UI or OS FS crates.
- Frontends only translate OS callbacks to `VfsEngine`.
- Third-party VFS types stay behind crate boundaries.
- UI runs on loopback `127.0.0.1` via `ControlServer` and reads/mutates state through `Bridge`.

See [OpenDrive-design.md](OpenDrive-design.md) and [DESIGN.md](../DESIGN.md) for sync invariants and UI architecture.
