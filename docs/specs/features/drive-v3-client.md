---
schema_version: 1
id: "feature.drive-v3-client"
title: "Google Drive v3 HTTP client and in-process fake Drive server"
kind: "feature"
status: "verified"
summary: "Thin reqwest-based Google Drive v3 client (about, shared drives, file listing, byte-range download, changes feed, resumable uploads, metadata/trash mutations, 429 backoff) and an in-process HTTP/1.1 test double."
tags: ["drive-v3", "http", "resumable-upload", "changes", "backoff", "fake-drive", "testing"]
source_paths: ["crates/opendrive-drive/src/client.rs", "crates/opendrive-drive/src/lib.rs", "crates/opendrive-drive/src/retry.rs", "crates/opendrive-drive/src/transfer.rs", "crates/opendrive-drive/src/types.rs", "crates/opendrive-drive/tests/drive_http_tests.rs", "crates/opendrive-drive/tests/drive_shared_tests.rs", "crates/opendrive-fake-drive/src/extras.rs", "crates/opendrive-fake-drive/src/http.rs", "crates/opendrive-fake-drive/src/lib.rs", "crates/opendrive-fake-drive/src/server.rs", "crates/opendrive-fake-drive/src/types.rs", "crates/opendrive-fake-drive/tests/fake_drive_tests.rs"]
related_specs: ["feature.oauth-credentials", "workflow.sync-hydration-conflicts"]
last_doc_update_at: "2026-11-17T21:18:00Z"
last_verified_at: "2026-11-17T21:18:00Z"
last_verified_code_commit: "fe2045703b98bdd6f83c55ecdf88d92b31bb1495"
last_source_change_commit: "4c14192584395c4c33f9bbc33815003cb49c3604"
last_source_change_at: "2026-11-18T13:03:12+07:00"
source_fingerprint: "sha256:034502fc4d253802dc03ac23dd49688a70d67f6f454e161cb1a6b4daaa1da997"
---

# Google Drive v3 HTTP client and in-process fake Drive server

## Purpose and boundaries
`opendrive-drive` provides the async HTTP client for Google Drive API v3 operations, and `opendrive-fake-drive` provides a deterministic in-process Drive v3 state machine and HTTP/1.1 server for integration testing without network access.

In scope: bearer-authenticated Drive v3 REST calls, JSON deserialization, HTTP error classification, exponential backoff on HTTP 429, byte-range media downloads, resumable upload sessions/chunks, change feed tokens, and the `FakeDrive`/`FakeDriveServer` test server.
Out of scope: OAuth token acquisition/refresh (`opendrive-auth`), local SQLite persistence (`opendrive-store`), and upload queue/conflict orchestration (`opendrive-sync`).

## Observable behavior

### `DriveClient` API and error mapping (`opendrive-drive`)
- `DriveClient::new(access_token)` targets `https://www.googleapis.com/drive/v3` and `https://www.googleapis.com/upload/drive/v3`; `with_base_urls` overrides both bases (trailing `/` trimmed).
- HTTP status mapping in `DriveClient::check`:
  - `401` or `403` -> `DriveError::Auth`
  - `429` -> `DriveError::RateLimited`
  - `404` -> `DriveError::NotFound`
  - Non-success other than `308` -> `DriveError::Http("{status}: {body}")`
- `with_backoff(f, max_attempts)` retries only `Err(DriveError::RateLimited)` while `attempt + 1 < max_attempts`, sleeping `(250 << attempt).min(30_000)` milliseconds between attempts.
- Supported endpoints on `DriveClient`:
  - `about()` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/about?fields=user,storageQuota` -> `About` (including `photo_url` mapped from `user.photoLink`).
  - `list_shared_drives(page_token)`: `GET {api_base}/drives?pageSize=100&fields=nextPageToken,drives(id,name)` (+ optional `pageToken`) -> `SharedDriveList`.
  - `list_files(parent, page_token, supports_all_drives)` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/files` with `q="'{parent}' in parents and trashed = false"`, `pageSize=100`, `FILE_FIELDS` (including `shortcutDetails(targetId,targetMimeType)`), optional `supportsAllDrives=true&includeItemsFromAllDrives=true` and `pageToken` -> `FileList` (populating `DriveFile::{shortcut_target_id, shortcut_target_mime}`).
  - `list_shared_with_me(page_token)` (wrapped in `with_backoff(..., 5)`): `GET {api_base}/files` with `q="sharedWithMe = true and trashed = false"`, `supportsAllDrives=true&includeItemsFromAllDrives=true`, and `FILE_FIELDS` -> `FileList`.
  - `download_bytes(url)`: fetches raw image/asset bytes over HTTPS (`200` / `206` -> `Bytes`) for account avatar downloads.
  - `download_range(file_id, offset, len)`: returns empty `Bytes` if `len == 0`; otherwise `GET {api_base}/files/{file_id}?alt=media` with `Range: bytes={offset}-{end}`. Returns bytes on `200`/`206`; delegates other statuses to `check()` (note: `308` passes `check()` and yields `Ok(Bytes::new())`).
  - `get_start_page_token()`: `GET {api_base}/changes/startPageToken?supportsAllDrives=true` -> `String`.
  - `changes(page_token)`: `GET {api_base}/changes` (`supportsAllDrives=true`, `includeItemsFromAllDrives=true`) -> `ChangeList` where `new_start_token` prefers `newStartPageToken` or falls back to `nextPageToken`, and each change resolves `file_id` from `fileId` or `file.id`.
  - `create_resumable(metadata)`: `POST {upload_base}/files?uploadType=resumable&supportsAllDrives=true` with JSON metadata, returning the `Location` response header (`DriveError::Other("missing Location header")` if absent).
  - `upload_chunk(session_uri, offset, total, data)`: `PUT {session_uri}` (without bearer auth) with `Content-Length` and `Content-Range` (`bytes */{total}` when `data` is empty; `bytes {offset}-{end}/{total}` otherwise). Returns `Ok(None)` on `308`, `Ok(Some(DriveFile))` on `200`/`201`, or maps errors via `check()`.
  - `trash(file_id)`: `PATCH {api_base}/files/{file_id}` with `{"trashed": true}`.
  - `update_metadata(file_id, patch)`: `PATCH {api_base}/files/{file_id}?supportsAllDrives=true` with `patch` -> `DriveFile`.
  - `DriveClient::native_doc_link(file_id)` returns `https://drive.google.com/open?id={file_id}`. `is_native_google_doc(mime)` matches Google Docs, Sheets, and Slides MIME types.

