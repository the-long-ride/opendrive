# GPUI spike plan

1. Pin `gpui` from crates.io or zed-industries git tag.
2. Minimal window: title "OpenDrive", shows account list from `AppController`.
3. Tray via `tray-icon` (separate from GPUI event loop if needed).
4. Channel bridge: UI sends `AppCommand`, receives `AppEvent`.
5. Smoke on Windows + Linux + macOS before M4.

Fallback: keep CLI-only until GPUI tray is stable; core stays UI-agnostic.
