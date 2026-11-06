<p align="center">
  <img src="docs/assets/opendrive-logo.svg" alt="OpenDrive logo" width="96" height="96" />
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

## Driverless Native Mounting

OpenDrive requires **no custom kernel drivers** (no WinFsp, Dokany, or macFUSE):

- **Windows:** Built-in Windows Cloud Files API (`CFAPI`) + optional virtual drive letter (`O:\`)
- **macOS:** Built-in localhost `NFSv3` (`nfsserve`)
- **Linux:** Standard userspace `FUSE3`

Portable archives and native installers (`NSIS` `.exe`, `.dmg`, `.AppImage`, `.deb`) are published via GitHub Releases.

## Development Setup

Requires **Rust** (`stable`) and **Node.js 24+**:

```bash
npm install
npm run desktop                         # Build & launch desktop UI (or VS Code F5)
cargo run -p opendrive-app --locked     # Run directly with Cargo
npm run test:gate                       # Format, lint, scripts, and >=90% coverage
```

## Docs

[Building](docs/building.md) · [Connect Account Guide](docs/connect-account-guide.md) · [Architecture](docs/architecture.md) · [Roadmap](docs/ROADMAP.md) · [OAuth](docs/oauth-setup.md) · [Status](docs/M0-checklist.md) · [Next](docs/NEXT.md) · [Design](DESIGN.md)

## License

MIT
