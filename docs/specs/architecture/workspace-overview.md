---
schema_version: 1
id: "architecture.workspace-overview"
title: "Workspace core domain, settings, and platform integration"
kind: "architecture"
status: "verified"
summary: "UI-agnostic domain models, JSON config persistence, command/event controller, and OS platform services (paths, instance lock, tray menu model, browser/file open, Windows virtual drive mounting)."
tags: ["architecture", "core", "accounts", "settings", "controller", "platform", "tray", "virtual-drive"]
source_paths: ["crates/opendrive-core/src/accounts_io.rs", "crates/opendrive-core/src/controller.rs", "crates/opendrive-core/src/credentials_io.rs", "crates/opendrive-core/src/lib.rs", "crates/opendrive-core/src/settings_io.rs", "crates/opendrive-core/tests/core_tests.rs", "crates/opendrive-platform/src/drive_mount.rs", "crates/opendrive-platform/src/lib.rs", "crates/opendrive-platform/src/tray.rs"]
related_specs: ["feature.oauth-credentials", "architecture.sqlite-store", "workflow.sync-hydration-conflicts", "interface.cli-http-bridge", "feature.control-center-ui"]
last_doc_update_at: "2026-11-19T05:16:00Z"
last_verified_at: "2026-11-19T05:16:00Z"
last_verified_code_commit: "7db9e71958cd8923af5338a9b12f7117abd5caa4"
last_source_change_commit: "7db9e71958cd8923af5338a9b12f7117abd5caa4"
last_source_change_at: "2026-11-19T05:16:00Z"
source_fingerprint: "sha256:05d957d17a642a3cc0c7a994be983e038f89fba27cd1aa1cabcca296333e4436"
---

# Workspace core domain, settings, and platform integration

## Purpose and boundaries
`opendrive-core` and `opendrive-platform` provide the shared, UI-agnostic foundation of the OpenDrive workspace:
- `opendrive-core` defines account, OAuth credential-set, and application settings models, JSON file persistence under a configuration directory, and the synchronous `AppController` state machine (`AppCommand` -> `Vec<AppEvent>`).
- `opendrive-platform` resolves OS-standard application directories, enforces single-instance locking, defines the tray menu model, and handles browser/path opening.

Out of scope for these two crates: OAuth token exchange and OS keyring storage (`opendrive-auth`), Drive v3 HTTP calls (`opendrive-drive`), SQLite metadata/cache/queue storage (`opendrive-store`), sync execution (`opendrive-sync`), and HTTP/UI presentation (`opendrive-app`).

## Observable behavior

### Domain models and defaults (`opendrive-core`)
- `default_account_alias(nickname, email)` resolves non-empty trimmed `nickname` or defaults to the local part before `@` in `email` (`"Drive"` fallback); `sanitize_folder_segment(raw)` strips invalid path separators / traversal dots cleanly.
- `CredentialSet` (`id: Uuid`, `label: String`, `client_id: String`) stores public OAuth client metadata; `client_secret` is excluded and delegated to `opendrive-auth`.
- `Account` (`id: Uuid`, `nickname: String`, `email: String`, `credential_set_id: Uuid`, `folder_name: String`, `read_only: bool`, `enabled: bool`, `status: AccountStatus`, `storage_used: u64`, `storage_quota: u64`, `avatar_url: Option<String>`) tracks account configuration, status (`Connected`, `Paused`, `NeedsReauth`, `Error`), cached Google Drive storage metrics (`#[serde(default)]` `0` for legacy JSON), and profile photo URL (`#[serde(default)]` `None`). Refresh tokens are excluded from JSON and delegated to `opendrive-auth`.
- `Settings` configures `root_path`, `cache_dir`, `cache_max_size` (default `10 GiB`), `bandwidth_limit: Option<u64>` (default `None`), `autostart` (`false`), `check_updates` (`false`), `docs_mode: DocsMode` (`Link` default, or `ExportOffice`), `virtual_drive_enabled: bool` (`#[serde(default)]`, default `false`), and `virtual_drive_letter: String` (`#[serde(default = "default_virtual_drive_letter")]`, default `"O"`). Legacy `settings.json` files omitting the virtual drive fields deserialize cleanly with those defaults.
- `AccountManager` manages in-memory lists of `Account` and `CredentialSet` with lookup/mutation methods returning `CoreError::AccountNotFound(Uuid)` or `CoreError::CredentialNotFound(Uuid)` when absent.

