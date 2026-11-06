# Neo-Brutalist Bento UI, Resizable Chrome, Alias Editing & Encrypted Vault Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task on the current local branch (`main`). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the Dark Neo-Brutalism design consolidation (`new-design.md` + Image 3 shadow -> `DESIGN.md`), fix text selection visibility and `"OpenDrive"` selectability, add frameless 8-direction window resizing with wise min size (`540x480`), remove Image 2 items 1–5 from the native bar, eliminate narrow/zoom overlap, add persistent account alias editing (with email prefix default and optional mount folder rename), redesign account cards with a bottom thin pastel usage bar, and add PBKDF2-HMAC-SHA256 + AES-256-GCM password-encrypted `.odcred` credential export/import.

**Architecture:**

- **`opendrive-core`:** `default_account_alias` helper and `AppCommand::RenameAccount { id, nickname, folder_name }` in `controller.rs`.
- **`opendrive-auth`:** `vault.rs` implementing `validate_vault_password` (`len > 8`, upper/lower/digit/symbol) and `encrypt_vault` / `decrypt_vault` (`ring` PBKDF2-HMAC-SHA256 600,000 iterations + AES-256-GCM).
- **`opendrive-app`:**
  - `theme.rs`: Dark Neo-Brutalism (`DarkStudio`) default + Light Neo-Brutalism (`ColorPop`) toggle, `selection.stroke = Color32::WHITE`, 3-tuple `tone_colors` `(bg, fg, border)`.
  - `widgets.rs` & `widgets_controls.rs`: Image 3 `dual_neon_card` (`(-2.5, -2.5)` Pink / `(+2.5, +2.5)` Cyan), `0px` blur hard offset shadows, `(+2.0, +2.0)` tactile press shift, bordered `status_pill`, and `thin_usage_bar` (`< 75%` pastel green, `75..<90%` pastel orange, `>= 90%` pastel red).
  - `desktop_resize.rs`, `desktop_window.rs`, `desktop_titlebar.rs`: `.with_resizable(true)`, `540x480` minimum size, 8-edge perimeter resize (`BeginResize`), selectable `"OpenDrive"`, and native bar cleaned to Logo + `"OpenDrive"`, `Pause` / `Resume`, Theme toggle, and window chrome.
  - `desktop_bento.rs`, `desktop_ledger.rs`, `desktop_modal.rs`, `desktop_drawer/`: overlap-free responsive rows, `Sync Pulse` health pill, plain account count + `+ Connect` on `Connected Accounts` header, redesigned account card with inline alias editor (`Also rename local mount folder` checkbox) & bottom pastel bar, and **Encrypted Credential Backup** export/import (`rfd` file picker + injectable `AuthEndpoints` hooks) in Settings Modal + Connect Drawer shortcut.

**Tech Stack:** Rust, `eframe`/`egui` 0.37, `ring` 0.17, `rand` 0.8, `base64` 0.22, `rfd` 0.15 (`xdg-portal` on Linux CI), `opendrive-core`, `opendrive-auth`, `opendrive-app`.

