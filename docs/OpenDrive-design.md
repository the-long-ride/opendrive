# OpenDrive — Design Document

> Status: **Draft v0.2** · License: **MIT** · Language: **Rust** · UI: **GPUI** · Platforms: **Windows, macOS, Linux**
>
> Full design is in the original attachment / local `docs/OpenDrive-design.md` (37 KB). Key points below.

## Summary

Multi-account Google Drive desktop client. Unlimited accounts under one root. Native Rust VFS engine (no rclone). Per-OS frontends: Windows Cloud Files, Linux FUSE3, macOS localhost NFS. BYO OAuth credentials. MIT. No telemetry.

## Workspace layout

See root README and crates/.

## Milestones

- **M0** Spike: OAuth, FUSE/Cloud Files/NFS hello-world, GPUI window
- **M1** Reference engine (Linux)
- **M2** Windows frontend
- **M3** macOS frontend
- **M4** Product (wizard, packaging, CI/release) → Alpha
- **M5** Polish → Beta

See original design for full sections 1–17 (architecture, sync invariants, dependency policy, CI, open questions).
