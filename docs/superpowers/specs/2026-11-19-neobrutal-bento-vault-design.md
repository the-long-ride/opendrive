# Design Spec: Neo-Brutalist Bento UI, Resizable Chrome, Alias Editing & Encrypted Credential Vault

**Date:** 2026-11-19
**Status:** Draft — Pending User Review

## 1. Goals & Approved Decisions

1. **Text Selection & Highlight Visibility (`theme.rs`, `desktop_titlebar.rs`)**
   - Fix invisible selected text caused by `visuals.selection.bg_fill` and `visuals.selection.stroke.color` sharing `pal.accent_blue`. Set selected foreground `selection.stroke` to high-contrast `#FFFFFF` against indigo selection fill across both themes.
   - Make `"OpenDrive"` in the custom native titlebar selectable (`Label::new(...).selectable(true)`) while restricting window drag (`ViewportCommand::StartDrag`) and double-click maximize to non-widget titlebar regions.
2. **Frameless Resizable Window & Wise Minimum Dimensions (`desktop_window.rs`, `desktop_resize.rs`)**
   - Configure `.with_resizable(true)`, default inner size `[960.0, 720.0]`, and wise minimum `[540.0, 480.0]`.
   - Implement an 8-direction perimeter hit-zone (`6px` inset around `ctx.content_rect()`) so the frameless (`with_decorations(false)`) window can be resized from any edge or corner via `ViewportCommand::BeginResize(...)` and OS resize cursors.
3. **Native Titlebar Cleanup & Zero-Overlap Responsive Main Window (`A1`)**
   - Remove circled items 1–5 from the custom native titlebar (`Sync Active` pill, total storage badge, `Settings` button, `+ Connect` button, `Open` button).
   - Native titlebar keeps ONLY: Logo + selectable `"OpenDrive"` on the left, and `Pause` / `Resume` sync buttons, `Color` / `Dark` theme toggle, and `Minimize` / `Maximize` / `Close` window buttons on the right.
   - Replace overlapping inline `horizontal_wrapped` + `right_to_left` strips across titlebar, Bento hero cards, and account cards with explicit width breakpoints that stack cleanly when narrow or zoomed in (`<= 580px`).
   - Move `+ Connect` / `Close` into the `Connected Accounts` card header alongside plain top-right account-count text (removing the green badge on `"Connected Accounts"`).
4. **Design System Consolidation: `new-design.md` + Image 3 Dual Shadow -> `DESIGN.md` (`A4`)**
   - Merge `new-design.md` **Dark Neo-Brutalism** tokens + tactile press shift (`+2px, +2px`) + Image 3 dual Pink/Cyan hard offset shadows (`(-2.5, -2.5)` Pink `#FF5E93` / `(+2.5, +2.5)` Cyan `#22D3EE`, `0px` blur, `1.5px` `#090A0F` stroke) into `DESIGN.md`, and remove duplicate `new-design.md`.
   - Make `ThemeMode::DarkStudio` (**Dark Neo-Brutalism**: `#111217` canvas, `#252834` dots spaced `24px`, `#181A20` primary cards, `#1E2029` subcards, `#232631` neutral pills, `#FF5E93` Pink, `#22D3EE` Cyan, `#7C74FF` Indigo, `#F4F5F8` White pill) the default theme, while keeping `ThemeMode::ColorPop` as the Light Neo-Brutalist toggle sharing the same hard shadows, strokes, press offsets, and bordered semantic status pills (`#133629`/`#34D399`/`#1D523E`, etc.).
5. **Account Alias Editing & Optional Folder Rename (`A3`)**
   - Default alias cleanly resolves to the local part before `@` in `email` when `nickname` is empty.
   - Each account card provides an inline **Edit** trigger opening an alias editor with optional checkbox **"Also rename local mount folder"**, **Save**, and **Cancel**.
   - Persists updates to `accounts.json` via `AppCommand::RenameAccount { id, nickname, folder_name: Option<String> }` (`Bridge::rename_account`); when `folder_name` changes, `MountManager::sync_now` unmounts the stale CFAPI root and remounts under the updated folder name.
