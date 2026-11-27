<p align="center">
  <img src="https://raw.githubusercontent.com/the-long-ride/opendrive/main/docs/assets/opendrive-logo.svg" alt="OpenDrive logo" width="96" height="96" />
</p>

<h1 align="center">OpenDrive</h1>

<p align="center">
  Free, open-source multi-account Google Drive desktop client (Rust).
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-ffffff?style=flat-square&labelColor=000000" alt="License MIT" /></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/Rust-Pure%20egui-ffffff?style=flat-square&labelColor=000000&logo=rust&logoColor=white" alt="Rust Pure egui" /></a>
  <a href="https://github.com/the-long-ride/opendrive/actions/workflows/quality-gate.yml"><img src="https://img.shields.io/github/actions/workflow/status/the-long-ride/opendrive/quality-gate.yml?branch=main&style=flat-square&color=ffffff&labelColor=000000&logo=githubactions&logoColor=white&label=Quality%20Gate" alt="Quality Gate" /></a>
  <a href="https://github.com/the-long-ride/opendrive/releases"><img src="https://img.shields.io/github/v/release/the-long-ride/opendrive?style=flat-square&color=ffffff&labelColor=000000&logo=github&logoColor=white&label=Release" alt="Release" /></a>
  <a href="https://github.com/the-long-ride/opendrive/releases"><img src="https://img.shields.io/github/downloads/the-long-ride/opendrive/total?style=flat-square&color=ffffff&labelColor=000000&logo=icloud&logoColor=white&label=Downloads" alt="Downloads" /></a>
  <a href="https://github.com/the-long-ride/opendrive/releases"><img src="https://img.shields.io/badge/Platforms-Win%20%C2%B7%20macOS%20%C2%B7%20Linux-ffffff?style=flat-square&labelColor=000000" alt="Platforms" /></a>
</p>

## Setup & Connect Google Drive Account

- Follow the step-by-step [Connect Account Guide](docs/connect-account-guide.md) (or click **Setup Guide** inside the app's **Connect** drawer) to create your own free Google Cloud OAuth 2.0 Desktop credentials or Service Account JSON key.

## Why OpenDrive Exists

Google Drive has no official Linux desktop client, and many third-party tools either restrict multi-account mounting behind paid plans or route OAuth tokens through third-party servers. **OpenDrive** solves this with a free, open-source, pure-Rust desktop client (`eframe` / `egui`, zero WebView) using your own Google Cloud credentials (`SecretStore` OS keyring + AES-GCM `.odcred` backup) and on-demand lazy hydration.

## Driverless Native Mounting & Clean Quota Display

OpenDrive requires **no custom kernel drivers** (no WinFsp, Dokany, or macFUSE):

- **Windows:** Built-in Windows Cloud Files API (`CFAPI`) and driverless **This PC** Explorer namespace shortcut (cleaning up any legacy `subst` drive mapping) — automatically hiding storage quota labels when any configured account reports an unknown or unlimited quota
- **macOS:** Built-in localhost `NFSv3` (`nfsserve`)
- **Linux:** Standard userspace `FUSE3`

Portable archives and native installers (`NSIS` `.exe`, `.dmg`, `.AppImage`, `.deb`) are published via GitHub Releases.

## Built-in Drive Explorer, Streaming & Open-With Rules

- **Full-Viewport Drive Explorer:** Click **Explore** on any account card to browse folders with clickable breadcrumbs, search filtering, List/Grid toggles, multi-selection (`Ctrl/Cmd+Click`, `Shift+Click`, `Ctrl/Cmd+A`), colored media-type icons, `Sync State` badges, drag-and-drop uploads, batch Copy/Move/Delete & Sync controls, and per-account session persistence across account switches.
- **Explicit Sync Controls (Safety Invariant #3):** Run **Sync Down** (atomic download + cache pinning + Windows placeholder reconciliation), **Sync Up** (files or recursive folders), or **Sync Up & Clear Local** (uploads and verifies remote `drive_id` before removing local replicas or dehydrating CFAPI placeholders).
- **Smooth Localhost `206` Video Streaming:** Streams remote videos over a token-authenticated `127.0.0.1` HTTP/1.1 `206 Partial Content` server (`MediaStreamServer`) backed by a `2 MiB` chunked `StreamCache` with concurrent head + tail `moov` pre-warming, single-flight coalescing, and read-ahead caching.
- **Custom Open-With Format Rules:** Configure per-extension (`mp4,mkv`), media-group (`@video`, `@image`, `@pdf`, `@audio`, `@code`), or wildcard (`*`) launcher overrides in **Settings**; streaming players (`vlc`, `mpv`, `iina`, `ffplay`, etc.) receive the localhost stream URL directly while other files open via OS defaults.

## CLI Commands

```bash
opendrive ui                                      # Launch desktop Control Center (default)
opendrive status                                  # Show connected accounts & config path
opendrive explore <account> [remote_path]         # List Drive folder contents
opendrive sync-down <account> <remote_path>       # Download file or folder into local cache
opendrive sync-up <account> <local_path> [parent] # Upload file or folder recursively
opendrive sync-clear <account> <remote_path>      # Upload/verify remote, then clear local copy
opendrive open-item [--dry-run] <account> <path>  # Resolve or launch item via Open-With rules
opendrive stream [--probe] <account> <path>       # Print localhost 206 stream URL (or probe range)
opendrive rename <account> <remote_path> <name>   # Rename Cloud + Local file or folder
opendrive rm|delete <account> <remote_path>       # Trash on Drive and remove local copy
opendrive cp|copy <account> <remote_path> [dest]  # Copy Cloud + Local file or folder
opendrive mv|move <account> <remote_path> <dest>  # Move Cloud + Local file or folder
opendrive --self-check                            # Run built-in end-to-end runtime self-check
```

## Development Setup

Requires **Rust** (`stable`) and **Node.js 24+**:

```bash
npm install
npm run desktop                         # Build & launch desktop UI (or VS Code F5)
cargo run -p opendrive-app --locked     # Run directly with Cargo
npm run test:gate                       # Format, lint, scripts, and >=95% coverage
```

## Docs

[Building](docs/building.md) · [Connect Account Guide](docs/connect-account-guide.md) · [Architecture](docs/architecture.md) · [Roadmap](docs/ROADMAP.md) · [OAuth](docs/oauth-setup.md) · [Status](docs/M0-checklist.md) · [Next](docs/NEXT.md) · [Design](DESIGN.md)

## License

MIT
