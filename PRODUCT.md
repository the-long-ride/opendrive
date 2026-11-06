# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

## Stack

Single-binary pure-Rust desktop Control Center built with `eframe` / `egui` (`glow` OpenGL renderer, zero WebView, zero HTML/CSS/JS runtime, zero external hot-reload server) inside `crates/opendrive-app/`. _(Confirmed by project architecture & repository state.)_

## Users

- **Primary users:** Desktop power users, developers, and multi-account Google Drive users running OpenDrive locally on Windows, Linux, or macOS who need a fast, lightweight native control center to manage multiple Google Drive accounts, virtual filesystem (VFS) cache budgets, transfer throttling, and sync state without Electron or browser overhead. _(Confirmed from repository README, architecture docs, and user brief.)_

## Product Purpose

OpenDrive is a free, open-source, multi-account Google Drive desktop client written in Rust with a native VFS engine (`VfsEngine`) and OS-native mount adapters (Linux FUSE3, Windows Cloud Files API, macOS localhost NFSv3). The **Dark Neo-Brutalist Bento** desktop UI (`eframe` / `egui`) provides a resizable frameless custom-titlebar cockpit (`Operate` mode) with dual Pink/Cyan Image-3 hard offset card shadows, an inline `"Shared with me"` virtual folder & shortcut resolution alongside lazy folder hydration, editable account aliases (defaulting before `@`) & cached Google profile avatars (`avatars.rs`), bottom thin pastel account quota bars, a scrollable Settings modal (`desktop_modal.rs`) with password-encrypted `.odcred` credential export/import (`vault.rs`), layered OpenDrive portal brand logo (`logo.rs`), automatic live desktop mounts (`%USERPROFILE%\OpenDrive\<folder>` with background token refresh, sync workers/pollers, and Windows CFAPI placeholder reconciliation), optional Windows `O:\` virtual drive mapping, integrated System Tray background minimize/restore (`desktop_tray.rs`), and live **Dark / Light Neo-Brutalism** theme switching. _(Confirmed from `README.md`, `DESIGN.md`, `crates/opendrive-core/src/lib.rs`, and `crates/opendrive-app/src/`.)_

## Positioning

Unlike single-account vendor sync clients or heavy Electron/Chromium wrappers, OpenDrive runs a lightweight local Rust `SyncEngine` backed by SQLite WAL metadata, OS keyring OAuth PKCE & Service Account JSON credentials via `SecretStore` (`OsKeyringStore`, `credentials.json` prefill), and native virtual filesystem mounts (`~/OpenDrive`), paired with an immediate-mode native `eframe` desktop window driven directly through the synchronous in-process `Bridge`. _(Confirmed from `docs/OpenDrive-design.md`, `crates/opendrive-core`, `crates/opendrive-auth`, and `crates/opendrive-app`.)_

## Operating Context

- Launched directly via `opendrive` / `opendrive ui` (or VS Code `F5`) as a resizable frameless native OS window (`OpenDrive`, `[960, 720]` default, `[540, 480]` min with 8-edge resize handles, minimal custom titlebar carrying only logo, selectable title, `Pause`/`Resume`, Theme toggle, and window controls, plus integrated System Tray icon with Show/Open Folder/Pause/Resume/Quit) interacting synchronously in-process with `Bridge` (`state_snapshot`, `add_account`, `rename_account`, `account_action`, `update_settings`, `evict_cache`, `export_encrypted_vault`, `import_encrypted_vault`).
- Defaults to **Dark Neo-Brutalism** (`ThemeMode::DarkStudio`: `#111217` dot-matrix canvas, `#181A20` primary cards with dual Pink `#FF5E93` / Cyan `#22D3EE` Image-3 hard offset shadows, `#090A0F` ink strokes, bordered status pills) with instant runtime **Dark / Color** Neo-Brutalist toggle (`ThemeMode::DarkStudio` <-> `ThemeMode::ColorPop`).
- Runs across daytime and late-night desktop sessions alongside file managers, terminals, and editors with minimal overhead (`main()` launches the native window synchronously without spawning a Tokio thread pool until an async CLI subcommand is invoked). _(Confirmed from `crates/opendrive-app/src/main.rs` and `desktop_window.rs`.)_

## Capabilities and Constraints

