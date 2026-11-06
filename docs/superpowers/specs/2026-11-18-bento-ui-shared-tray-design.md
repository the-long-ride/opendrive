# OpenDrive Bento Redesign, Shared Folders Fix, Account Avatars, Custom Bar & System Tray Design

**Date:** 2026-11-18
**Status:** Proposed for User Review
**Scope:** Multi-crate feature & UI architecture spanning `opendrive-drive`, `opendrive-core`, `opendrive-sync`, `opendrive-fs-windows`, and `opendrive-app`.

---

## 1. Root Cause Analysis: Shared Folders & Empty Explorer Folders

Inspection of `%APPDATA%\opendrive\meta.db`, Windows NTFS reparse points (`0x9000601a`), `crates/opendrive-sync/src/vfs_impl.rs`, `crates/opendrive-fs-windows/src/cfapi_sys.rs`, and `crates/opendrive-drive/src/client.rs` established three gaps:

1. **CFAPI Empty Directory Lock-In (`vfs_impl.rs` + `cfapi_sys.rs`)**
   - In `SyncEngine::list(node, ...)`, when an unhydrated folder (`state = Placeholder`) has zero local children while `DriveClient` is absent (for example during `NeedsReauth` or pre-refresh startup) or when `hydrate_folder` returns `Err`, `vfs_impl.rs` currently swallows the failure and returns `Ok(DirPage { entries: [] })`.
   - `CfSyncFilter::fetch_placeholders` receives `entries = []` and invokes `ticket.pass_with_placeholders(&mut [])`. Under `PopulationType::Full`, Windows CFAPI then marks that NTFS directory as **In-Sync and fully populated with 0 children**, preventing File Explorer from invoking `fetch_placeholders` on subsequent opens even after authentication recovers.
2. **Drive Shortcuts Ignored (`client.rs` + `engine.rs`)**
   - `FILE_FIELDS` omits `shortcutDetails(targetId,targetMimeType)` and `hydrate_folder` maps `application/vnd.google-apps.shortcut` as a `0`-byte `"file"` rather than resolving its target ID/MIME type.
3. **`"Shared with me"` Excluded (`engine.rs` + `client.rs`)**
   - `hydrate_folder` only queries `'{parent}' in parents and trashed = false` starting from `'root'` (`My Drive`). Folders shared via `"Shared with me"` (unless added cleanly to My Drive as shortcuts/folders) never appear in the mounted tree.

### Fix Design

- **`opendrive-drive`:**
  - Expand `FILE_FIELDS` with `shortcutDetails(targetId,targetMimeType)` and deserialize `shortcut_target_id: Option<String>` and `shortcut_target_mime: Option<String>` on `DriveFile`.
  - Add `DriveClient::list_shared_with_me(page_token: Option<&str>) -> Result<FileList>` querying `sharedWithMe = true and trashed = false` with `supportsAllDrives=true&includeItemsFromAllDrives=true`.
- **`opendrive-sync`:**
  - Define `pub const SHARED_WITH_ME_DRIVE_ID: &str = "sharedWithMe";`.
  - During `hydrate_folder(account_id, parent_node)`:
    - When `drive_parent == "root"`, create/retain a synthetic subfolder `"Shared with me"` (`drive_id = SHARED_WITH_ME_DRIVE_ID`, `kind = "folder"`, `state = NodeState::Placeholder`) directly under the account root.
    - When `drive_parent == SHARED_WITH_ME_DRIVE_ID`, call `client.list_shared_with_me(...)`.
    - For shortcuts (`mime_type == "application/vnd.google-apps.shortcut"` with `shortcut_target_id`), persist `drive_id = target_id` (storing target folder MIME as `kind = "folder"`) so opening shortcut folders lazily hydrates the target shared folder's children.
  - In `SyncEngine::list`, if `children.is_empty()` for a `Placeholder` folder (`node > 0`):
    - Return `Err(SyncError::Other("no drive client".into()))` when no `DriveClient` is registered.
    - Propagate `hydrate_folder` errors with `?` rather than returning an empty `DirPage`.
- **`opendrive-fs-windows`:**
  - In `CfSyncFilter::fetch_placeholders`, call `ticket.fail(CloudErrorKind::NetworkUnavailable)` when `bridge.fetch_placeholders` returns `Err`, preventing NTFS from caching an empty directory state.
  - Add `reconcile_placeholder_dirs(root_path, engine, account_id)` invoked on `MountManager::mount_one`: any existing NTFS placeholder subdirectory under `root_path` whose SQLite node is still `NodeState::Placeholder` with zero local children is updated via `Placeholder::update(UpdateOptions::default().has_children().mark_in_sync())` (`CF_UPDATE_FLAG_ENABLE_ON_DEMAND_POPULATION`) so previously stuck folders (including `"Nghe Tiểu Thuyết Mỗi Ngày"`) re-trigger `fetch_placeholders` on next open.

---

