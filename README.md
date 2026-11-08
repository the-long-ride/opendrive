# OpenDrive

Free, open-source multi-account Google Drive desktop client.

**Status:** M0 in progress — auth, Drive client, store, sync engine, FUSE read path, CLI. CFAPI/NFS/GPUI still stubs.

## Docs

- [Building](docs/building.md)
- [Architecture](docs/architecture.md)
- [M0 checklist](docs/M0-checklist.md)
- [Open questions](docs/open-questions.md)
- [OAuth setup](docs/oauth-setup.md)

## Quick CLI

```bash
cargo run -p opendrive-app -- --self-check
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- list-root <access_token>
```

## License

MIT
