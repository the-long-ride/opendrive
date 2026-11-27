# OpenDrive Agent Navigation & Quality Notes

- **Architecture & specs index:** [docs/specs/INDEX.md](docs/specs/INDEX.md) and [CONTRIBUTING.md](CONTRIBUTING.md). Keep `crates/` logic layered (`core`/`sync` beneath `app` and `fs-*`).
- **Quality gate (`npm run test:gate`):** format check (`prettier` + `cargo fmt --check`), lint (`oxlint`, `loc-lint`, `knip`, cross-crate `clippy`), `npm run validate:specs`, JS tests + >=95% `c8` coverage, and Rust >=95% `cargo llvm-cov`. Install git hook once via `npm run hooks:install`.
- **Hard LOC limits (`scripts/loc-lint.mjs`):** `.rs` files <= **300** _post-`cargo fmt`_ lines (budget ~230 raw lines when authoring); `.mjs` files <= **350** lines. Run `cargo fmt --all` before `npm run lint:loc`.
- **Windows / shell rules:**
  - If `opendrive.exe` is running, run targeted Cargo tests with `--target-dir target/llvm-cov-target` to avoid `os error 5` binary lock errors.
  - In `pwsh`, pass multi-line Python via `@' ... '@ | python -` (avoid `{}` in double-quoted `python -c`); use `powershell.exe -File` for WinRT APIs (`StorageProviderSyncRootManager`).
  - Always pass explicit `cwd` on `fast_apply_edits` / `fast_batch_search` calls.
  - Keep local `rustup update stable` aligned with GitHub CI Clippy; push via HTTPS (`gh auth git-credential`) if SSH keys are unavailable.