- **Multi-account control, authentication, shared folders & live mounting:** List accounts with `nickname`, `email`, `folder_name`, `avatar_url` (cached locally and rendered via `account_avatar` with monogram fallback), `read_only`, `enabled`, and `status` (`Connected` | `Paused` | `NeedsReauth` | `Error`); connect accounts inline via **Browser Sign-In** (OAuth 2.0 PKCE loopback with **Sign in with Google**, **Copy Link** clipboard mode, pending **Copy Link**/**Open Browser**/**Cancel Sign-In** controls, and saved `credentials.json` + OS keyring prefill) or **Service Account JSON (No Login)** (local RS256 JWT signing via `ServiceAccountKey` with raw JSON, path, or `.json` drag-and-drop stored compactly in the OS keyring via `set_secret`); re-authenticate any `NeedsReauth` or `Error` account in place via per-row **Sign In** (browser) and **Copy Link** buttons (opening a pre-populated re-auth drawer if `client_id` / `client_secret` are missing); pause, resume, toggle read-only, or remove individual accounts (cleaning stored secrets); pause all / resume all globally from the custom titlebar or System Tray; automatically activate live desktop mounts (`mount_manager`) that create `%USERPROFILE%\OpenDrive\<folder>`, refresh OAuth/Service Account tokens & profile photos from `SecretStore`/`DriveClient` on startup/auth, spawn background sync workers and `JoinHandle` change pollers, expose `"Shared with me"` + resolve Drive shortcuts alongside lazy subfolder hydration (propagating errors rather than sealing folders empty), reconcile pre-existing Windows CFAPI placeholders, and isolate multi-account Windows CFAPI roots (`CloudFilesFrontend::connect_live_at` + `ProviderBridge::with_root`).
- **VFS telemetry, aggregated storage & Settings modal:** Bento Grid workspace tiles displaying aggregated multi-account storage (`summarize_total_storage` in titlebar & Storage card) + `Active Transfers` / `Upload Queue` / `Sync Errors`, plus centered Settings modal (`desktop_modal.rs`) holding `Autostart on Login`, Windows **Mount Virtual Drive (`O:\`)**, Google Docs mode (`Link` vs `ExportOffice`), 5-step **Bandwidth Limit**, 5-step **Local Cache Limit**, and `Purge Unpinned Cache`.
- **Technical constraints:** Pure Rust workspace (`eframe` `glow` backend without `wgpu` or WebView bloat); strict physical line caps (`<= 300` lines per `.rs`, `<= 350` lines per `.mjs`); >=90% line/function/region test coverage including headless `egui::Context::run_ui` verification and contrast assertions. _(Confirmed from repository quality gates & user brief.)_

## Brand Commitments

- **Product name:** OpenDrive (MIT-licensed open-source project).
- **Aesthetic directive:** **Image-2 Neo-Pop Bento** — bold Indigo dotted canvas, off-white cards with layered Hot Pink & Cyan offset backplates, `2px` ink borders, unified OpenDrive portal logo, and custom frameless titlebar with full **DarkStudio** parity.
- **Truthfulness rule:** All telemetry, account states, mount paths, avatars, and settings shown in the Control Center bind strictly to real backend `Bridge::state_snapshot()` and `DriveClient` data; never fabricate telemetry or commercial claims.

## Evidence on Hand

- `README.md`, `DESIGN.md`, `docs/architecture.md`, `docs/OpenDrive-design.md`, `docs/M0-checklist.md`, `docs/ROADMAP.md`, `docs/oauth-setup.md`
- `crates/opendrive-core/src/lib.rs`, `crates/opendrive-core/src/controller.rs`, `crates/opendrive-platform/src/{lib,drive_mount,tray}.rs`, `crates/opendrive-fs-windows/src/{lib,provider,cfapi_sys}.rs`, `crates/opendrive-sync/src/{engine,vfs_impl,poller}.rs`, `crates/opendrive-auth/src/{lib,keyring,loopback,pkce,secret_store,service_account}.rs`, `crates/opendrive-app/src/{auth_runner,avatars,bridge,bridge_types,mount_manager,formatters,theme,icons,logo,widgets,widgets_controls,desktop,desktop_bento,desktop_drawer,desktop_modal,desktop_panels,desktop_ledger,desktop_titlebar,desktop_tray,desktop_window,ui,main}.rs` (plus `auth_runner/complete.rs`, `mount_manager/{live,sync_ops}.rs`, `desktop/{auth_handlers,settings_actions}.rs`, `desktop_drawer/sections.rs`)
- **Explicit scope:** Both desktop **Browser Sign-In** (OAuth 2.0 PKCE loopback with Copy Link) / **Service Account JSON (No Login)** flows (`auth_runner`) and CLI `opendrive authorize` perform real Google Drive token/metadata exchange and hydrate the root folder into `SyncEngine` while persisting `CredentialSet` metadata in `credentials.json` and secrets via `SecretStore`.

## Product Principles

1. **Operational legibility first:** Every account's avatar, sync state, mount folder, and write protection (`read_only`) plus combined storage usage is scannable at a glance inside structured Bento cards while verbose host settings stay tucked inside a clean modal.
2. **Reversible, tactile actions:** Pill switches, step sliders, layered cards, and high-contrast pill docks communicate active state clearly via fill, `2px` ink outline, and tactile bottom lip.
3. **Zero-bloat precision:** Pure native immediate-mode UI + integrated System Tray with zero WebView or background HTTP daemon overhead.
4. **Cross-platform honesty:** Surface the active `platform` and `mount_root` faithfully without assuming Linux-only paths on Windows or macOS.

## Accessibility & Inclusion

- High-contrast **ColorPop** (`#F8FAFC`/`#181A20` on `#645DF6`) and **DarkStudio** (`#13151B`/`#F5F7FA`) palettes with semantic status tones and enforced glyph color resolution (`#FFFFFF` on primary `#4F46E5` buttons, `pal.dock_fg` on pill docks).
- Full-width stretched containers (`set_min_width`), clear labels + helper subtitles on every control row, keyboard-navigable `TextEdit::singleline` inputs, and error banners with `Retry` / dismiss controls.
