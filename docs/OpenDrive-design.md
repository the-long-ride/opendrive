# OpenDrive — Design Document (v0.2)

The full design document (37 KB) was provided as the project attachment.

**Source of truth:** keep `OpenDrive-design.md` from the original conversation attachment in this `docs/` folder when cloning, or request the maintainers to upload the complete file.

## Quick reference

| Item | Choice |
|------|--------|
| Engine | Native Rust `VfsEngine` (no rclone) |
| Windows | Cloud Files API (`cloud-filter`) |
| Linux | FUSE3 (`fuser`) |
| macOS | localhost NFSv3 (`nfsserve`) |
| UI | GPUI |
| Auth | BYO OAuth client + PKCE loopback |
| DB | SQLite (rusqlite, WAL) |
| License | MIT |

## Milestones

- **M0** Spike (OAuth, FUSE/CFAPI/NFS hello-world, GPUI)
- **M1** Reference engine on Linux
- **M2** Windows frontend
- **M3** macOS frontend
- **M4** Product / Alpha
- **M5** Polish / Beta

See original design for architecture, safety invariants, dependency policy, CI/CD, open questions Q1–Q10.
