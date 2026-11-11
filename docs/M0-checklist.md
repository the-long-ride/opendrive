# M0 Spike checklist

- [x] OAuth loopback + PKCE + token exchange/refresh + keyring
- [x] Drive `about` / `files.list` / `changes` / range download / resumable upload
- [x] SQLite store (nodes, change tokens, upload queue)
- [x] SyncEngine list/lookup/stat/read/create/remove + hydrate_folder
- [x] Upload queue worker + local staging + change poller
- [x] Linux FUSE read path (lookup/getattr/readdir/read)
- [x] CLI: authorize, list-root, status, --self-check
- [x] AppController (UI-agnostic)
- [x] Provisional Q1–Q5 answers
- [x] CFAPI callback map documented (implementation = M2)
- [ ] Windows Cloud Files live register/hydrate
- [ ] macOS NFS live mount
- [ ] GPUI window + tray

Exit: provisional **Go** on GPUI / cloud-filter / NFS.