## 2. Account Avatar Binding

- **`opendrive-drive` (`types.rs`, `client.rs`):**
  - Update `DriveClient::about_once` `fields` to `"user(emailAddress,displayName,photoLink),storageQuota"` and add `pub photo_url: Option<String>` on `About`.
  - Add `DriveClient::download_bytes(&self, url: &str) -> Result<Vec<u8>>` for fetching `photoLink` images with HTTP status checks.
- **`opendrive-core` (`lib.rs`):**
  - Add `#[serde(default)] pub avatar_url: Option<String>` to `Account` so `accounts.json` stays backwards-compatible with existing files.
- **`opendrive-app` (`avatars.rs`, `complete.rs`, `mount_manager/sync_ops.rs`):**
  - Resolve cached avatar file path under `<data_dir>/avatars/<account_id>.img` (`Bridge` exposes `data_dir: Option<PathBuf>` wired in `ui.rs`).
  - Fetch & write avatar bytes when `about.photo_url` is present during OAuth/Service Account completion and startup `MountManager::mount_one` (best-effort, non-blocking for mount failure if avatar URL is unreachable).
  - `ControlCenterApp` decodes cached avatar bytes once per account via `image::load_from_memory` (PNG/JPEG) into an `egui::TextureHandle` (`load_avatar_texture`) and renders circular-clipped / framed avatars on account cards, falling back cleanly to the monogram badge whenever no image is cached or decoding fails.

---

## 3. New OpenDrive Logo, Image-2 Colorful Bento Grid, Custom Native Bar & Settings Modal

### 3.1 New OpenDrive Logo (`crates/opendrive-app/src/logo.rs`)

- **Concept ("Open Multi-Cloud Drive"):** Three staggered rounded-rectangle drive layers matching Image 2's layered geometry — a **Hot Pink** back layer (`#FF3D7F`), an **Electric Cyan** front-bottom offset layer (`#00D4FF`), and a bold-ink-outlined **White/Indigo Portal Card** bearing an open-door / split cloud drive aperture (`OpenDrive`).
- **Outputs:**
  - `paint_logo(painter, rect, pal)`: pure vector painter for the custom native titlebar and UI branding.
  - `render_logo_rgba(size: u32) -> Vec<u8>`: deterministic procedural RGBA bitmap generator shared across `ViewportBuilder::with_icon` (`64x64`) and Windows/macOS `tray_icon::Icon::from_rgba` (`32x32`).

### 3.2 Dual Palette: Colorful Bento Default + Dark Studio (`theme.rs`)

- `ThemeMode` (default `ThemeMode::ColorPop`, toggleable to `ThemeMode::DarkStudio`):
  - **`ColorPop` (Image-2 Theme):** Periwinkle/Indigo backdrop (`#645DF6`), crisp off-white cards (`#F8FAFC`) & alt surfaces (`#EEF1F8`), bold dark ink borders & layered lips (`#1C1E24`), **Hot Pink (`#FF3D7F`)**, **Electric Cyan (`#00D4FF`)**, **Cobalt/Indigo (`#4F46E5`)**, emerald OK (`#10B981`), warm amber (`#F59E0B`), and high-contrast charcoal text (`#1A1D24`).
  - **`DarkStudio`:** Deep Obsidian canvas (`#13151B`), elevated slate cards (`#1C1F28` / `#252A36`), using the same **Pink (`#FF3D7F`)**, **Cyan (`#00D4FF`)**, and **Periwinkle (`#7B74FF`)** bento accents with `#090A0D` tactile outlines and `#F5F7FA` text.
- **Layered Bento Card Primitive (`widgets.rs`):** Offset pink/cyan/ink back-plate behind a `2px` dark-ink-bordered card frame (`CornerRadius::same(14)`).

### 3.3 Custom Native Titlebar (`desktop_titlebar.rs`)

- `desktop_window.rs` sets `.with_decorations(false)` and `.with_icon(Some(Arc::new(logo_icon_data())))`.
- Top draggable bar (`Sense::drag()` -> `ViewportCommand::StartDrag`; double-click -> toggle `Maximized`):
  - **Left:** Custom OpenDrive vector logo, `"OpenDrive"` title, sync health pill, and **Total Multi-Account Storage Badge** (`summarize_total_storage(&state.accounts)` displaying aggregate `used / total` across all accounts or active accounts).
  - **Right:** Quick **`Pause All`** / **`Resume All`** controls, **`Open Folder`**, **`Theme`** switch (`Color` / `Dark`), **`Settings`** modal trigger (`IconKind::Settings`), **`Connect Account` (`+`)**, and custom window chrome (**Minimize**, **Maximize / Restore**, **Close -> Hide to Tray**).

### 3.4 Bento Grid Main Workspace (`desktop_bento.rs`, `desktop_ledger.rs`)

