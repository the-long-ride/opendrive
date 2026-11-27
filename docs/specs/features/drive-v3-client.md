---
schema_version: 1
id: "feature.drive-v3-client"
title: "Google Drive v3 HTTP client and in-process fake Drive server"
kind: "feature"
status: "verified"
summary: "Thin reqwest-based Google Drive v3 client (about, shared drives, file listing, byte-range download, changes feed, resumable uploads, metadata/trash mutations, 429 backoff) and an in-process HTTP/1.1 test double."
tags: ["drive-v3", "http", "resumable-upload", "changes", "backoff", "fake-drive", "testing"]
source_paths: ["crates/opendrive-drive/src/client.rs", "crates/opendrive-drive/src/lib.rs", "crates/opendrive-drive/src/retry.rs", "crates/opendrive-drive/src/token.rs", "crates/opendrive-drive/src/transfer.rs", "crates/opendrive-drive/src/types.rs", "crates/opendrive-drive/tests/drive_coverage_tests.rs", "crates/opendrive-drive/tests/drive_http_tests.rs", "crates/opendrive-drive/tests/drive_shared_tests.rs", "crates/opendrive-drive/tests/token_refresh_tests.rs", "crates/opendrive-fake-drive/src/extras.rs", "crates/opendrive-fake-drive/src/http.rs", "crates/opendrive-fake-drive/src/lib.rs", "crates/opendrive-fake-drive/src/server.rs", "crates/opendrive-fake-drive/src/types.rs", "crates/opendrive-fake-drive/tests/fake_drive_tests.rs"]
related_specs: ["feature.oauth-credentials", "workflow.sync-hydration-conflicts"]
last_doc_update_at: "2026-11-26T18:05:00Z"
last_verified_at: "2026-11-26T18:05:00Z"
last_verified_code_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_commit: "600d2df0ad0c4d97420d87fa94048e4eac1b8129"
last_source_change_at: "2026-11-26T14:29:37+07:00"
source_fingerprint: "sha256:37d4177c7d89c85a43219a8f1981aa0de7d341df95652c88bf98bf90aa488a9b"
---

# Google Drive v3 HTTP client and in-process fake Drive server

## Purpose and boundaries
`opendrive-drive` provides the async HTTP client for Google Drive API v3 operations, and `opendrive-fake-drive` provides a deterministic in-process Drive v3 state machine and HTTP/1.1 server for integration testing without network access.

In scope: bearer-authenticated Drive v3 REST calls, JSON deserialization, HTTP error classification, exponential backoff on HTTP 429, byte-range media downloads, resumable upload sessions/chunks, change feed tokens, and the `FakeDrive`/`FakeDriveServer` test server.
Out of scope: OAuth token acquisition/refresh (`opendrive-auth`), local SQLite persistence (`opendrive-store`), and upload queue/conflict orchestration (`opendrive-sync`).

## Observable behavior

### `DriveClient` API and error mapping (`opendrive-drive`)
- `DriveClient::new(access_token)` targets `https://www.googleapis.com/drive/v3` and `https://www.googleapis.com/upload/drive/v3`; `with_base_urls` overrides both bases (trailing `/` trimmed). `with_token_refresher` and `with_proactive_refresh_age` wire `TokenStore` (`token.rs`), which proactively refreshes via `TokenRefresher` after 50 minutes (or `proactive_age`) and performs single-flight `refresh_after_unauthorized` retry on HTTP `401` (coalescing concurrent requests via `refresh_gate`, exposed on `send_authed`/`send_authed_json` and `access_token()`). `403` never triggers refresh.
- HTTP status mapping in `DriveClient::check`:
  - `401` or `403` -> `DriveError::Auth`
  - `429` -> `DriveError::RateLimited`
  - `404` -> `DriveError::NotFound`
  - Non-success other than `308` -> `DriveError::Http("{status}: {body}")`