6. **Account Card Relayout & Bottom Thin Pastel Usage Line**
   - Relayout account cards into three clean vertical tiers that never collide:
     1. Top row: Avatar + Editable Alias + Email + Neo-Brutalist Status Pill (left) and Action Cluster (`Sign In`/`Copy Link` when `NeedsReauth`, active-sync switch, `Read-Only` pill, `Edit` alias trigger, `Remove`) wrapping cleanly below on narrow widths.
     2. Middle metadata strip: Folder icon + monospace mount path + `used / quota (pct%)` readout.
     3. Bottom edge: Full-width thin (`5px`) pastel usage progress line colored by storage ratio:
        - `< 75%`: pastel green (`#6EE7B7`)
        - `75% .. < 90%`: pastel orange (`#FDBA74`)
        - `>= 90%`: pastel red (`#FCA5A5`)
7. **Password-Encrypted Credential Export & Import (`A2`)**
   - Add `vault.rs` in `opendrive-auth` using **PBKDF2-HMAC-SHA256** (`600,000` iterations, `16`-byte CSPRNG salt) + **AES-256-GCM** (`12`-byte CSPRNG nonce, AAD `"opendrive-vault-v1"`) via `ring`.
   - Enforce password policy: strictly `> 8` characters (`>= 9`) with at least 1 uppercase letter, 1 lowercase letter, 1 digit, and 1 symbol.
   - Encrypted `.odcred` JSON envelope captures all OAuth `CredentialSet`s (+ `client_secret`s) and `Account` records (+ `refresh_token` / `service_account_json`), restoring them into local `credentials.json`, `accounts.json`, and `SecretStore` (`OsKeyringStore`) on import and triggering `spawn_mount_sync`.
   - Expose native Save/Open file dialogs (`rfd` with pure-Rust `xdg-portal` on Linux, abstracted behind injectable `AuthEndpoints` pickers + optional path fallback) inside a dedicated **Encrypted Credential Backup** section in the **Settings Modal**, plus a **Load Backup (.odcred)** shortcut button inside the **Connect Drawer**.

---

## 2. Crate & File Breakdown

### `crates/opendrive-core`

- `src/lib.rs` & `src/controller.rs`:
  - Add `default_account_alias(nickname: &str, email: &str) -> String`.
  - Add `AppCommand::RenameAccount { id: Uuid, nickname: String, folder_name: Option<String> }` updating `acct.nickname` (falling back to email prefix when blank) and optionally `acct.folder_name` (validated non-empty, sanitized filename segment), returning `AppEvent::AccountUpdated`.
- `tests/core_tests.rs`: unit/integration assertions for `RenameAccount` and default email-prefix alias derivation.

### `crates/opendrive-auth`

- `src/vault.rs`:
  - `validate_vault_password(password: &str) -> Result<(), &'static str>` enforcing `chars().count() > 8`, uppercase, lowercase, ASCII digit, and symbol.
  - `VaultPayload`, `ExportedCredentialSet`, `ExportedAccount`, `EncryptedVaultFile`.
  - `encrypt_vault(payload: &VaultPayload, password: &str) -> Result<Vec<u8>, AuthError>` and `decrypt_vault(bytes: &[u8], password: &str) -> Result<VaultPayload, AuthError>`.
- `src/lib.rs`: export `vault` types & helpers.
- `tests/auth_tests.rs`: password policy edge cases, encrypt/decrypt round-trip, wrong-password rejection, tampered-ciphertext rejection, deterministic version check.

### `crates/opendrive-app`

- `Cargo.toml`: add `rfd = { version = "0.15", default-features = false, features = ["xdg-portal"] }` cleanly for native file dialogs without GTK headers on Linux CI.
- `src/theme.rs`:
  - Set `ThemeMode::DarkStudio` default (`bg_canvas #111217`, `bg_dot_matrix #252834`, `bg_card_primary #181A20`, `bg_card_secondary #1E2029`, `bg_pill_neutral #232631`, `accent_pink #FF5E93`, `accent_cyan #22D3EE`, `accent_indigo #7C74FF`, `accent_white #F4F5F8`, `border_subtle #2E3241`, `border_strong #090A0F`, semantic status badges `#133629`/`#34D399`/`#1D523E`, `#332714`/`#FBBF24`/`#5E4720`, `#2A161D`/`#F87171`/`#5C2432`).
  - Fix `visuals.selection.stroke = Stroke::new(1.5, Color32::WHITE)` against `pal.accent_blue`.
  - Update `tone_colors` to return `(bg, fg, border)` so status badges render with crisp `1.5px` borders (`greenbadge` matches theme style).