### JSON configuration persistence (`opendrive-core`)
- `accounts_io`: reads/writes `<config_dir>/accounts.json`. Missing file yields `Ok(Vec::new())`.
- `credentials_io`: reads/writes `<config_dir>/credentials.json`. Missing file yields `Ok(Vec::new())`.
- `settings_io`: reads/writes `<config_dir>/settings.json`. Missing file yields `Ok(Settings::default())`.
- All `save_*` helpers create `config_dir` via `fs::create_dir_all` and write pretty-printed JSON, mapping I/O and serialization errors to `CoreError::Other(String)`.

### Controller command handling (`AppController`)
- `AppCommand::AddAccount { nickname }` creates a new `Account` with `Uuid::new_v4()`, `credential_set_id: Uuid::nil()`, `folder_name: nickname`, `read_only: false`, `enabled: true`, `status: AccountStatus::NeedsReauth`, and emits `AppEvent::AccountUpdated`.
- `AppCommand::RenameAccount { id, nickname, folder_name }` updates `nickname` via `default_account_alias(&nickname, &a.email)` and optionally updates `folder_name` when `sanitize_folder_segment` yields a valid folder name, emitting `AppEvent::AccountUpdated`.
- `PauseAccount { id }` sets `status = AccountStatus::Paused` and `enabled = false`; `ResumeAccount { id }` sets `status = AccountStatus::Connected` and `enabled = true`; `SetReadOnly { id, read_only }` updates `read_only`. Unknown IDs emit `AppEvent::Error("account not found")`.
- `PauseAll` and `ResumeAll` fan out `PauseAccount` / `ResumeAccount` across all accounts in order and concatenate the resulting events.
- `RemoveAccount { id }` removes the account and emits `AppEvent::Ready`, or `AppEvent::Error` if not found.
- `UpdateSettings(settings)` replaces `controller.settings` and emits `AppEvent::Ready`.
- `OpenFolder` and `Quit` return `[AppEvent::Ready]` in the core controller; side effects are performed by outer host layers.

### Platform services (`opendrive-platform`)
- Directory resolution (`dirs` crate): `config_dir()` (`<config>/opendrive`), `cache_dir()` (`<cache>/opendrive`), `data_dir()` (`<data>/opendrive`), `mount_root()` (`<home>/OpenDrive`), and `ensure_dirs()` (creates config, cache, and data directories).
- Single-instance lock: `acquire_instance_lock(lock_path)` creates parent directories, opens `lock_path` with `create_new(true)`, and writes the current PID; fails with `PlatformError::Other` if the lock file already exists. `ensure_single_instance()` locks `<data_dir>/opendrive.lock`.
- `autostart_command(exe_path, enabled)` formats a Windows `reg add` / `reg delete` command under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenDrive` on Windows, or `enable-autostart:{exe}` / `disable-autostart:OpenDrive` on non-Windows. `set_autostart(enabled)` currently computes the command for `current_exe()` and logs it via `tracing::info!` without executing OS registration commands.
- `notify(title, body)` currently logs via `tracing::info!` without invoking OS desktop notification APIs.
- `open_path(path)` delegates to `open::that(path)`; `open_drive_file(file_id)` opens `https://drive.google.com/open?id={file_id}`.
- `tray.rs` defines `TrayAction` (`ShowWindow`, `OpenFolder`, `PauseAll`, `ResumeAll`, `Quit`), `TrayMenuItem`, `default_menu()` (4 enabled items), `desktop_tray_menu()` (5 enabled items adding `Show OpenDrive`), and `show_tray_stub()` (logs and returns `default_menu()` without creating a native tray icon).
- `drive_mount.rs` implements Windows virtual drive letter mounting: `is_virtual_drive_supported()` (`cfg!(windows)`), `normalize_drive_letter(raw)` (accepts `"o"`, `"O:\\"`, `"D:/"` and rejects reserved `A`/`B`/`C` or multi-character/non-ASCII inputs), `drive_mount_command` / `drive_unmount_command` (`subst`), `parse_subst_target(stdout, letter)` (case-insensitive `X:\: => <target>` parser), and `mount_virtual_drive` / `unmount_virtual_drive` (on Windows, executes hidden `subst` with `CREATE_NO_WINDOW` and sets/deletes `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\<letter>\DefaultLabel` = `"OpenDrive"` via hidden `reg`; on non-Windows, returns `PlatformError::Other`).

