---
schema_version: 1
id: "architecture.quality-gate"
title: "Quality Gate, Desktop Launcher, and CI Scripts"
kind: "architecture"
status: "verified"
summary: "Node.js scripts enforcing LOC/compression limits, static safety checks, cross-OS desktop binary launching, PR change routing, and 95% coverage gates."
tags: ["ci", "quality-gate", "lint", "loc", "coverage", "desktop-launch"]
source_paths: ["scripts/ci-changes.mjs", "scripts/generate-release-notes.mjs", "scripts/launch-desktop.mjs", "scripts/lib/desktop-launch.mjs", "scripts/lib/source-policy.mjs", "scripts/lint.mjs", "scripts/loc-lint.mjs", "scripts/release-gate.sh", "scripts/test-coverage-node.mjs", "scripts/test-coverage-rust.mjs", "scripts/validate-specs.mjs", "scripts/test/ci-changes.test.mjs", "scripts/test/desktop-launch.test.mjs", "scripts/test/lint.test.mjs", "scripts/test/loc-lint.test.mjs", "scripts/test/quality-gate.test.mjs", "scripts/test/release-gate.test.mjs", "scripts/test/release-notes.test.mjs", "scripts/test/release-packaging.test.mjs", "scripts/test/source-policy.test.mjs", "scripts/test/validate-specs.test.mjs"]
related_specs: ["architecture.workspace-overview", "feature.control-center-ui", "interface.cli-http-bridge"]
last_doc_update_at: "2026-11-27T06:40:00Z"
last_verified_at: "2026-11-27T06:40:00Z"
last_verified_code_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_at: "2026-11-26T22:55:18+07:00"
source_fingerprint: "sha256:f1393157f33553781677fdf5d82b9c5d12d546fea2e1c2f8c5e8eecb66deb92d"
---
# Quality Gate, Desktop Launcher, and CI Scripts

## Purpose and boundaries
Provides cross-OS desktop launch helpers, policy linters, PR area classification, and 95% Node/Rust test coverage runners under `scripts/`.

- **In scope**: Cross-OS `cargo build` + native desktop launcher (`launch-desktop.mjs`, `lib/desktop-launch.mjs`), PR diff area routing (`ci-changes.mjs`), source LOC and anti-compression enforcement (`source-policy.mjs`, `loc-lint.mjs`), static safety checks (`lint.mjs`), `c8` / `cargo llvm-cov` coverage gates (`test-coverage-node.mjs`, `test-coverage-rust.mjs`), and release tag validation (`release-gate.sh`).
- **Out of scope**: GitHub Actions workflow definitions (`.github/workflows/quality-gate.yml`), Prettier/Knip config files, and Rust crate test suites.

## Observable behavior
- **Cross-OS desktop launcher (`launch-desktop.mjs`, `lib/desktop-launch.mjs`)**: Resolves `target/debug/opendrive.exe` on `win32` or `target/debug/opendrive` on POSIX (`resolveDesktopBinary`), builds `opendrive-app` (`cargo build -p opendrive-app`), spawns the native binary with `['ui', ...extraArgs]` (forwarding CLI arguments), and cleanly terminates the child process on `SIGINT`/`SIGTERM` (`taskkill /T /F` on `win32`, `SIGTERM` on POSIX).
- **PR change classifier (`ci-changes.mjs`)**: Normalizes paths and inspects `git diff --name-only <BASE_SHA>...HEAD` on `pull_request` events (`GITHUB_OUTPUT`). Docs, `assets/**`, `.agents/**`, `.md`/`.txt`/`.rst`, and `LICENSE` trigger neither (`paths-ignore` also skips CI and Quality Gate runs on doc/asset-only PRs and pushes); `crates/**` and Rust root manifests trigger `rust`; `scripts/**` and Node root configs (`package.json`, `package-lock.json`, `knip.json`) trigger `node`; workflows/unknown root files trigger both. Non-PR events default to `{ node: true, rust: true }`.
- **Spec validation (`validate-specs.mjs`)**: Cross-platform LF-normalized (`\r\n` -> `\n`) YAML frontmatter, link, `INDEX.md` reachability, and SHA-256 `source_fingerprint` validator (`npm run validate:specs`, wired into `npm run test:gate`, `.githooks/pre-push`, and `static-checks`).
- **Source LOC & safety lint (`source-policy.mjs`, `loc-lint.mjs`, `lint.mjs`)**:
  - `loc-lint.mjs` scans `git ls-files -z --cached --others --exclude-standard` (all tracked and untracked non-gitignored files), applying `LIMITS` strictly with zero exceptions (300 lines for `.rs`; 350 lines for `.ts`/`.mjs`; 400 lines for `.css`/`.html`) while excluding generated directories (`dist`, `target`, `node_modules`, etc.), data/binary/doc extensions, and generated suffixes. Rejects lines `> 500` chars with `>= 20` `;`/`{`/`}` tokens (`looksArtificiallyCompressed`).
  - `lint.mjs` walks `src`, `apps`, `packages`, and `scripts` (ignoring build/output dirs), rejects `eval(` and `child_process...exec(`, runs `oxlint` (`no-unused-vars`, `no-unused-private-class-members`, `no-unused-labels`) across collected JS/TS files, and verifies every Cargo `[workspace.package]` field and `[workspace.dependencies]` entry is referenced by at least one member `crates/*/Cargo.toml`.
