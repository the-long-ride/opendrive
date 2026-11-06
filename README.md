# OpenDrive

Free, open-source multi-account Google Drive desktop client.

**Status:** Scaffold (design v0.2). Native Rust engine + per-OS frontends (Cloud Files / FUSE3 / localhost NFS). GPUI UI. BYO OAuth credentials.

See `docs/OpenDrive-design.md` for full design.

## Workspace

```
crates/
  opendrive-app/          # GPUI UI + binary
  opendrive-core/         # domain model
  opendrive-auth/         # OAuth + keychain
  opendrive-drive/        # Drive v3 client
  opendrive-store/        # SQLite
  opendrive-sync/         # VfsEngine
  opendrive-fs-{windows,linux,macos}/
  opendrive-platform/
  opendrive-fake-drive/
```

## Build

```bash
cargo build --workspace
cargo run -p opendrive-app
```

## License

MIT