- `with_backoff(f, max_attempts)` retries only `Err(DriveError::RateLimited)` while `attempt + 1 < max_attempts`, sleeping `(250 << attempt).min(30_000)` milliseconds between attempts.
- Supported endpoints on `DriveClient`:
  - `about()` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/about?fields=user(emailAddress,displayName,photoLink),storageQuota` -> `About` (including `photo_url` mapped from `user.photoLink`).
  - `list_shared_drives(page_token)`: `GET {api_base}/drives?pageSize=100&fields=nextPageToken,drives(id,name)` (+ optional `pageToken`) -> `SharedDriveList`.
  - `root_folder_id()` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/files/root?fields=id&supportsAllDrives=true` -> canonical root folder ID.
  - `list_files(parent, page_token, supports_all_drives)` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/files` with `q="'{parent}' in parents and trashed = false"`, `pageSize=100`, `FILE_FIELDS` (including `ownedByMe`, `sharedWithMeTime`, `shortcutDetails(targetId,targetMimeType)`), optional `supportsAllDrives=true&includeItemsFromAllDrives=true` and `pageToken` -> `FileList` (populating `DriveFile::{owned_by_me, shared_with_me, shortcut_target_id, shortcut_target_mime}`).
  - `list_top_unparented_files()` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/files` with `q="trashed = false and not 'root' in parents"`, `orderBy="viewedByMeTime desc,modifiedTime desc"`, filtering `f.parents.is_empty()` to surface unparented owned and link-opened top-level files.
  - `list_shared_with_me(page_token)` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/files` with `q="sharedWithMe = true and trashed = false"`, `supportsAllDrives=true&includeItemsFromAllDrives=true`, and `FILE_FIELDS` -> `FileList`.
  - `download_bytes(url)`: fetches raw bytes from `url` without bearer auth (checking status via `check()` -> `Vec<u8>`) for account avatar downloads.
  - `download_range(file_id, offset, len)`: returns empty `Bytes` if `len == 0`; otherwise `GET {api_base}/files/{file_id}?alt=media` with `Range: bytes={offset}-{end}`. Returns bytes on `200`/`206`; delegates other statuses to `check()` (note: `308` passes `check()` and yields `Ok(Bytes::new())`).
  - `get_start_page_token()`: `GET {api_base}/changes/startPageToken?supportsAllDrives=true` -> `String`.
  - `changes(page_token)`: `GET {api_base}/changes` (`supportsAllDrives=true`, `includeItemsFromAllDrives=true`) -> `ChangeList` where `new_start_token` prefers `newStartPageToken` or falls back to `nextPageToken`, and each change resolves `file_id` from `fileId` or `file.id`.
  - `create_resumable(metadata)`: `POST {upload_base}/files?uploadType=resumable&supportsAllDrives=true` with JSON metadata, returning the `Location` response header (`DriveError::Other("missing Location header")` if absent).
  - `upload_chunk(session_uri, offset, total, data)`: `PUT {session_uri}` (without bearer auth) with `Content-Length` and `Content-Range` (`bytes */{total}` when `data` is empty; `bytes {offset}-{end}/{total}` otherwise). Returns `Ok(None)` on `308`, `Ok(Some(DriveFile))` on `200`/`201`, or maps errors via `check()`.
  - `trash(file_id)`: `PATCH {api_base}/files/{file_id}?supportsAllDrives=true` with `{"trashed": true}`.
  - `update_metadata(file_id, patch)`: `PATCH {api_base}/files/{file_id}?supportsAllDrives=true&fields={FILE_FIELDS}` with `patch` -> `DriveFile`.
  - `copy_file(file_id, name, parent_id)`: `POST {api_base}/files/{file_id}/copy?supportsAllDrives=true&fields={FILE_FIELDS}` with `{"name": name, "parents": [parent_id]}` -> `DriveFile`.
  - `move_file(file_id, add_parent, remove_parents, name)`: `PATCH {api_base}/files/{file_id}?supportsAllDrives=true&fields={FILE_FIELDS}&addParents={add_parent}` (+ optional `removeParents`) -> `DriveFile`.
  - `DriveClient::native_doc_link(file_id)` returns `https://drive.google.com/open?id={file_id}`. `is_native_google_doc(mime)` matches Google Docs, Sheets, and Slides MIME types.

### In-process test server (`opendrive-fake-drive`)
- `FakeDrive` stores mutex-guarded files (pre-seeded with `"root"` folder `"My Drive"` at `"rev-1"`, with `list_children` treating `"root"` and `root_real_id` as aliases), shared drives, `FakeAbout`, a monotonic `change_token` starting at `1`, a `FakeChange` log, active `UploadSession` buffers, a one-shot `error_queue`, a sticky `forced_error`, and `FakeExtras` (supporting `sharedWithMe` / `sharedWithMeTime`, `mark_unowned`, `set_root_real_id`, `list_not_in_root`, `shortcutDetails` target metadata, `/about` `photoLink`, and `/assets/*` avatar binary fixtures).
- `FakeDriveServer::start(Arc<FakeDrive>)` binds `127.0.0.1:0` and serves HTTP/1.1 routes `/drive/v3/about`, `/drive/v3/drives`, `/drive/v3/files`, `/drive/v3/files/{id}`, `/drive/v3/files/{id}/copy` (`POST` server-side file copy), `PATCH /drive/v3/files/{id}` (`addParents`/`removeParents`/`name`/`trashed`), `/drive/v3/changes/startPageToken`, `/drive/v3/changes`, `/upload/drive/v3/files`, `/upload/session/{sid}`, and `/assets/*`. On each request, `drive.take_error()` is checked first (draining `error_queue` before `forced_error`) to inject HTTP error statuses.

