# OpenDrive

Free, open-source multi-account Google Drive desktop client.

**Status:** M0 engine path largely complete (auth, Drive, store, sync, FUSE read, CLI). Live CFAPI/NFS and GPUI still pending.

## Docs

- [Building](docs/building.md) · [Architecture](docs/architecture.md)
- [M0 checklist](docs/M0-checklist.md) · [Open questions](docs/open-questions.md)
- [OAuth setup](docs/oauth-setup.md) · [GPUI spike](docs/gpui-spike.md)

## CLI

```bash
cargo run -p opendrive-app -- --self-check
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- list-root <access_token>
cargo run -p opendrive-app -- status
```

## License

MIT
