# Bento UI, Shared Folders Fix, Account Avatars, Custom Native Bar & System Tray Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task cleanly in the current local branch. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix shared/placeholder Explorer folders (resolve shortcuts + `"Shared with me"` + CFAPI placeholder reconciliation), bind Google profile avatars to accounts, design a new layered OpenDrive logo, restyle the desktop UI as an Image-2 colorful Bento Grid with Settings in a modal, add a custom draggable native titlebar with Pause/Resume + total multi-account storage, and integrate an always-active OS System Tray (hide-on-close).

**Architecture:** The work splits cleanly across four layers: (1) Drive v3 client + FakeDrive metadata extensions (`shortcutDetails`, `sharedWithMe`, `user.photoLink`, `download_bytes`), (2) `SyncEngine` & Windows CFAPI placeholder recovery (propagating offline/hydration errors instead of sealing empty folders, creating synthetic `"Shared with me"` folder nodes, resolving folder shortcuts, and repairing pre-existing `Placeholder` directories on mount), (3) `Account::avatar_url` + `<data_dir>/avatars` caching + `formatters::summarize_total_storage`, and (4) `opendrive-app` pure-Rust UI & native desktop shell (`logo.rs`, Image-2 `ThemeMode::ColorPop`/`DarkStudio` tokens, `desktop_titlebar.rs`, `desktop_bento.rs`, `desktop_modal.rs`, avatar rendering in `desktop_ledger.rs`, and `desktop_tray.rs` wired into `desktop_window.rs`).

**Tech Stack:** Rust workspace (`opendrive-drive`, `opendrive-fake-drive`, `opendrive-core`, `opendrive-sync`, `opendrive-fs-windows`, `opendrive-app`), `eframe` 0.37 (`glow`), `image` 0.25 (`png`, `jpeg`), `cloud-filter` 0.0.7 (`UpdateOptions::has_children`), `tray-icon` 0.19 (Windows/macOS default + Linux `tray` feature).

**Spec:** [docs/superpowers/specs/2026-11-18-bento-ui-shared-tray-design.md](../specs/2026-11-18-bento-ui-shared-tray-design.md)

## Global Constraints

- Every `.rs` source and test file MUST remain `<= 300` physical lines (`npm run lint:loc`) and pass `cargo fmt --all -- --check`.
- `npm run test:gate` MUST exit `0`: Prettier clean, `lint.mjs`, `loc-lint.mjs`, `knip`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (`+ --features tray`), Node coverage `>= 90%`, and Rust `cargo llvm-cov --workspace --locked` `>= 90%` lines/functions/regions.
- Linux CI (`ubuntu-latest`) has no GTK/AppIndicator headers: keep `tray-icon` target-gated on `windows`/`macos` plus the optional `tray` feature.
- Backwards-compatible serde defaults (`#[serde(default)]`) on all new `Account` fields so existing `accounts.json` files deserialize cleanly.

## Review Focus

- **Shortcut to a folder without target MIME or pointing to a trashed target:** Treat as non-hydratable shortcut/file rather than panicking or creating infinite hydration loops; pin a test in `opendrive-sync` (`lazy_hydration_tests.rs`).
- **Pre-existing empty NTFS directory placeholder already marked empty before upgrade:** `MountManager::mount_one` calls `reconcile_placeholder_dirs` to re-enable `CF_UPDATE_FLAG_ENABLE_ON_DEMAND_POPULATION` for `NodeState::Placeholder` folders with 0 local children; handle missing/non-placeholder directories gracefully as `Ok(())`.
- **Corrupt, HTML-error, or slow avatar download / missing image:** Avatar fetch failures in `complete.rs` / `sync_ops.rs` must never fail mounting or OAuth; invalid image bytes in `<data_dir>/avatars` must fall back cleanly to the monogram badge (`avatar_cache_and_fallback_work` test).
- **Zero-quota / unlimited-quota accounts in total storage bar:** `summarize_total_storage` sums `storage_used` and `storage_quota` across accounts cleanly without division-by-zero or misleading percentages when `total_quota == 0`.
- **Closing main window when System Tray is unavailable or headless:** On platforms/tests without an active tray handle (`quit_requested` or headless `ControlCenterApp`), never trap the window unclosable; guard `CancelClose` + `Visible(false)` strictly by `self.tray_active && !self.quit_requested`.

