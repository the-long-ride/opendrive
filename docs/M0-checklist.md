# M0 Spike checklist

- [x] OAuth loopback + PKCE + token exchange/refresh + keyring
- [x] Drive `about` / `files.list` / `changes` / range download / resumable upload
- [x] SQLite store (nodes, change tokens, upload queue)
- [x] SyncEngine list/lookup/stat/read/create/remove + hydrate_folder
- [x] Upload queue worker
- [x] Linux FUSE read path (lookup/getattr/readdir/read)
- [ ] Windows Cloud Files placeholder + hydration (stub only)
- [ ] macOS NFS read-only mount (stub only)
- [ ] GPUI window + tray
- [ ] Answer open questions Q1–Q5

Exit: go/no-go on GPUI, cloud-filter, NFS approach.
