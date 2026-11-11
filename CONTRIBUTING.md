# Contributing

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo run -p opendrive-app -- --self-check
```

## Layout

See [docs/architecture.md](docs/architecture.md). Keep UI and OS frontends thin; business logic in core/sync.

## License policy

`deny.toml` allow-list only. No GPL (e.g. no WinFsp).

## PRs

- `cargo fmt` + `clippy -D warnings`
- Tests for store/sync changes
- Do not commit secrets or real OAuth tokens
