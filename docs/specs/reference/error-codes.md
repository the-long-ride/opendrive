---
schema_version: 1
id: "reference.error-codes"
title: "OpenDrive Error and Warning Codes"
kind: "interface"
status: "verified"
summary: "Structured OD-* diagnostic error/warning code catalog in opendrive-core paired with credential-masked AppLogHub capture for Terminal Log, App Logs, and AI support reference."
tags: ["error-codes", "logging", "diagnostics", "masking", "llm"]
source_paths: ["crates/opendrive-app/src/runtime/app_logs.rs", "crates/opendrive-app/src/runtime/log_mask.rs", "crates/opendrive-app/tests/diagnostics_and_logging_tests.rs", "crates/opendrive-core/src/error_codes.rs"]
related_specs: ["architecture.workspace-overview", "feature.control-center-ui", "interface.cli-http-bridge"]
last_doc_update_at: "2026-11-27T02:21:00Z"
last_verified_at: "2026-11-27T02:21:00Z"
last_verified_code_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_commit: "unavailable"
last_source_change_at: "2026-11-26T17:06:00Z"
source_fingerprint: "sha256:15b90e5d3e0c47dc340ccc11512df66e5f82c039e01c1edd95736a1dcdff25cd"
---
# OpenDrive Error and Warning Codes

## Purpose and boundaries
Defines OpenDrive's structured `OD-*` error and warning catalog paired with automatic credential masking (`mask_sensitive_text`) and bounded in-memory log capture (`AppLogHub`) for the desktop **Terminal -> App Logs** view and root `llm.txt` AI troubleshooting workflows.

## Observable behavior
Every `WARN` and `ERROR` event recorded in OpenDrive's in-app **Terminal -> App Logs** view carries a structured `OD-*` diagnostic code paired with its masked message so users and AI assistants can diagnose issues quickly. Inferring `OD-*` runs against the unmasked event text while only the masked message is retained.

> All OAuth secrets (`GOCSPX-*`), tokens (`ya29.*`, `1//*`, `Bearer ...`/`BEARER ...`), Service Account private keys, vault passwords, query `code=...` parameters (preserving 3-digit HTTP status codes such as `code=403`), and full OAuth Client ID prefixes are masked before being stored in the log buffer.

## Authentication (`OD-AUTH-*`)

| Code | Severity | Meaning | Recovery |
| --- | --- | --- | --- |
| `OD-AUTH-001` | ERROR | Invalid or rejected OAuth client credentials (`invalid_client` / `unauthorized_client`) | Open Re-auth -> **Credentials** and verify the Desktop App Client ID & Secret from Google Cloud Console. |
| `OD-AUTH-002` | ERROR | OAuth token expired or revoked (`invalid_grant`, or poller `"auth"` failure after auto-refresh) | Drive calls auto-refresh at 50m and on HTTP 401; if refresh fails, click **Sign In** on the account row (or publish the consent screen beyond 7-day Testing). |
| `OD-AUTH-003` | WARN | Browser launch or `127.0.0.1` loopback redirect interrupted | Use **Copy Link** to paste the Google sign-in URL into any browser manually. |
| `OD-AUTH-004` | ERROR | Service Account JSON invalid or JWT exchange failed | Re-select/paste a valid Service Account `.json` key, confirm system clock accuracy, and share target Drive folders with `client_email`. |
| `OD-AUTH-005` | ERROR | OS keyring or secret store unavailable | Unlock the OS keyring / secret service or export/import accounts via encrypted `.odcred` backup. |
| `OD-AUTH-006` | WARN | Account requires re-authentication | Click **Sign In** or **Credentials** on the account card. |

## Sync & Drive API (`OD-SYNC-*`)

| Code | Severity | Meaning | Recovery |
| --- | --- | --- | --- |
| `OD-SYNC-001` | ERROR | Google Drive API request or sync failure | Check network connectivity and retry or refresh Explorer. |
| `OD-SYNC-002` | ERROR | Insufficient Drive scope or permission (`403` / `access_denied`) | Enable Google Drive API, add `https://www.googleapis.com/auth/drive` scope under Data Access, and re-auth. |
| `OD-SYNC-003` | WARN | Sync conflict detected | Inspect the conflict sibling file in Explorer or the local mount folder and keep the desired version. |
| `OD-SYNC-004` | ERROR | Local replica or SQLite cache error | Verify disk space and permissions on the cache/data folder or run **Purge Unpinned Cache** in Settings. |
| `OD-SYNC-005` | WARN | Rate limited by Google Drive API (`429`) | Automatic exponential backoff is active; pause briefly before retrying bulk operations. |

