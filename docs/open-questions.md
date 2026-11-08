# Open questions (M0) — provisional answers

## Q1. Cloud Files: sibling roots, nested, min Windows, non-NTFS

- **Sibling roots:** Supported. OneDrive and third-party providers register multiple sync roots under a parent folder. Nested sync roots are not supported by the Cloud Files API.
- **Min version:** Windows 10 1809+ (Cloud Files API introduction). Prefer Win10 21H2+ / Win11 for stability.
- **Non-NTFS:** Not supported; CFAPI requires NTFS.

## Q2. cloud-filter (0.0.x) vs own wrapper

- `cloud-filter` is MIT and targets the callbacks we need (placeholders, hydration, pin).
- Risk: pre-1.0 API churn. Mitigation: thin adapter trait; vendor/fork if needed; fallback to `windows` crate bindings.
- Many sync roots: treat as one provider with multiple roots; validate in M2 spike on real hardware.

## Q3. macOS NFS

- **Privileges:** `mount_nfs` to localhost often works without elevation for user mounts under `$HOME`; confirm per macOS version.
- **AppleDouble:** Filter `._*` and `.DS_Store` in readdir; do not upload them.
- **Finder/Spotlight:** Expect extra metadata probes; keep attr cache short.
- **Throughput:** Adequate for interactive use; large sequential reads may lag vs native File Provider.
- File Provider remains post-v1 option.

## Q4. fuser vs fuse3

- Ship with `fuser` (MIT, used in production by others).
- Keep adapter boundary so `fuse3` crate can replace if `fuser` stalls.
- Notifier/invalidation: use engine event stream + FUSE notify where available.

## Q5. GPUI

- Pin to a git revision or published crate once chosen; keep UI thin.
- Windows/Linux backends exist but are younger than macOS — smoke-test tray + window on all three in M0.
- Accessibility/IME: verify basic text input before beta.
- Tray: `tray-icon` (+ platform notes); fallback to no-tray if event-loop conflicts.

## Go / no-go (suggested)

| Piece | Stance |
|-------|--------|
| GPUI | **Go** with pin + thin UI; revisit if tray/Windows regresses |
| cloud-filter | **Go** with adapter; vendor if API breaks |
| NFS approach | **Go** for v1; File Provider later |
