# OpenDrive

Free, open-source multi-account Google Drive desktop client.

**Status:** Scaffold (design v0.2) — types, traits, stubs, tests. Real M0 spikes not yet implemented.

## Docs

- [Building](docs/building.md)
- [Architecture](docs/architecture.md)
- [M0 checklist](docs/M0-checklist.md)
- [OAuth setup](docs/oauth-setup.md)

## Workspace

| Crate | State |
|-------|-------|
| opendrive-core | AccountManager + tests |
| opendrive-auth | PKCE/loopback skeletons |
| opendrive-drive | API surface stubs |
| opendrive-store | SCHEMA_SQL + NodeState |
| opendrive-sync | VfsEngine + queue/conflict |
| opendrive-fake-drive | in-memory + tests |
| opendrive-fs-* | mount stubs |
| opendrive-platform | paths/tray stubs |
| opendrive-app | binary + `--self-check` |

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo run -p opendrive-app -- --self-check
```

## License

MIT
