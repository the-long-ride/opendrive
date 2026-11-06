# Building

```bash
rustup show   # stable toolchain from rust-toolchain.toml
cargo build --workspace
cargo test --workspace
cargo run -p opendrive-app -- --self-check
cargo run -p opendrive-app -- ui
```

## VS Code Launch (Windows, Linux, macOS)

- **VS Code `F5` launch profile:** `.vscode/launch.json` provides a single cross-OS configuration, `"OpenDrive: Desktop App"`, running `scripts/launch-desktop.mjs` under VS Code's `node` adapter (`integratedTerminal`).
- **Behavior:** Builds `opendrive-app` via `cargo build -p opendrive-app` and launches the compiled native desktop binary (`target/debug/opendrive[.exe] ui`) directly with `RUST_LOG=info,opendrive=debug` and `RUST_BACKTRACE=1`, cleanly terminating the child process on exit (`SIGINT`/`SIGTERM`).
- **CLI alias:** `npm run desktop` runs the same cross-OS desktop launcher from a terminal.
- **Release size optimization:** Root `Cargo.toml` sets `[profile.release]` with `lto = "thin"`, `codegen-units = 1`, and `strip = "symbols"` for compact release binaries.

## Platform notes

- **Linux:** `fuse3` package required for real mounts later; OpenGL/X11 or Wayland windowing libraries for `eframe` `glow`.
- **Windows:** NTFS + Cloud Files API (Win10 1809+).
- **macOS:** built-in NFS client; no extra deps for NFS path.

Dependencies stay behind crate boundaries; see `deny.toml`.