## Code map
- `crates/opendrive-core/src/lib.rs` — `CredentialSet`, `AccountStatus`, `Account`, `Settings` (including `virtual_drive_enabled` and `virtual_drive_letter`), `DocsMode`, `CoreError`, `AccountManager`.
- `crates/opendrive-core/src/controller.rs` — `AppCommand`, `AppEvent`, `AppController::handle`.
- `crates/opendrive-core/src/accounts_io.rs` — `accounts_path`, `load_accounts`, `save_accounts`.
- `crates/opendrive-core/src/credentials_io.rs` — `credentials_path`, `load_credentials`, `save_credentials`.
- `crates/opendrive-core/src/settings_io.rs` — `settings_path`, `load_settings`, `save_settings`.
- `crates/opendrive-core/tests/core_tests.rs` — integration tests for `AccountManager`, JSON persistence error cases, backwards-compatible `Settings` deserialization, and `AppController`.
- `crates/opendrive-platform/src/lib.rs` — `PlatformError`, directory helpers, instance lock, autostart command builder, `notify`, `open_path`, `open_drive_file`, `TrayAction`, re-exports from `drive_mount`, and 4 unit tests.
- `crates/opendrive-platform/src/drive_mount.rs` — `is_virtual_drive_supported`, `normalize_drive_letter`, `drive_mount_command`, `drive_unmount_command`, `parse_subst_target`, `mount_virtual_drive`, `unmount_virtual_drive`, and 2 unit tests.
- `crates/opendrive-platform/src/tray.rs` — `TrayMenuItem`, `default_menu`, `show_tray_stub`, and 1 unit test.

## Verification
- Existing tests observed in source (not executed during this documentation pass):
  - `crates/opendrive-core/tests/core_tests.rs`: 4 integration tests (`account_manager_and_io_roundtrip`, `io_errors_on_corrupt_json_and_file_as_dir`, `app_controller_handles_all_commands`, `settings_backwards_compat_defaults_virtual_drive_fields`).
  - `crates/opendrive-platform/src/lib.rs`: 4 unit tests (`standard_dirs_resolve`, `instance_lock_creates_and_blocks_duplicate`, `autostart_command_and_notify`, `instance_lock_fails_when_parent_is_a_file`).
  - `crates/opendrive-platform/src/drive_mount.rs`: 2 unit tests (`normalize_drive_letter_accepts_valid_and_rejects_invalid`, `drive_commands_and_subst_parsing`).
  - `crates/opendrive-platform/src/tray.rs`: 1 unit test (`tray_menu_and_stub_have_expected_items`).
- Gap: `set_autostart` and `notify` are log-only stubs in `opendrive-platform`, and `show_tray_stub` does not create a native tray icon (live tray event loop resides in `opendrive-app`).

## Related documentation
- [OAuth and credentials](../features/oauth-and-credentials.md)
- [SQLite store and cache](../architecture/sqlite-store-and-cache.md)
- [Sync, hydration, and conflicts](../workflows/sync-hydration-and-conflicts.md)
- [CLI and HTTP bridge](../interfaces/cli-and-http-bridge.md)
- [Control Center UI](../features/control-center-ui.md)
