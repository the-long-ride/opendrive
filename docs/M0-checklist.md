# M0 Spike checklist

- [x] OAuth loopback + PKCE + token exchange/refresh + keyring
- [x] Drive `about` / `files.list` / `changes` / range download / resumable upload
- [x] SQLite store (nodes, change tokens, upload queue)
- [x] SyncEngine list/lookup/stat/read/create/remove + hydrate_folder
- [x] Upload queue worker + local staging
- [x] Linux FUSE read path (lookup/getattr/readdir/read)
- [x] CLI: `authorize`, `list-root`, `--self-check`
- [x] Provisional answers Q1–Q5 — see [open-questions.md](open-questions.md)
- [ ] Windows Cloud Files placeholder + hydration (stub only)
- [ ] macOS NFS read-only mount (stub only)
- [ ] GPUI window + tray

Exit: go/no-go on GPUI, cloud-filter, NFS approach — **provisional Go** (see open-questions.md).

## Manual test

```bash
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- list-root <access_token>
```
