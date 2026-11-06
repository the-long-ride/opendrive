# Changelog

All notable changes to OpenDrive are documented in this file.

## [v0.1.0] - 2026-11-19

### Added

- **Pure-Rust Desktop Control Center (`opendrive-app`)**: `eframe` / `egui` (`glow`) desktop UI in Dark & Light Neo-Brutalist Bento style with frameless 8-edge resizing, custom non-selectable draggable titlebar, `Ctrl/Cmd` keyboard & scroll-wheel zoom, bottom-right auto-dismissing toasts (5s), Settings modal, and step-by-step Google Cloud Console Setup Guide modal.
- **Multi-Account Authentication & Secure Storage**: Browser OAuth 2.0 PKCE loopback (**Sign in with Google** & **Copy Link**), in-place **Re-auth**, **Service Account JSON (No Login)** RS256 JWT exchange, OS keyring secret persistence (`OsKeyringStore`), editable account aliases, cached rounded avatars, and AES-GCM + Argon2id `.odcred` encrypted credential backup export/import.
- **Cross-Platform Virtual Filesystems & Sync Engine**: `SyncEngine` with SQLite metadata store, upload queue, cache eviction, conflict handling, lazy subfolder hydration, shared-folder shortcut materialization, Windows Cloud Files (`cloud-filter` CFAPI) + optional virtual drive letter (`O:\`), macOS localhost NFSv3 (`nfsserve`), Linux FUSE3, and System Tray integration with live usage summary.
- **CLI & Quality Gates**: `authorize`, `list-root`, `status`, `mount`, `--self-check`, and full Rust + Node CI/release automation (`windows` portable + NSIS installer, `linux` tarball + `.deb` + `.AppImage`, `macos` portable + `.dmg` installer).
