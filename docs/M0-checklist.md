# Status

## Done (engine + Linux)

- OAuth, Drive, store, SyncEngine, staging, workers, change poller
- Linux FUSE full read/write path + CLI `mount`
- Windows `ProviderBridge` (ready for cloud-filter)
- macOS `NfsBackend` (ready for nfsserve)
- Tray menu model (no live icon yet)

## Remaining

| Item | Needs |
|------|--------|
| CFAPI live | Windows host + enable `cloud-filter` dep |
| NFS live | macOS host + enable `nfsserve` |
| GPUI + tray icon | Pin GPUI; wire `tray-icon` |

These cannot be finished from a headless Linux CI agent alone.
