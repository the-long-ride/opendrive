# Architecture (one-pager)

```
opendrive-app (GPUI)
        ↓ channels
opendrive-core          accounts, settings, orchestration
opendrive-auth          OAuth PKCE + loopback + keychain
opendrive-drive         Drive v3 HTTP client
opendrive-store         SQLite nodes / cache / upload queue
opendrive-sync          VfsEngine (change track, hydrate, write-back)
        ↓
fs-windows | fs-linux | fs-macos   thin OS adapters
opendrive-platform      paths, tray, autostart, single-instance
```

Rules:
- core/auth/drive/store/sync must not depend on GPUI or OS FS crates.
- Frontends only translate OS callbacks → VfsEngine.
- Third-party types stay behind crate boundaries.

See full design for sync invariants and milestones.