---

### Task 1: Drive v3 API & FakeDrive Support for Shortcuts, `sharedWithMe`, and `photoLink`

**Files:**

- Modify: `crates/opendrive-drive/src/types.rs`
- Modify: `crates/opendrive-drive/src/client.rs`
- Modify: `crates/opendrive-fake-drive/src/types.rs`, `crates/opendrive-fake-drive/src/lib.rs`, `crates/opendrive-fake-drive/src/server.rs`
- Test: `crates/opendrive-drive/tests/drive_http_tests.rs`

**Interfaces:**

- Produces:
  - `About::photo_url: Option<String>`
  - `DriveFile::shortcut_target_id: Option<String>`
  - `DriveFile::shortcut_target_mime: Option<String>`
  - `DriveClient::list_shared_with_me(&self, page_token: Option<&str>) -> Result<FileList>`
  - `DriveClient::download_bytes(&self, url: &str) -> Result<Vec<u8>>`
  - `FakeAbout::photo_url: Option<String>`, `FakeFile::shared_with_me: bool`, `FakeFile::shortcut_target_id: Option<String>`, `FakeFile::shortcut_target_mime: Option<String>`, `FakeDrive::set_static_blob(&self, path: &str, bytes: Vec<u8>)`

- [ ] **Step 1: Write failing tests in `drive_http_tests.rs`**
  - Add `shared_with_me_shortcuts_and_avatar_photo_fetch` exercising `about().photo_url`, `download_bytes(&url)`, `list_shared_with_me(None)` returning only `sharedWithMe` entries, and `list_files("root", ...)` returning shortcut entries with populated `shortcut_target_id` / `shortcut_target_mime`.
- [ ] **Step 2: Implement Drive types, `FILE_FIELDS`, `about_once`, `list_shared_with_me`, `download_bytes`, and FakeDrive endpoints**
  - Request `fields = "user(emailAddress,displayName,photoLink),storageQuota"` in `about_once`.
  - Add `shortcutDetails(targetId,targetMimeType)` to `FILE_FIELDS` and map onto `DriveFile`.
  - Extract shared query helper between `list_files_once` and `list_shared_with_me`.
  - Extend `FakeDriveServer` `/drive/v3/files` to branch on `q.contains("sharedWithMe = true")` and serve static bytes (for local avatar fetch tests) via `/assets/...`.
- [ ] **Step 3: Verify `cargo test -p opendrive-drive -p opendrive-fake-drive --locked` passes cleanly.**

---

### Task 2: Fix Shared Folders, Shortcuts & Empty CFAPI Placeholder Lock-In in `SyncEngine` & `opendrive-fs-windows`

**Files:**

- Modify: `crates/opendrive-sync/src/engine.rs`
- Modify: `crates/opendrive-sync/src/vfs_impl.rs`
- Modify: `crates/opendrive-sync/tests/lazy_hydration_tests.rs`
- Modify: `crates/opendrive-fs-windows/src/cfapi_sys.rs`, `crates/opendrive-fs-windows/src/lib.rs`
- Modify: `crates/opendrive-app/src/mount_manager/live.rs`, `crates/opendrive-app/src/mount_manager/sync_ops.rs`

**Interfaces:**

