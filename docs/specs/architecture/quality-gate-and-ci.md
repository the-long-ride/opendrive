---
schema_version: 1
id: "architecture.quality-gate"
title: "Quality Gate, Desktop Launcher, and CI Scripts"
kind: "architecture"
status: "verified"
summary: "Node.js scripts enforcing LOC/compression limits, static safety checks, cross-OS desktop binary launching, PR change routing, and 90% coverage gates."
tags: ["ci", "quality-gate", "lint", "loc", "coverage", "desktop-launch"]
source_paths: ["scripts/ci-changes.mjs", "scripts/launch-desktop.mjs", "scripts/lib/desktop-launch.mjs", "scripts/lib/source-policy.mjs", "scripts/lint.mjs", "scripts/loc-lint.mjs", "scripts/test-coverage-node.mjs", "scripts/test-coverage-rust.mjs", "scripts/test/ci-changes.test.mjs", "scripts/test/desktop-launch.test.mjs", "scripts/test/loc-lint.test.mjs", "scripts/test/quality-gate.test.mjs", "scripts/test/source-policy.test.mjs"]
related_specs: ["architecture.workspace-overview", "feature.control-center-ui", "interface.cli-http-bridge"]
last_doc_update_at: "2026-11-18T07:15:00Z"
last_verified_at: "2026-11-18T07:15:00Z"
last_verified_code_commit: "018d1c9aa32bb78dfe351da53e12305b69e679c8"
last_source_change_commit: "018d1c9aa32bb78dfe351da53e12305b69e679c8"
last_source_change_at: "2026-11-18T07:15:00Z"
source_fingerprint: "sha256:94256372856a223608d23c31c3d774db0d1c3f51e48a36237c841da042cc1faf"
---
# Quality Gate, Desktop Launcher, and CI Scripts

## Purpose and boundaries
Provides cross-OS desktop launch helpers, policy linters, PR area classification, and 90% Node/Rust test coverage runners under `scripts/`.

- **In scope**: Cross-OS `cargo build` + native desktop launcher (`launch-desktop.mjs`, `lib/desktop-launch.mjs`), PR diff area routing (`ci-changes.mjs`), source LOC and anti-compression enforcement (`source-policy.mjs`, `loc-lint.mjs`), static safety checks (`lint.mjs`), and `c8` / `cargo llvm-cov` coverage gates (`test-coverage-node.mjs`, `test-coverage-rust.mjs`).
- **Out of scope**: GitHub Actions workflow definitions (`.github/workflows/quality-gate.yml`), Prettier/Knip config files, and Rust crate test suites.

## Observable behavior
- **Cross-OS desktop launcher (`launch-desktop.mjs`, `lib/desktop-launch.mjs`)**: Resolves `target/debug/opendrive.exe` on `win32` or `target/debug/opendrive` on POSIX (`resolveDesktopBinary`), builds `opendrive-app` (`cargo build -p opendrive-app`), spawns the native binary with `['ui']`, and cleanly terminates the child process on `SIGINT`/`SIGTERM` (`taskkill /T /F` on `win32`, `SIGTERM` on POSIX).
- **PR change classifier (`ci-changes.mjs`)**: Normalizes paths and inspects `git diff --name-only <BASE_SHA>...HEAD` on `pull_request` events (`GITHUB_OUTPUT`). Docs trigger neither; `crates/**` and Rust root manifests trigger `rust`; `scripts/**` and Node root configs (`package.json`, `package-lock.json`, `knip.json`) trigger `node`; workflows/unknown root files trigger both. Non-PR events default to `{ node: true, rust: true }`.
- **Source LOC & safety lint (`source-policy.mjs`, `loc-lint.mjs`, `lint.mjs`)**:
  - `loc-lint.mjs` scans `git ls-files -z`, applying `LIMITS` (300 lines for `.rs`; 350 lines for `.ts`/`.mjs`; 400 lines for `.css`/`.html`) while excluding generated directories (`dist`, `target`, `node_modules`, etc.), data/binary/doc extensions, and generated suffixes. Rejects lines `> 500` chars with `>= 20` `;`/`{`/`}` tokens (`looksArtificiallyCompressed`).
  - `lint.mjs` walks `src`, `apps`, `packages`, and `scripts` (ignoring build/output dirs) and rejects `eval(` and `child_process...exec(`.
- **Coverage gates (`test-coverage-node.mjs`, `test-coverage-rust.mjs`)**:
  - Node gate runs `c8` with `--check-coverage` at `90%` minimum for lines, statements, functions, and branches across `scripts/test/*.test.mjs`, excluding `scripts/test/**`, `scripts/launch-desktop.mjs`, and `scripts/test-coverage-*.mjs`.
  - Rust gate runs `cargo llvm-cov --workspace --locked` at `90%` minimum for lines, functions, and regions, ignoring `main.rs`, `types.rs`, `keyring.rs`, `fuse_sys.rs`, `cfapi_sys.rs`, `desktop_window.rs`, `desktop_tray.rs`, and `tray_loop.rs`.

## Code map
- `scripts/launch-desktop.mjs` — Cross-OS VS Code `F5` / `npm run desktop` launcher building and starting the native `opendrive` binary.
- `scripts/lib/desktop-launch.mjs` — Pure binary-path and argv helpers (`resolveDesktopBinary`, `buildCargoBuildArgs`, `buildDesktopArgs`).
- `scripts/ci-changes.mjs` — `normalizePath`, `classifyChanges`, `getPrChangedFiles`, and GitHub Actions output writer.
- `scripts/lib/source-policy.mjs` — `LIMITS`, `EXCLUDED_SEGMENTS`, `EXCLUDED_EXTENSIONS`, `sourceLimit`, `countPhysicalLines`, `looksArtificiallyCompressed`.
- `scripts/lint.mjs` — Static scanner banning `eval(` and `child_process` `exec(`.
- `scripts/loc-lint.mjs` — Tracked-file runner enforcing `sourceLimit` and `looksArtificiallyCompressed`.
- `scripts/test-coverage-node.mjs` — `collectTestFiles` and `runNodeCoverage` invoking `c8` at 90% thresholds.
- `scripts/test-coverage-rust.mjs` — `runRustCoverage` invoking `cargo llvm-cov` at 90% thresholds with platform/window-loop exclusions.
- `scripts/test/desktop-launch.test.mjs` — 2 tests covering cross-OS executable path resolution and cargo/app CLI argument builders.
- `scripts/test/ci-changes.test.mjs` — 4 tests covering path normalization, change routing, and temporary-repo CLI execution.
- `scripts/test/loc-lint.test.mjs` — 3 tests covering limit failures, ignored paths/extensions, compressed lines, and non-git error exit.
- `scripts/test/quality-gate.test.mjs` — 3 policy tests validating `package.json`, `.github/workflows/quality-gate.yml`, `rust-toolchain.toml`, `.gitignore`, `.prettierignore`, `.vscode/launch.json`, and `knip.json`.
- `scripts/test/source-policy.test.mjs` — 4 tests verifying language limits, exclusions, physical line counting, and compression detection.

## Verification
Verified in Node test and coverage runs:
- `scripts/test/*.test.mjs`: 16 Node tests across 5 files executed via `npm run test:scripts` and `npm run test:coverage:node`.
- Note: `scripts/launch-desktop.mjs` and `scripts/test-coverage-*.mjs` are excluded from `c8` coverage enforcement; `scripts/test/quality-gate.test.mjs` statically asserts threshold flags, launch config, and workflow alignment.

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Native Control Center Desktop UI](../features/control-center-ui.md)
- [CLI and synchronous control bridge](../interfaces/cli-and-http-bridge.md)
