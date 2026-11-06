# What to do next

## Run Control Center UI

```bash
npm run build:ui
cargo run -p opendrive-app -- ui
```

Optional native GPUI window or system tray:

```bash
cargo run -p opendrive-app --features gpui -- ui
cargo run -p opendrive-app --features tray -- tray
```

## Mount Virtual Drive (Linux / Windows / macOS)

```bash
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- mount <access_token> [path]
```

- **Windows**: Registers and connects a `cloud-filter` CFAPI sync root per account under `%USERPROFILE%\OpenDrive\<account>`.
- **macOS**: Starts a loopback `nfsserve` NFSv3 server on `127.0.0.1` and prints the `mount_nfs` hint.
- **Linux**: Mounts the FUSE3 filesystem at `~/OpenDrive` (or `[path]`).

## Release & Beta Packaging

- Wire code signing, `.msi` / `.dmg` / `.AppImage` installers, and tag-driven artifact publishing in `.github/workflows/release.yml`.
- Validate live Google Drive OAuth + Finder/Explorer/Nautilus manual smoke tests on target desktop hosts.