- Consumes: `DriveClient::list_shared_with_me`, `DriveFile::{shortcut_target_id, shortcut_target_mime}`
- Produces:
  - `opendrive_sync::SHARED_WITH_ME_DRIVE_ID: &str = "sharedWithMe"`
  - `SyncEngine::list` returning `Err` when unhydrated folder lacks a `DriveClient` or `hydrate_folder` fails
  - `opendrive_fs_windows::CloudFilesFrontend::reconcile_placeholders(&Path, Vec<String>)` (calling `cfapi_sys::reconcile_placeholder_dirs` on `#[cfg(windows)]`)

- [ ] **Step 1: Write failing tests in `lazy_hydration_tests.rs`**
  - Verify `list` on an unhydrated `Placeholder` folder without a `DriveClient` returns `Err` (never empty `Ok(DirPage)`).
  - Verify hydrating `root` creates a `"Shared with me"` folder (`drive_id == "sharedWithMe"`), and `list(shared_node.id, None)` populates `sharedWithMe` folders/files.
  - Verify folder shortcut (`application/vnd.google-apps.shortcut` -> `targetId = "shared-folder-1"`, `targetMimeType = "application/vnd.google-apps.folder"`) hydrates `"shared-folder-1"` children when opened.
  - Verify broken shortcut without target metadata or missing target lists gracefully without panicking.
- [ ] **Step 2: Implement `SHARED_WITH_ME_DRIVE_ID`, shortcut resolution, and `SyncEngine::list` error propagation**
  - Ensure `hydrate_folder` fetches from `list_shared_with_me` when `drive_parent == SHARED_WITH_ME_DRIVE_ID`, ensures `"Shared with me"` child under `root`, and maps shortcut target ID + folder MIME onto stored `Node`s.
  - Ensure `SyncEngine::list` checks `self.drive_client_for(folder.account_id)?` and propagates `self.hydrate_folder(folder.account_id, node).await?` without masking errors.
- [ ] **Step 3: Implement `cfapi_sys` error reporting & pre-existing NTFS placeholder recovery**
  - Update `CfSyncFilter::fetch_placeholders` to call `ticket.fail(CloudErrorKind::NetworkUnavailable)` when `bridge.fetch_placeholders` errs.
  - Add `cfapi_sys::reconcile_placeholder_dirs(acct_root, rel_placeholder_dirs)` using `OpenOptions` + `Placeholder::update(UpdateOptions::default().has_children().mark_in_sync())`, wired into `LiveMountBackend::connect_account` / `MountManager::mount_one` for SQLite `Placeholder` folders under `<acct_root>`.
- [ ] **Step 4: Run `cargo test -p opendrive-sync -p opendrive-fs-windows --locked` to verify.**

---

### Task 3: Account Avatar Model, Storage Aggregation, Caching & Logo Renderer

**Files:**

- Modify: `crates/opendrive-core/src/lib.rs`
- Modify: `crates/opendrive-app/Cargo.toml` (+ `Cargo.lock` if adding `image` `jpeg` feature), `crates/opendrive-app/src/lib.rs`, `crates/opendrive-app/src/bridge.rs`, `crates/opendrive-app/src/ui.rs`
- Create: `crates/opendrive-app/src/avatars.rs`
- Create: `crates/opendrive-app/src/logo.rs`
- Modify: `crates/opendrive-app/src/auth_runner/complete.rs`, `crates/opendrive-app/src/mount_manager/sync_ops.rs`, `crates/opendrive-app/src/formatters.rs`
- Test: `crates/opendrive-app/tests/formatters_tests.rs`, `crates/opendrive-core/tests/core_tests.rs`

**Interfaces:**

- Produces:
  - `Account::avatar_url: Option<String>` (`#[serde(default)]`)
  - `Bridge::with_data_dir(mut self, dir: PathBuf) -> Self` and `Bridge::data_dir(&self) -> Option<&Path>`
  - `avatars::{avatar_file_path, cache_account_avatar, load_cached_avatar_bytes}`
  - `logo::{paint_logo, render_logo_rgba}`
  - `formatters::{summarize_total_storage, TotalStorageSummary}`