- Replaces the stacked vertical rail with a structured **Bento Grid** centered up to `980px`:
  - **Top Bento Row (2 columns on wide viewports, stacked when narrow):**
    - **Storage & Mount Bento Card** (layered Pink/Cyan accent): Total Quota progress bar + aggregate used/free summary across all connected accounts, active OS mount path + Windows `O:\` status chip, and quick folder/connect actions.
    - **Sync Pulse Bento Card:** Compact telemetry sub-tiles (`Active Transfers`, `Upload Queue`, `Sync Errors`) and current transfer throughput readout.
  - **Bottom Full-Width Bento Card (`Connected Accounts`):** Grid/list of account cards showing bound **Avatar** (circular image or monogram fallback), Nickname, Email, Status Pill, Local Mount Path, per-account Drive storage bar/readout, **`Sign In`** & **`Copy Link`** buttons on `NeedsReauth` / `Error` (plus inline pending banner), active sync toggle switch, `Read-Only` lock pill, and `Remove` (plus inline Connect / Re-authenticate drawer when expanded).

### 3.5 Settings Modal (`desktop_modal.rs`)

- `ControlCenterApp` tracks `settings_open: bool`. Settings controls no longer occupy space on the main canvas.
- Clicking **Settings** opens a centered modal overlay (dimmed backdrop, `Escape` or **Done / Close** dismissal) containing:
  - `Autostart on Login` switch
  - `Mount Virtual Drive (O:\)` switch
  - `Export Google Docs as Office Files` switch
  - `Bandwidth Limit` 5-step tactile slider
  - `Local Cache Limit` 5-step tactile slider + `used / max (%)` readout
  - `Purge Unpinned Cache` button

---

## 4. Integrated System Tray & Hide-on-Close Lifecycle

- **Crate Dependencies (`opendrive-app/Cargo.toml`):**
  - Keep `tray = ["dep:tray-icon", "dep:winit"]` for explicit CLI/Linux opt-in while adding `tray-icon = "0.19"` under `[target.'cfg(any(windows, target_os = "macos"))'.dependencies]` so desktop builds on Windows and macOS always bundle the integrated System Tray without requiring GTK/AppIndicator on headless Linux CI (`ubuntu-latest`).
- **Tray Bridge (`desktop_tray.rs` & `desktop_window.rs`):**
  - Initialize `TrayIconBuilder` inside `eframe::run_native` setup on Windows/macOS using `render_logo_rgba(32)` and tray menu items: **Show OpenDrive**, **Open OpenDrive Folder**, **Pause Sync**, **Resume Sync**, and **Quit**.
  - Register `TrayIconEvent::set_event_handler` and `MenuEvent::set_event_handler` forwarding queued `TrayUiEvent`s into `ControlCenterApp` + waking the GUI via `egui_ctx.request_repaint()`.
  - Tray icon click or **Show OpenDrive** sends `ViewportCommand::Visible(true)` and `ViewportCommand::Focus`.
  - Window Close (`X` button in custom bar or OS `CloseRequested` when `!quit_requested`) cancels close (`ViewportCommand::CancelClose`) and hides the window (`ViewportCommand::Visible(false)`) so background mounts and pollers keep running cleanly. Selecting **Quit** from the tray menu sets `quit_requested = true` and closes the application.

---

## 5. Architecture Boundaries, LOC Limits & Test Plan

- Keep every `.rs` file `<= 300` physical lines (`npm run lint:loc`), splitting `widgets.rs`, `desktop.rs`, `desktop_bento.rs`, `desktop_titlebar.rs`, `desktop_modal.rs`, `logo.rs`, and `avatars.rs` cleanly.
- Maintain `>= 90%` Rust coverage (`cargo llvm-cov --workspace --locked`) by isolating `tray-icon` OS event hooks inside `desktop_tray.rs` / `desktop_window.rs` (same exclusion pattern as `desktop_window.rs` / `tray_loop.rs` if OS tray creation requires a desktop session, while unit/integration-testing pure menu routing, state actions, logo RGBA pixels, avatar fallback, Bento/modal/titlebar rendering, shared/shortcut Drive/Sync hydration, and total-storage formatters).
- Exclude none of the core VFS/Drive/UI logic:
  1. `opendrive-drive`: test `shortcutDetails`, `list_shared_with_me`, `About::photo_url`, and `download_bytes` against `FakeDriveServer`.
  2. `opendrive-sync`: test `SHARED_WITH_ME_DRIVE_ID` creation/hydration, folder shortcut target hydration, `SyncEngine::list` failing when `DriveClient` absent or hydration fails (never returning empty `Cached` or empty `Placeholder` falsely).
  3. `opendrive-app`: test `summarize_total_storage`, `render_logo_rgba`, `ControlCenterApp` modal toggle + custom bar `Pause All`/`Resume All`/theme actions + hide-to-tray `close_requested` state transitions + `headless_egui` rendering of `ColorPop` & `DarkStudio` Bento Grid, custom bar, avatar fallback & texture path, and Settings modal.
