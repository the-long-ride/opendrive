# OpenDrive

Free, open-source multi-account Google Drive desktop client.

**Status:** Scaffold (design v0.2). Native Rust engine + per-OS frontends (Cloud Files / FUSE3 / localhost NFS). GPUI UI. BYO OAuth credentials.

## Docs

- [Building](docs/building.md)
- [M0 checklist](docs/M0-checklist.md)
- [OAuth setup](docs/oauth-setup.md)
- Design: see original attachment / local `docs/OpenDrive-design.md`

## Workspace

```
crates/
  opendrive-app/          # binary + (future) GPUI
  opendrive-core/         # domain model
  opendrive-auth/         # OAuth + keychain + loopback
  opendrive-drive/        # Drive v3 client
  opendrive-store/        # SQLite schema
  opendrive-sync/         # VfsEngine
  opendrive-fs-{windows,linux,macos}/
  opendrive-platform/
  opendrive-fake-drive/   # test server
```

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo run -p opendrive-app -- --self-check
```

## License

MIT