- [ ] **Step 1: Add `Account::avatar_url` + serde backwards-compatibility test in `core_tests.rs`**
- [ ] **Step 2: Enable `image` (`png`, `jpeg`) & target-gated `tray-icon` dependencies in `opendrive-app/Cargo.toml`**
  - Verify `cargo check --workspace --all-targets` compiles cleanly and `Cargo.lock` stays consistent with `--locked`.
- [ ] **Step 3: Implement `avatars.rs`, `logo.rs`, and `summarize_total_storage` in `formatters.rs`**
  - `render_logo_rgba(size: u32) -> Vec<u8>` produces exact `size * size * 4` RGBA pixel buffer depicting the layered Hot Pink / Cyan / Dark-Ink Open-Portal OpenDrive mark (`paint_logo` paints the matching vector geometry).
  - `cache_account_avatar` downloads `photo_url` best-effort into `<data_dir>/avatars/<acct_id>.img` during auth completion (`complete.rs`) and `mount_one` (`sync_ops.rs`).
- [ ] **Step 4: Add unit tests for `summarize_total_storage`, `render_logo_rgba`, `cache_account_avatar`, and `avatar_file_path` fallback.**

---

### Task 4: Image-2 Colorful Bento Palette, Widgets, Custom Draggable Titlebar, Bento Grid & Settings Modal

**Files:**

- Modify: `crates/opendrive-app/src/theme.rs`, `crates/opendrive-app/src/icons.rs`, `crates/opendrive-app/src/widgets.rs`
- Create: `crates/opendrive-app/src/desktop_titlebar.rs`
- Create: `crates/opendrive-app/src/desktop_bento.rs`
- Create: `crates/opendrive-app/src/desktop_modal.rs`
- Modify: `crates/opendrive-app/src/desktop_ledger.rs`, `crates/opendrive-app/src/desktop_drawer.rs`
- Modify: `crates/opendrive-app/src/desktop.rs`, `crates/opendrive-app/src/lib.rs` (remove/re-home `desktop_panels.rs` / `desktop_rail.rs` cleanly or keep focused under 300 LOC)
- Test: `crates/opendrive-app/tests/desktop_ui_tests.rs`, `crates/opendrive-app/tests/desktop_render_tests.rs`

**Interfaces:**

- Produces:
  - `ThemeMode::{ColorPop, DarkStudio}` (`#[default] ColorPop`) with Image-2 palette tokens (`backdrop_indigo`, `pop_pink`, `pop_cyan`, etc. on `ThemePalette`)
  - `IconKind::{Settings, Minimize, Maximize, Restore}` alongside existing icons
  - `widgets::bento_card(ui, pal, accent_bg, title, subtitle, right_pill, add_body)`
  - `UiAction::{ToggleSettings(Option<bool>), WindowMinimize, WindowToggleMaximize, WindowRequestClose}`
  - `ControlCenterApp::{settings_open, quit_requested, avatar_textures, avatar_attempted}`

- [ ] **Step 1: Update `theme.rs`, `icons.rs`, `widgets.rs`**
  - Implement `ThemeMode::ColorPop` (Image 2: Indigo canvas `#645DF6`, Pink `#FF3D7F`, Cyan `#00D4FF`, White `#F8FAFC`, Dark Ink `#1C1E24`) and `ThemeMode::DarkStudio` counterpart, plus `bento_card` offset layered frame.
  - Add `Settings`, `Minimize`, `Maximize`, `Restore` crisp 1.5px stroke icons.
