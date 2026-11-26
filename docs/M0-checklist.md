# Status

## Done

- Engine path (`opendrive-auth`, `opendrive-drive`, `opendrive-fake-drive`, `opendrive-store`, `opendrive-sync`)
- Cross-platform VFS frontends:
  - Windows CFAPI (`opendrive-fs-windows` using `cloud-filter` `SyncFilter` callbacks)
  - macOS NFSv3 (`opendrive-fs-macos` using `nfsserve` loopback server)
  - Linux FUSE3 (`opendrive-fs-linux`)
- Sync safety invariants: crash-safe upload queue, revision conflict sibling materialization, token-bucket bandwidth limiter, unpinned/clean LRU cache eviction, shutdown drain
- Seijaku Cel Terminal Control Center Web UI (`ui/` + `ControlServer` loopback HTTP API in `opendrive-app`)
- Optional GPUI window (`--features gpui -- ui`) and system tray (`--features tray -- tray`)
- Automated quality gate (`npm run test:gate`): Prettier, `cargo fmt`, `loc-lint`, `knip`, `clippy --locked -D warnings`, `tsc`, Node tests, and >=95% coverage across Node and Rust

## Verify locally

```bash
npm run build:ui
cargo run -p opendrive-app -- --self-check
cargo run -p opendrive-app -- ui --no-open
npm run test:gate
```