- **Coverage gates (`test-coverage-node.mjs`, `test-coverage-rust.mjs`)**:
  - Node gate runs `c8` with `--check-coverage` at `95%` minimum for lines, statements, functions, and branches across `scripts/test/*.test.mjs`, excluding `scripts/test/**`, `scripts/launch-desktop.mjs`, and `scripts/test-coverage-*.mjs`.
  - Rust gate runs `cargo llvm-cov --workspace --locked` at `95%` minimum for lines, functions, and regions, ignoring `main.rs`, `types.rs`, `keyring.rs`, `fuse_sys.rs`, `cfapi_sys.rs`, and `shell/{window,tray,tray_loop}.rs`.
- **Release tag gate & notes (`release-gate.sh`, `generate-release-notes.mjs`)**: `release-gate.sh` supports `ARTIFACT_ONLY=true` (emitting `v{version}-manual-{sha7}`), validates semver tags (accepting annotated `refs/tags/${tag}^{commit}` targets that are ancestors of `refs/remotes/origin/main`), verifies `Cargo.toml` `[workspace.package]` `version`, and writes `tag`, `version`, and `ref_sha` to `GITHUB_OUTPUT`. `generate-release-notes.mjs` renders collapsible `<details>` changelog sections without redundant version headings, direct download links for all OS installers/portables, and a clickable GitHub avatar + `@username` contributor flow layout.

## Code map
- `scripts/launch-desktop.mjs` — Cross-OS VS Code `F5` / `npm run desktop` launcher building and starting the native `opendrive` binary.
- `scripts/lib/desktop-launch.mjs` — Pure binary-path and argv helpers (`resolveDesktopBinary`, `buildCargoBuildArgs`, `buildDesktopArgs`).
- `scripts/ci-changes.mjs` — `normalizePath`, `classifyChanges`, `getPrChangedFiles`, and GitHub Actions output writer.
- `scripts/lib/source-policy.mjs` — `LIMITS`, `EXCLUDED_SEGMENTS`, `EXCLUDED_EXTENSIONS`, `sourceLimit`, `countPhysicalLines`, `looksArtificiallyCompressed`.
- `scripts/lint.mjs` — Static scanner banning `eval(` and `child_process` `exec(`, enforcing unused-symbol checks via `oxlint`, and verifying Cargo workspace manifest fields/dependencies are used by member crates.
- `scripts/loc-lint.mjs` — Zero-exception tracked + untracked file runner enforcing `sourceLimit` and `looksArtificiallyCompressed`.
- `scripts/release-gate.sh` — Release tag validation and gate script.
- `scripts/generate-release-notes.mjs` — GitHub Release body generator (changelog extract, direct asset links, clickable contributor avatar flow layout).
- `scripts/test-coverage-node.mjs` — `collectTestFiles` and `runNodeCoverage` invoking `c8` at 95% thresholds.
- `scripts/test-coverage-rust.mjs` — `runRustCoverage` invoking `cargo llvm-cov` at 95% thresholds with platform/window-loop exclusions.
- `scripts/validate-specs.mjs` — `computeSourceFingerprint` (LF-normalized SHA-256) and `validateSpecs` (`npm run validate:specs`).
- `scripts/test/desktop-launch.test.mjs` — Tests covering cross-OS executable path resolution and cargo/app CLI argument builders.
- `scripts/test/ci-changes.test.mjs` — Tests covering path normalization, change routing, and temporary-repo CLI execution.
- `scripts/test/lint.test.mjs` — Tests covering clean passes, `eval`/`exec` bans, `oxlint` unused-symbol checks, and Cargo workspace manifest unused checks.
- `scripts/test/loc-lint.test.mjs` — Tests covering tracked & untracked limit failures, gitignored/excluded paths & extensions, compressed lines, and non-git error exit.
- `scripts/test/quality-gate.test.mjs` — Policy tests validating zero-exception `loc-lint.mjs`, `package.json`, `.github/workflows/quality-gate.yml`, `rust-toolchain.toml`, `.gitignore`, `.prettierignore`, `.vscode/launch.json`, and `knip.json`.
- `scripts/test/release-gate.test.mjs` — Tests covering `release-gate.sh` tag and dispatch behavior.
- `scripts/test/release-notes.test.mjs` — Tests covering changelog extraction, download link tables, contributor avatars/flow HTML, and CLI execution.
- `scripts/test/release-packaging.test.mjs` — Tests verifying GitHub README logo URL, `the-long-ride` author metadata, generated PNG/ICO icon headers, macOS iconset script rules, and `OD-*` coverage across `error-codes.md` and `llm.txt`.
- `scripts/test/source-policy.test.mjs` — Tests verifying language limits, exclusions, physical line counting, and compression detection.
- `scripts/test/validate-specs.test.mjs` — Tests verifying CRLF/LF fingerprint equivalence, valid spec trees, frontmatter/link errors, and CLI exit codes.

## Verification
Verified in Node test and coverage runs:
- `scripts/test/*.test.mjs`: 40 Node tests across 10 files executed via `npm run test:scripts` and `npm run test:coverage:node`.
- Note: `scripts/launch-desktop.mjs` and `scripts/test-coverage-*.mjs` are excluded from `c8` coverage enforcement; `scripts/test/quality-gate.test.mjs` statically asserts threshold flags, launch config, and workflow alignment.

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Native Control Center Desktop UI](../features/control-center-ui.md)
- [CLI and synchronous control bridge](../interfaces/cli-and-http-bridge.md)
