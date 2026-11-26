---
schema_version: 1
id: "architecture.workspace-overview"
title: "Workspace core domain, settings, and platform integration"
kind: "architecture"
status: "verified"
summary: "UI-agnostic domain models, Open-With media classification/rules, JSON config persistence, command/event controller, and OS platform services (paths, instance lock, tray menu model, app launch, Windows virtual drive & This PC namespace quota formatting)."
tags: ["architecture", "core", "accounts", "settings", "open-rules", "controller", "platform", "tray", "virtual-drive", "this-pc"]
source_paths: ["crates/opendrive-core/src/accounts_io.rs", "crates/opendrive-core/src/controller.rs", "crates/opendrive-core/src/credentials_io.rs", "crates/opendrive-core/src/lib.rs", "crates/opendrive-core/src/open_rules.rs", "crates/opendrive-core/src/settings_io.rs", "crates/opendrive-core/tests/core_coverage_tests.rs", "crates/opendrive-core/tests/core_tests.rs", "crates/opendrive-core/tests/open_rules_tests.rs", "crates/opendrive-platform/src/drive_mount.rs", "crates/opendrive-platform/src/explorer_quota.rs", "crates/opendrive-platform/src/lib.rs", "crates/opendrive-platform/src/mount_cleanup.rs", "crates/opendrive-platform/src/tray.rs", "crates/opendrive-platform/tests/explorer_quota_tests.rs", "crates/opendrive-platform/tests/platform_coverage_tests.rs"]
related_specs: ["feature.oauth-credentials", "architecture.sqlite-store", "workflow.sync-hydration-conflicts", "interface.cli-http-bridge", "feature.control-center-ui"]
last_doc_update_at: "2026-11-26T07:07:19Z"
last_verified_at: "2026-11-26T07:07:19Z"
last_verified_code_commit: "aa2135c5d17ce0016d4fa42bb2c2705bbba7c709"
last_source_change_commit: "600d2df0ad0c4d97420d87fa94048e4eac1b8129"
last_source_change_at: "2026-11-26T14:29:37+07:00"
source_fingerprint: "sha256:dede90ef23c1072d04bdd0b5c76b00e314d4ff16470f6fe22a64f2f767a849f4"
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
- `Settings` configures `root_path`, `cache_dir`, `cache_max_size` (default `10 GiB`), `bandwidth_limit: Option<u64>` (default `None`), `autostart` (`false`), `check_updates` (`false`), `docs_mode: DocsMode` (`Link` default, or `ExportOffice`), `virtual_drive_enabled: bool` (`#[serde(default)]`, default `false`), `virtual_drive_letter: String` (`#[serde(default = "default_virtual_drive_letter")]`, default `"O"`), `ignored_release_version: Option<String>`, `open_with_rules: Vec<OpenWithRule>` (`#[serde(default)]`, default empty), and `explorer_folder_cache: bool` (`#[serde(default = "default_explorer_folder_cache")]`, default `true`). Legacy `settings.json` files omitting the newer fields deserialize cleanly with defaults.
- `open_rules.rs` defines `MediaKind` (`Folder`, `Image`, `Video`, `Pdf`, `Audio`, `Archive`, `CodeText`, `NativeDoc`, `Other`), `OpenWithRule { pattern, app_command }`, `classify_media_kind(name, mime)`, `resolve_open_app(rules, name, mime)`, `normalize_rule_tokens(pattern)` / `validate_open_with_rule(pattern, app_command, existing, ignore_index)` (canonicalizing tokens, checking balanced quotes, and preventing overlapping extension/group assignments across apps), and `supports_http_stream_arg(app_command)`.
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
- `RemoveAccount { id }` removes the account and emits `AppEvent::Ready`, or `AppEvent::Error` if not found; `MoveAccount { id, delta }` reorders the account list in-place (clamped to bounds) and emits `AppEvent::Ready`.
- `UpdateSettings(settings)` replaces `controller.settings` and emits `AppEvent::Ready`.
- `OpenFolder` and `Quit` return `[AppEvent::Ready]` in the core controller; side effects are performed by outer host layers.