**Spec:** [2026-11-19-neobrutal-bento-vault-design.md](file:///d:/my-repos/open-sources/opendrive/docs/superpowers/specs/2026-11-19-neobrutal-bento-vault-design.md)

## Global Constraints

- Physical line cap: `<= 300` lines per `.rs` file, `<= 350` lines per `.mjs` file (`npm run lint:loc`).
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --features tray --locked -- -D warnings`, `validate_specs.py --repo .`, `npm run test:gate` (`>= 90%` Node & Rust coverage).
- Implement directly on local branch `main` (no new worktree or sandbox).

---

### Task 1: Account Alias Support in `opendrive-core` & Encrypted Vault in `opendrive-auth`

- [ ] Add `default_account_alias(nickname: &str, email: &str) -> String` in `crates/opendrive-core/src/lib.rs` and `AppCommand::RenameAccount { id: Uuid, nickname: String, folder_name: Option<String> }` in `crates/opendrive-core/src/controller.rs`.
- [ ] Add `crates/opendrive-core/tests/core_tests.rs` assertions covering `default_account_alias` and `RenameAccount` (alias-only and optional folder rename).
- [ ] Create `crates/opendrive-auth/src/vault.rs` implementing `validate_vault_password`, `VaultPayload`, `ExportedCredentialSet`, `ExportedAccount`, `EncryptedVaultFile`, `encrypt_vault`, `decrypt_vault` (`ring` PBKDF2-HMAC-SHA256 + AES-256-GCM), and export from `crates/opendrive-auth/src/lib.rs`.
- [ ] Add unit/integration tests in `crates/opendrive-auth/tests/auth_tests.rs` verifying password rules (`> 8` chars + upper/lower/digit/symbol), round-trip encrypt/decrypt, wrong password, and tampered JSON rejection.
- [ ] Verify: `cargo test -p opendrive-core -p opendrive-auth --locked`.

### Task 2: Bridge Operations (`rename_account`, `export_encrypted_vault`, `import_encrypted_vault`) & `rfd` Picker Hooks

- [ ] Add `rfd` (`default-features = false, features = ["xdg-portal"]`) to `crates/opendrive-app/Cargo.toml` and `auth_runner.rs` `VaultSavePicker` / `VaultOpenPicker` callbacks on `AuthEndpoints`.
- [ ] Implement `Bridge::rename_account`, `Bridge::export_encrypted_vault`, and `Bridge::import_encrypted_vault` in `crates/opendrive-app/src/bridge.rs` (or a clean helper submodule `bridge_vault.rs` to keep `bridge.rs <= 300` LOC), persisting `accounts.json`, `credentials.json`, and `SecretStore` secrets, and triggering `spawn_mount_sync` on import or folder rename.
- [ ] Use `default_account_alias` cleanly in `auth_runner/complete.rs` / `formatters.rs` when `nickname` is blank.
- [ ] Add tests in `crates/opendrive-app/tests/bridge_ui_tests.rs` covering `rename_account`, `export_encrypted_vault`, and `import_encrypted_vault` (preserving OAuth + Service Account secrets across two separate `Bridge` + `MemorySecretStore` instances).
- [ ] Verify: `cargo test -p opendrive-app --test bridge_ui_tests --locked`.

### Task 3: Dark Neo-Brutalism Theme, Selection Visibility, Image 3 Shadows & Tactile Primitives

- [ ] Update `crates/opendrive-app/src/theme.rs`: `ThemeMode::DarkStudio` default using `new-design.md` tokens (`#111217`, `#252834`, `#181A20`, `#1E2029`, `#232631`, `#FF5E93`, `#22D3EE`, `#7C74FF`, `#F4F5F8`, `#2E3241`, `#090A0F`), harmonized Light Neo-Brutalist `ThemeMode::ColorPop`, `visuals.selection.stroke = Stroke::new(1.5, Color32::WHITE)`, and bordered 3-color `tone_colors` `(bg, fg, border)`.
- [ ] Update `crates/opendrive-app/src/icons.rs` (+ `IconKind::Edit`), `crates/opendrive-app/src/widgets.rs` (bordered `status_pill`, tactile press shift `+2.0, +2.0`, `0px` hard offset shadows, `thin_usage_bar`, `usage_bar_color` returning pastel green `<75%` / orange `75..<90%` / red `>=90%`), and `crates/opendrive-app/src/widgets_controls.rs` (`bento_card` / `dual_neon_card` Image 3 `(-2.5, -2.5)` Pink / `(+2.5, +2.5)` Cyan shadow + `1.5px` dark border).
- [ ] Update `crates/opendrive-app/src/formatters.rs` if needed to expose `usage_pastel_tone` helper cleanly (`<= 300` LOC).

### Task 4: Frameless Resize, Native Titlebar Cleanup, Bento Cards, Account Relayout, Alias Editor & Backup Modal

- [ ] Create `crates/opendrive-app/src/desktop_resize.rs` (`render_resize_handles` 8-edge perimeter `BeginResize`) and update `crates/opendrive-app/src/desktop_window.rs` (`.with_resizable(true)`, `[960.0, 720.0]`, min `[540.0, 480.0]`).
- [ ] Update `crates/opendrive-app/src/desktop_titlebar.rs`: remove circled 1–5 items, keep only `[Logo]` + `.selectable(true)` `"OpenDrive"`, `Pause` / `Resume`, Theme toggle, and `Minimize` / `Maximize` / `Close`, with non-overlapping narrow wrap and background-only drag.
- [ ] Update `crates/opendrive-app/src/desktop_bento.rs`: `24px` dot matrix (`paint_bento_dots`), global sync status badge in `Sync Pulse` header, overlap-free responsive grid.
- [ ] Update `crates/opendrive-app/src/desktop_ledger.rs`: plain `"N account(s)"` + `+ Connect` / `Close` in card header (no green badge), redesigned overlap-free account card layout, inline alias editor + `"Also rename local mount folder"` checkbox, and full-width bottom `thin_usage_bar`.
- [ ] Update `crates/opendrive-app/src/desktop_modal.rs` & `desktop_drawer/sections.rs`: add **Encrypted Credential Backup** section (`Password`, `Confirm Password`, **Export (.odcred)**, **Import (.odcred)**) inside Settings Modal + **Load Backup (.odcred)** shortcut in Connect Drawer.
- [ ] Wire `UiAction::{RenameAccount, ExportVault, ImportVault, OpenVaultImport}` and inline alias state in `crates/opendrive-app/src/desktop.rs` / `desktop/settings_actions.rs`.
- [ ] Update/extend `formatters_tests.rs`, `desktop_ui_tests.rs`, `desktop_render_tests.rs`, and `bento_avatar_tray_tests.rs` (selection stroke contrast, pastel color thresholds, narrow `540px` rendering without overlap, alias edit state, encrypted backup modal UI actions).

### Task 5: Docs Consolidation (`new-design.md` -> `DESIGN.md`), Spec Sync & Full Verification Gate

- [ ] Merge `new-design.md` + Image 3 shadow + Light Neo-Brutalist toggle + thin pastel usage bar + updated layout/modules into `DESIGN.md`; remove `new-design.md`.
- [ ] Update `PRODUCT.md`, `report.md`, `docs/specs/.sync-state.json`, and affected specs (`architecture.workspace-overview`, `feature.oauth-credentials`, `feature.control-center-ui`, `interface.cli-http-bridge`); normalize LF & recompute fingerprints (`validate_specs.py --repo .`).
- [ ] Run full verification: `npm run lint:loc`, `cargo clippy --workspace --all-targets --features tray --locked -- -D warnings`, `cargo test --workspace --target-dir target/verify --locked`, `--self-check`, `npx prettier --check .`, `validate_specs.py --repo .`, and `npm run test:gate`.