## Code map
- `crates/opendrive-drive/src/lib.rs` — crate exports for `DriveClient`, `TokenRefresher`, `TokenFuture`, `with_backoff`, `is_native_google_doc`, and Drive data types.
- `crates/opendrive-drive/src/token.rs` — `TokenRefresher`, `TokenFuture`, `TokenStore` (50m proactive refresh + single-flight `401` coalescing), and `execute_authed_request`.
- `crates/opendrive-drive/src/types.rs` — `DriveError`, `FileJson`, `ShortcutDetailsJson`, `DriveFile` (with `owned_by_me`, `shared_with_me`, `shortcut_target_id`, `shortcut_target_mime`), `About` (with `photo_url`), `FileList`, `SharedDrive`, `SharedDriveList`, `ChangeList`, `Change`, `is_native_google_doc`.
- `crates/opendrive-drive/src/retry.rs` — `with_backoff` and 4 unit tests.
- `crates/opendrive-drive/src/client.rs` — `DriveClient` core, `with_token_refresher`, `with_proactive_refresh_age`, `access_token`, `check`, `send_authed`, `send_authed_json`, `about`, `list_shared_drives`, `list_files`, `list_shared_with_me`, `download_range`, `download_bytes`, `native_doc_link`.
- `crates/opendrive-drive/src/transfer.rs` — `root_folder_id`, `list_top_unparented_files`, `get_start_page_token`, `changes`, `create_resumable`, `upload_chunk`, `trash`, `update_metadata`, `copy_file`, `move_file`.
- `crates/opendrive-drive/tests/{drive_coverage_tests,drive_http_tests,drive_shared_tests,token_refresh_tests}.rs` — HTTP/TCP mock integration tests covering `about`, shared drives, `list_top_unparented_files`, `list_shared_with_me`, `shortcutDetails`, `download_bytes`/`download_range` truncated-body errors, resumable uploads, backoff, and `401`/proactive/concurrent token refresh.
- `crates/opendrive-fake-drive/src/lib.rs` — `FakeDrive` in-memory state and mutation/fault-injection methods.
- `crates/opendrive-fake-drive/src/extras.rs` — `FakeDrive` extension methods (`set_photo_url`, `photo_url`, `mark_shared_with_me`, `list_shared_with_me`, `set_shortcut`, `set_static_asset`, `get_static_asset`, `mark_unowned`, `set_root_real_id`, `root_real_id`, `list_not_in_root`, `file_to_json`, `copy_file`, `patch_parents`).
- `crates/opendrive-fake-drive/src/types.rs` — `FakeFile`, `FakeSharedDrive`, `FakeAbout`, `FakeChange`, `UploadSession`, `FakeExtras`, `file_to_json`, `bump_rev`.
- `crates/opendrive-fake-drive/src/http.rs` — minimal HTTP/1.1 request reader, query/range/content-range parsers, and response writer.
- `crates/opendrive-fake-drive/src/server.rs` — `FakeDriveServer` TCP listener and route dispatcher.
- `crates/opendrive-fake-drive/tests/fake_drive_tests.rs` — Unit/HTTP tests covering `FakeDrive` state, resumable sessions, and malformed HTTP requests.

## Verification
- Verified in `cargo test -p opendrive-drive -p opendrive-fake-drive --locked`:
  - `opendrive-drive` unit tests (`retry.rs`) + integration tests (`drive_coverage_tests.rs`, `drive_http_tests.rs`, `drive_shared_tests.rs`) pass.
  - `opendrive-fake-drive/tests/fake_drive_tests.rs` pass against in-process `FakeDriveServer` and mock TCP listeners.
- Gap: Live Google Drive v3 API behavior is not exercised in automated tests; all HTTP verification runs against `FakeDriveServer` or an inline raw TCP listener.

## Related documentation
- [OAuth and credentials](../features/oauth-and-credentials.md)
- [Sync, hydration, and conflicts](../workflows/sync-hydration-and-conflicts.md)