### Platform services (`opendrive-platform`)
- Directory resolution (`dirs` crate): `config_dir()` (`<config>/opendrive`), `cache_dir()` (`<cache>/opendrive`), `data_dir()` (`<data>/opendrive`), `mount_root()` (`<home>/OpenDrive`), and `ensure_dirs()` (creates config, cache, and data directories).
- Single-instance lock: `acquire_instance_lock(lock_path)` creates parent directories, opens `lock_path` with `create_new(true)`, and writes the current PID; fails with `PlatformError::Other` if the lock file already exists. `ensure_single_instance()` locks `<data_dir>/opendrive.lock`.
- `autostart_command(exe_path, enabled)` formats a Windows `reg add` / `reg delete` command under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenDrive` on Windows, or `enable-autostart:{exe}` / `disable-autostart:OpenDrive` on non-Windows. `set_autostart(enabled)` currently computes the command for `current_exe()` and logs it via `tracing::info!` without executing OS registration commands.
- `notify(title, body)` currently logs via `tracing::info!` without invoking OS desktop notification APIs.
- `open_path(path)` rejects empty paths before delegating to `open::that(path)`; `open_drive_file(file_id)` opens `https://drive.google.com/open?id={file_id}`. `launch_with_app(app_command, target)` and `build_launch_invocation` (in `explorer_quota.rs`) parse quoted executable + arguments and fall back to `open_path` when no custom app is provided.
- `lib.rs` defines `TrayAction` (`Summary`, `ShowWindow`, `OpenFolder`, `PauseAll`, `ResumeAll`, `Quit`); `tray.rs` defines `TrayMenuItem`, `default_menu()` (4 enabled items), `desktop_tray_menu()` (6 items: disabled `Summary` `"Standby · 0 B used"`, enabled `Show OpenDrive`, plus `default_menu()`), and `show_tray_stub()` (logs and returns `default_menu()` without creating a native tray icon).
- `explorer_quota.rs` provides `ExplorerQuota { used, total, free }`, `compute_explorer_quota(used_and_quotas)` (returning `None` when empty or when any account has `quota == 0 || quota < used`), `format_bytes_compact`, `format_explorer_quota_label` (`"OpenDrive"` when `None` vs `"OpenDrive (used / total)"`), `format_explorer_tile_comment`, `build_this_pc_reg_commands` (removing `MyComputer\NameSpace`, `Desktop\NameSpace`, and `CLSID` keys on disable), and `sync_this_pc_namespace(enabled, target, icon_ico, quota)` (registering/removing the Windows Explorer namespace folder shortcut and flushing Shell PIDL removal notifications so open Explorer windows update immediately without restarting `explorer.exe`).
- `drive_mount.rs` implements Windows virtual drive letter helpers & safe cleanup: `is_virtual_drive_supported()` (`cfg!(windows)`), `normalize_drive_letter(raw)` (accepts `"o"`, `"O:\\"`, `"D:/"` and rejects reserved `A`/`B`/`C` or multi-character/non-ASCII inputs), `drive_mount_command` / `drive_unmount_command` (`subst`), `drive_label_reg_command` / `drive_icon_reg_command` (`DriveIcons` registry helpers), `parse_subst_target(stdout, letter)` (case-insensitive `X:\: => <target>` parser), and `mount_virtual_drive` / `unmount_virtual_drive` / `unmount_virtual_drive_for_target` (applying `DriveIcons` label/icon, calling `notify_shell_mounts_changed()` on Windows, unmounting `letter:` only when `subst` maps it to `target`, and cleaning `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\<letter>` via hidden `reg`; on non-Windows, `unmount_virtual_drive_for_target` validates `normalize_drive_letter` before returning `PlatformError::Other`).
- `mount_cleanup.rs` provides `notify_shell_mounts_changed` (flushed Windows `SHChangeNotify` on `CSIDL_DRIVES`, `CSIDL_DESKTOP`, `SHCNE_ASSOCCHANGED`, and `WM_SETTINGCHANGE`), `remove_namespace_with_shell_notify` (`SHParseDisplayName` + `SHCNE_RMDIR` / `SHCNE_DELETE` before/after key deletion), `spawn_detached_background` (using valid `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP` with `CREATE_BREAKAWAY_FROM_JOB` fallback instead of mutually exclusive `DETACHED_PROCESS`), `install_console_cleanup_handler` (`SetConsoleCtrlHandler` for `Ctrl+C`/close/shutdown), `is_process_alive(pid)` (cross-platform liveness probe without `/proc`), and `wait_for_process_exit(pid)` for OS Explorer shell refresh and detached process-kill cleanup.

