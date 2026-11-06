# Building

```bash
rustup show   # expects 1.85+ from rust-toolchain.toml
cargo build --workspace
cargo test --workspace
cargo run -p opendrive-app -- --self-check
```

## Platform notes

- **Linux:** `fuse3` package required for real mounts later.
- **Windows:** NTFS + Cloud Files API (Win10 1809+).
- **macOS:** built-in NFS client; no extra deps for NFS path.

Dependencies stay behind crate boundaries; see `deny.toml`.
