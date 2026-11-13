# OpenDrive

Free, open-source multi-account Google Drive desktop client (Rust).

**Status:** Engine + Linux FUSE read/write + CLI mount path working. Windows CFAPI, macOS NFS, and GPUI still stubs.

## Quick start (Linux)

```bash
cargo build -p opendrive-app
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- mount <access_token>
# mounts at ~/OpenDrive
```

## Docs

[Building](docs/building.md) · [Architecture](docs/architecture.md) · [Roadmap](docs/ROADMAP.md) · [OAuth](docs/oauth-setup.md) · [Status](docs/M0-checklist.md)

## License

MIT