## Code map
- `crates/opendrive-core/src/lib.rs` — `CredentialSet`, `AccountStatus`, `Account`, `Settings` (including `virtual_drive_enabled`, `virtual_drive_letter`, `open_with_rules`, `explorer_folder_cache`), `DocsMode`, `CoreError`, `AccountManager`, and re-exports from `open_rules`.
- `crates/opendrive-core/src/open_rules.rs` — `MediaKind`, `OpenWithRule`, `classify_media_kind`, `resolve_open_app`, `normalize_rule_tokens`, `validate_open_with_rule`, and `supports_http_stream_arg`.
- `crates/opendrive-core/src/controller.rs` — `AppCommand`, `AppEvent` (including `SyncProgress { transfers, queue }`), `AppController::handle`.
- `crates/opendrive-core/src/accounts_io.rs` — `accounts_path`, `load_accounts`, `save_accounts`.
- `crates/opendrive-core/src/credentials_io.rs` — `credentials_path`, `load_credentials`, `save_credentials`.
- `crates/opendrive-core/src/settings_io.rs` — `settings_path`, `load_settings`, `save_settings`.
- `crates/opendrive-core/tests/core_tests.rs` — 5 integration tests for `AccountManager`, JSON persistence error cases, backwards-compatible `Settings` deserialization, `RenameAccount`, and `AppController`.
- `crates/opendrive-core/tests/core_coverage_tests.rs` — Integration tests for directory-blocker I/O errors and domain model edge cases.
- `crates/opendrive-core/tests/open_rules_tests.rs` — 5 integration tests covering `classify_media_kind`, `resolve_open_app`, `validate_open_with_rule` / `normalize_rule_tokens`, `supports_http_stream_arg`, and `Settings::open_with_rules` serialization.
- `crates/opendrive-platform/src/lib.rs` — `PlatformError`, directory helpers, instance lock, autostart command builder, `notify`, `open_path`, `open_drive_file`, `TrayAction`, re-exports from `drive_mount`, `explorer_quota` (including `format_bytes_compact`), and `mount_cleanup`, and 5 unit tests.
- `crates/opendrive-platform/src/drive_mount.rs` — `is_virtual_drive_supported`, `normalize_drive_letter`, `drive_mount_command`, `drive_unmount_command`, `drive_label_reg_command`, `drive_icon_reg_command`, `parse_subst_target`, `mount_virtual_drive`, `unmount_virtual_drive`, `unmount_virtual_drive_for_target`, and unit test.
- `crates/opendrive-platform/src/explorer_quota.rs` — `ExplorerQuota`, `compute_explorer_quota`, `format_bytes_compact`, `format_explorer_quota_label`, `format_explorer_tile_comment`, `build_this_pc_reg_commands`, `sync_this_pc_namespace`, `build_launch_invocation`, `launch_with_app`, and unit test.
- `crates/opendrive-platform/src/mount_cleanup.rs` — `notify_shell_mounts_changed`, `remove_namespace_with_shell_notify`, `spawn_detached_background`, `install_console_cleanup_handler`, `is_process_alive`, `wait_for_process_exit`, and unit test.
- `crates/opendrive-platform/src/tray.rs` — `TrayMenuItem`, `default_menu`, `desktop_tray_menu`, `show_tray_stub`, and 1 unit test.
- `crates/opendrive-platform/tests/explorer_quota_tests.rs` — 5 integration tests verifying quota computation/hiding, `This PC`/`Desktop` registry command generation, virtual drive error paths, app invocation builders, detached background spawning, and console cleanup registration.
- `crates/opendrive-platform/tests/platform_coverage_tests.rs` — Integration tests for isolated instance locking, drive letter validation, `TrayAction`, and process exit waiting.

## Verification
- Verified in `opendrive-core` and `opendrive-platform` (`cargo test -p opendrive-core -p opendrive-platform --locked`):
  - `crates/opendrive-core/tests/{core_tests,core_coverage_tests,open_rules_tests}.rs` pass.
  - `crates/opendrive-platform/src/{lib,drive_mount,mount_cleanup,tray,explorer_quota}.rs` unit tests and `crates/opendrive-platform/tests/{explorer_quota_tests,platform_coverage_tests}.rs` pass.
- Gap: `set_autostart` and `notify` are log-only stubs in `opendrive-platform`, and `show_tray_stub` does not create a native tray icon (live tray event loop resides in `opendrive-app`).

## Related documentation
- [OAuth and credentials](../features/oauth-and-credentials.md)
- [SQLite store and cache](../architecture/sqlite-store-and-cache.md)
- [Sync, hydration, and conflicts](../workflows/sync-hydration-and-conflicts.md)
- [CLI and HTTP bridge](../interfaces/cli-and-http-bridge.md)
- [Control Center UI](../features/control-center-ui.md)