- `src/widgets.rs` & `src/widgets_controls.rs`:
  - Update `bento_card` / `dual_neon_card` with Image 3 hard offset shadow (`(-2.5, -2.5)` Pink & `(+2.5, +2.5)` Cyan ridges + `1.5px` `BORDER_DARK` border, plus single-color `3.5px` hard offset option).
  - Add tactile press shift (`+2.0, +2.0`) and `0px`-blur hard shadow on `pill_button` (`Primary` & `TactilePill` & bordered `Default`/`Danger`/`Toggle`).
  - Update `status_pill` to use Neo-Brutalist `1.5px` border + square indicator dot + theme tokens.
  - Add `thin_usage_bar(ui, pal, ratio)` drawing a `5px` bottom bar colored Pastel Green (`< 75%`), Pastel Orange (`75%..90%`), or Pastel Red (`>= 90%`).
- `src/icons.rs`: add `IconKind::Edit` (`Pencil` geometric stroke).
- `src/desktop_resize.rs` (new small module): 8-edge/corner frameless window resize handles (`ViewportCommand::BeginResize`).
- `src/desktop_titlebar.rs`: stripped to Logo + selectable `"OpenDrive"`, `Pause` / `Resume`, Theme toggle, and `Minimize` / `Maximize` / `Close`; empty space drag does not steal text selection.
- `src/desktop_bento.rs`: `paint_dot_matrix` (`24px` spacing, `1.5px` radius dots per `new-design.md`), `Storage & Workspace` (`Dual` Image-3 shadow), `Sync Pulse` (now displaying global sync health badge alongside Docs badge + 3 subcards), responsive 2-column -> 1-column breakpoint.
- `src/desktop_ledger.rs`: top-right shows plain `"N accounts"` text + `+ Connect` / `Close` button; redesigned overlap-free account cards with inline alias editor (`Also rename local mount folder` checkbox) and `thin_usage_bar` along card bottom.
- `src/desktop_drawer.rs` / `sections.rs`: add `Load Backup (.odcred)` shortcut button that opens/routes to Encrypted Import.
- `src/desktop_modal.rs`: add **Encrypted Credential Backup** section (Password, Confirm Password, validation feedback, **Export (.odcred)** & **Import (.odcred)** buttons using native file dialogs / injectable pickers + path fallback).
- `src/bridge.rs`: add `Bridge::rename_account` and `Bridge::export_encrypted_vault` / `Bridge::import_encrypted_vault`.
- `src/desktop.rs`, `src/desktop/settings_actions.rs`, `src/desktop/auth_handlers.rs`: wire `RenameAccount`, `ExportVault`, `ImportVault`, `OpenVaultDialog`, and `BeginResize`.
- `src/desktop_window.rs`: `.with_resizable(true)`, `with_inner_size([960.0, 720.0])`, `with_min_inner_size([540.0, 480.0])`, start in `ThemeMode::default()` (`DarkStudio`).

### Documentation Consolidation (`A4`)

- Replace `DESIGN.md` with the merged `new-design.md` Dark Neo-Brutalism + Image 3 dual Pink/Cyan shadow + Light Neo-Brutalist toggle + thin pastel usage bar specification, delete `new-design.md`, and sync `PRODUCT.md`, `report.md`, and `docs/specs/**` + `docs/specs/.sync-state.json`.

---

## 3. Verification Criteria

1. Unit & integration tests cover:
   - Default alias extraction (`alice@example.com` -> `alice`), `RenameAccount` persistence, and optional `folder_name` update + remount.
   - `validate_vault_password` (rejects `<= 8` chars or missing uppercase/lowercase/digit/symbol; accepts valid passwords) and `encrypt_vault`/`decrypt_vault` round-trip, wrong password, and corrupted data across `Bridge::export_encrypted_vault` / `import_encrypted_vault` preserving secrets in `SecretStore`.
   - Pastel usage color selection at `< 0.75`, `0.75..0.90`, `>= 0.90`; `selection.stroke.color != selection.bg_fill` contrast check; native bar absence of items 1–5; narrow viewport (`540px` / zoom) layout rendering without panics or widget overlap; alias inline editor state transitions.
2. Full workspace quality gate: `cargo fmt --all -- --check`, `npm run lint:loc` (`<= 300` lines per `.rs`), `cargo clippy --workspace --all-targets --features tray --locked -- -D warnings`, `validate_specs.py --repo .`, `cargo test --workspace --locked`, and `npm run test:gate` (`>= 90%` Rust & Node coverage).