## OS Mounts (`OD-MNT-*`)

| Code | Severity | Meaning | Recovery |
| --- | --- | --- | --- |
| `OD-MNT-001` | ERROR | OS virtual filesystem mount failed (`CFAPI` / `FUSE3` / `NFSv3`) | Verify OS mount prerequisites (`libfuse3` on Linux, loopback NFS on macOS, NTFS/CFAPI on Windows) and stale mount locks. |
| `OD-MNT-002` | WARN | Windows `This PC` namespace registration failed | Toggle **Show OpenDrive in This PC** in Settings or check `HKCU` registry access. |
| `OD-MNT-003` | WARN | Remote mount sync interrupted | Click **Refresh** on the dashboard or check account status. |

## Streaming & Open-With (`OD-STRM-*`)

| Code | Severity | Meaning | Recovery |
| --- | --- | --- | --- |
| `OD-STRM-001` | ERROR | Localhost `206 Partial Content` media stream server error | Retry opening the item or verify account connectivity. |
| `OD-STRM-002` | WARN | Custom Open-With launcher failed | Check the executable path or command under **Settings -> Open With Rules**. |

## Configuration & Updates (`OD-CFG-*`)

| Code | Severity | Meaning | Recovery |
| --- | --- | --- | --- |
| `OD-CFG-001` | ERROR | Settings or account configuration persistence error | Ensure the OpenDrive config directory is writable. |
| `OD-CFG-002` | ERROR | Encrypted `.odcred` vault backup/restore failed | Verify the vault password (9+ chars with upper, lower, digit, symbol) and file permissions. |
| `OD-CFG-003` | WARN | GitHub release check failed | Verify HTTPS access to `api.github.com` and retry **Check Now** in Settings. |

## UI Fonts (`OD-FONT-*`)

| Code | Severity | Meaning | Recovery |
| --- | --- | --- | --- |
| `OD-FONT-001` | ERROR | Invalid or unsupported UI font file or family | Select an installed system font, enter a valid Google Font family name (e.g. `Inter`), or import a `.ttf`/`.otf`/`.ttc` file. |
| `OD-FONT-002` | WARN | Font download or persistence failed | Check network access to `fonts.googleapis.com` or browse for a local font file instead. |

## Code map
- `crates/opendrive-core/src/error_codes.rs` — `LogSeverity`, `DiagnosticSpec`, `DIAGNOSTIC_CODES`, `lookup_diagnostic`, `infer_diagnostic_code`.
- `crates/opendrive-app/src/runtime/log_mask.rs` — `mask_sensitive_text` (PEM blocks, `GOCSPX-`/`ya29.`/`1//`/`Bearer `, Client ID suffixes, key/value & JSON fields with escaped-quote and HTTP `code=NNN` handling).
- `crates/opendrive-app/src/runtime/app_logs.rs` — `AppLogEntry`, `AppLogHub` (1000-entry ring buffer), `AppLogTracingLayer`, `init_app_logging`, `format_terminal_text`, `format_app_logs_text`.
- `crates/opendrive-app/tests/diagnostics_and_logging_tests.rs` — Unit/integration tests verifying all `OD-*` codes, inference, masking, ring eviction, and `AppLogTracingLayer`.

## Verification
- `cargo test -p opendrive-app --test diagnostics_and_logging_tests --locked`: passes catalog completeness, inference ordering, masking edge cases, ring eviction, and tracing capture.
- `npm run test:coverage:node` (`scripts/test/release-packaging.test.mjs`): verifies every `OD-*` code in `error_codes.rs` appears in both this spec and root `llm.txt`.

## Related documentation
- [Workspace core domain, settings, and platform integration](../architecture/workspace-overview.md)
- [Native Control Center Desktop UI](../features/control-center-ui.md)
- [CLI Commands, Synchronous Control Bridge, and Auth Runners](../interfaces/cli-and-http-bridge.md)