### In-process test server (`opendrive-fake-drive`)
- `FakeDrive` stores mutex-guarded files (pre-seeded with `"root"` folder `"My Drive"` at `"rev-1"`), shared drives, `FakeAbout`, a monotonic `change_token` starting at `1`, a `FakeChange` log, active `UploadSession` buffers, a one-shot `error_queue`, a sticky `forced_error`, and `FakeExtras` (supporting `sharedWithMe` items, `shortcutDetails` target metadata, `/about` `photoLink`, and `/assets/*` avatar binary fixtures).
- `FakeDriveServer::start(Arc<FakeDrive>)` binds `127.0.0.1:0` and serves HTTP/1.1 routes `/drive/v3/about`, `/drive/v3/drives`, `/drive/v3/files`, `/drive/v3/files/{id}`, `/drive/v3/changes/startPageToken`, `/drive/v3/changes`, `/upload/drive/v3/files`, `/upload/session/{sid}`, and `/assets/*`. On each request, `drive.take_error()` is checked first (draining `error_queue` before `forced_error`) to inject HTTP error statuses.

## Code map
- `crates/opendrive-drive/src/lib.rs` — crate exports for `DriveClient`, `with_backoff`, `is_native_google_doc`, and Drive data types.
- `crates/opendrive-drive/src/types.rs` — `DriveError`, `FileJson`, `ShortcutDetailsJson`, `DriveFile` (with `shortcut_target_id`, `shortcut_target_mime`), `About` (with `photo_url`), `FileList`, `SharedDrive`, `SharedDriveList`, `ChangeList`, `Change`, `is_native_google_doc`.
- `crates/opendrive-drive/src/retry.rs` — `with_backoff` and 4 unit tests.
- `crates/opendrive-drive/src/client.rs` — `DriveClient` core, `check`, `send_json`, `about`, `list_shared_drives`, `list_files`, `list_shared_with_me`, `download_range`, `download_bytes`, `native_doc_link`.
- `crates/opendrive-drive/src/transfer.rs` — `get_start_page_token`, `changes`, `create_resumable`, `upload_chunk`, `trash`, `update_metadata`.
- `crates/opendrive-drive/tests/drive_http_tests.rs` — 4 HTTP integration tests against `FakeDriveServer` and a raw TCP mock.
- `crates/opendrive-drive/tests/drive_shared_tests.rs` — integration test verifying `about().photo_url`, `list_shared_with_me`, `shortcutDetails`, and `download_bytes`.
- `crates/opendrive-fake-drive/src/lib.rs` — `FakeDrive` in-memory state and mutation/fault-injection methods.
- `crates/opendrive-fake-drive/src/extras.rs` — `FakeExtras`, `shared_with_me` / `shortcutDetails` queries, `photoLink`, and `/assets/*` fixture routes.
- `crates/opendrive-fake-drive/src/types.rs` — `FakeFile`, `FakeSharedDrive`, `FakeAbout`, `FakeChange`, `UploadSession`, `bump_rev`.
- `crates/opendrive-fake-drive/src/http.rs` — minimal HTTP/1.1 request reader, query/range/content-range parsers, and response writer.
- `crates/opendrive-fake-drive/src/server.rs` — `FakeDriveServer` TCP listener and route dispatcher.
- `crates/opendrive-fake-drive/tests/fake_drive_tests.rs` — 3 unit/HTTP tests covering `FakeDrive` state, resumable sessions, and malformed HTTP requests.

## Verification
- Existing tests observed in source (not executed during this documentation pass):
  - `crates/opendrive-drive/src/retry.rs`: 4 unit tests (`immediate_success`, `retries_on_rate_limited_then_succeeds`, `exhausts_max_attempts`, `does_not_retry_non_rate_limit_error`).
  - `crates/opendrive-drive/tests/drive_http_tests.rs`: 4 integration tests (`about_and_shared_drives`, `list_download_update_trash_and_changes`, `resumable_upload_multi_chunk_and_empty`, `error_status_mapping_and_backoff_recovery`).
  - `crates/opendrive-fake-drive/tests/fake_drive_tests.rs`: 3 tests (`fake_drive_crud_changes_and_errors`, `fake_drive_resumable_upload_session`, `fake_drive_server_edge_routes_and_malformed_requests`).
- Gap: Live Google Drive v3 API behavior is not exercised in automated tests; all HTTP verification runs against `FakeDriveServer` or an inline raw TCP listener.

## Related documentation
- [OAuth and credentials](../features/oauth-and-credentials.md)
- [Sync, hydration, and conflicts](../workflows/sync-hydration-and-conflicts.md)