- [ ] **Step 2: Build `desktop_titlebar.rs`, `desktop_bento.rs`, `desktop_modal.rs`, and update `desktop_ledger.rs` with Avatar textures**
  - `desktop_titlebar.rs`: custom draggable bar with `paint_logo`, `"OpenDrive"`, global health pill, **Total Used / Total Quota** badge (`summarize_total_storage`), **Pause All / Resume All** buttons, **Open Folder**, **Settings** modal trigger, **Theme** toggle (`Dark` / `Color`), **Connect Account**, and **Minimize / Maximize / Close** buttons.
  - `desktop_bento.rs`: 2-column hero Bento Row (Multi-Account Storage & Mount Hub + Sync Pulse telemetry tiles) above the Connected Accounts Bento Card (`desktop_ledger.rs`), adapting cleanly to narrow widths (`<= 760px`).
  - `desktop_modal.rs`: centered Settings Modal dialog hosting `Autostart on Login`, `Mount Virtual Drive (O:\)`, `Export Google Docs as Office Files`, `Bandwidth Limit` slider, `Local Cache Limit` slider, and `Purge Unpinned Cache`.
  - `desktop_ledger.rs`: render circular avatar texture loaded from `avatar_cache` (falling back to colorful monogram badge).
- [ ] **Step 3: Wire actions + modal/window commands in `desktop.rs` and update `desktop_ui_tests.rs` & `desktop_render_tests.rs`**
  - Verify custom bar actions, modal open/close/Escape, avatar fallback + PNG texture rendering, contrast checks, and all icons in `ColorPop` & `DarkStudio` modes.

---

### Task 5: Integrated System Tray, Custom Frameless Native Window & Full Repository Quality Gate

**Files:**

- Create: `crates/opendrive-app/src/desktop_tray.rs`
- Modify: `crates/opendrive-app/src/desktop_window.rs`, `crates/opendrive-platform/src/tray.rs`, `crates/opendrive-platform/src/lib.rs`
- Modify: `scripts/test-coverage-rust.mjs` (keep `desktop_tray.rs` excluded alongside `desktop_window.rs` & `tray_loop.rs` if OS GUI tray handles are uncreatable in headless CI)
- Modify docs & specs (`PRODUCT.md`, `DESIGN.md`, `report.md`, `docs/specs/**`, `.sync-state.json`)

**Interfaces:**

- Produces:
  - `desktop_tray::DesktopTray` + pure testable `route_tray_action` / `TrayUiEvent` mapping
  - `run_native_window` using `.with_decorations(false)`, `.with_icon(...)`, and initializing `DesktopTray` on desktop

- [ ] **Step 1: Add pure tray event routing (`TrayUiEvent` + `TrayAction::ShowWindow`) in `opendrive-platform/src/tray.rs` and `desktop_tray.rs`**
  - Implement `DesktopTray` (`#[cfg(any(feature = "tray", windows, target_os = "macos"))]` with clean no-op stub on headless Linux) using `render_logo_rgba(32)` and tray click + menu callbacks waking `egui::Context`.
- [ ] **Step 2: Wire `desktop_window.rs` & `desktop.rs` Close -> Hide-to-Tray lifecycle**
  - Close button (`WindowRequestClose`) or OS `CloseRequested` hides viewport (`Visible(false)`) when tray is active and `!quit_requested`; Tray **Show** restores `Visible(true)` + `Focus`; Tray **Quit** sets `quit_requested = true` and emits `ViewportCommand::Close`.
- [ ] **Step 3: Add unit tests for tray routing + hide-on-close state machine (`desktop_ui_tests.rs` & `opendrive-platform` tests)**
- [ ] **Step 4: Sync specs, `PRODUCT.md`, `DESIGN.md`, `report.md`, `.sync-state.json` and run full verification gate**
  - Format Prettier + `cargo fmt --all`
  - `npm run lint:loc` (all `.rs <= 300`, `.mjs <= 350`)
  - `python C:/Users/long.trinh/.gemini/config/skills/sync-docs-specs/scripts/validate_specs.py --repo .`
  - `cargo clippy --workspace --all-targets --features tray --locked -- -D warnings`
  - `cargo test --workspace --locked`
  - `npm run test:gate` (`>= 90%` Node + Rust coverage)
