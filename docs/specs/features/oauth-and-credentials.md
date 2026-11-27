---
schema_version: 1
id: "feature.oauth-credentials"
title: "OAuth 2.0 PKCE loopback authentication, Service Account JWT, and SecretStore"
kind: "feature"
status: "verified"
summary: "Google OAuth 2.0 PKCE loopback auth, Service Account JSON RS256 JWT bearer exchange, SecretStore abstraction (OsKeyringStore and MemorySecretStore), and integration tests."
tags: ["auth", "oauth2", "pkce", "loopback", "keyring", "secret-store", "service-account", "google-drive"]
source_paths: ["crates/opendrive-auth/src/keyring.rs", "crates/opendrive-auth/src/lib.rs", "crates/opendrive-auth/src/loopback.rs", "crates/opendrive-auth/src/pkce.rs", "crates/opendrive-auth/src/secret_store.rs", "crates/opendrive-auth/src/service_account.rs", "crates/opendrive-auth/src/vault.rs", "crates/opendrive-auth/tests/auth_coverage_tests.rs", "crates/opendrive-auth/tests/auth_tests.rs", "crates/opendrive-auth/tests/vault_tests.rs"]
related_specs: ["architecture.workspace-overview", "feature.drive-v3-client", "interface.cli-http-bridge", "feature.control-center-ui"]
last_doc_update_at: "2026-11-26T07:07:19Z"
last_verified_at: "2026-11-26T17:06:00Z"
last_verified_code_commit: "91d18b7a858b4edc92690c91562150ceb6622d5a"
last_source_change_commit: "600d2df0ad0c4d97420d87fa94048e4eac1b8129"
last_source_change_at: "2026-11-26T14:29:37+07:00"
source_fingerprint: "sha256:a0f1d5702c417b4f5c042fb6f0ac26ffe20c0aa7d36b298a2bcd099914f26cc5"
---

# OAuth 2.0 PKCE loopback authentication, Service Account JWT, and SecretStore

## Purpose and boundaries
`opendrive-auth` handles Google OAuth 2.0 user authorization, Google Service Account JSON (no-login) JWT RS256 bearer exchange, token exchange/refresh, and secret persistence (`refresh_token`, `client_secret`, and `service_account_json`) behind a testable `SecretStore` trait.

In scope: PKCE S256 pair and random state generation, ephemeral loopback HTTP redirect capture, Google OAuth v2 authorization URL construction, `authorization_code` / `refresh_token` / `jwt-bearer` HTTP POST token exchanges, Service Account PKCS#8 PEM parsing & RS256 JWT signing (`ring`), `SecretStore` (`OsKeyringStore`, `MemorySecretStore`), and `keyring` wrappers.
Out of scope: persisting public `Account` or `CredentialSet` JSON metadata (`opendrive-core`) and making Drive API calls (`opendrive-drive`).

## Observable behavior

### PKCE and CSRF state (`pkce.rs`)
- `generate_pkce()` reads 32 random bytes from `rand::thread_rng()`, encodes them as base64url without padding (`verifier`), hashes `verifier` with SHA-256, and base64url-encodes the digest (`challenge`).
- `random_state()` generates 16 random bytes encoded as base64url without padding.

### Loopback redirect listener (`loopback.rs`)
- `bind_loopback()` binds `127.0.0.1:0` and returns `(TcpListener, redirect_uri, SocketAddr)` where `redirect_uri` is `http://127.0.0.1:{port}/callback`.
- `wait_for_redirect(listener, expected_state, timeout_secs)` loops within `timeout_secs` (returning `LoopbackError::Timeout` on expiry), ignores empty TCP preconnects and responds `204 No Content` to non-callback requests (e.g. `/favicon.ico`) before handling `/callback` (`code`, `state`, `error`).
- Query decoding converts `+` to space and `%XX` hex bytes via `u8::from_str_radix` (leaving invalid `%` sequences literal).
- It sends an `HTTP/1.1 200 OK` HTML response via `render_callback_html(err_detail)` rendered in the centered Light Neo-Brutalist palette (`#d6d4f0` dot-matrix backdrop, `#f8fafc` card, `#1c1e24` ink border, enlarged `76x76` / `42x42` checkmark or warning icon, without any redundant status pill) only marking success when `code` is present, `state == expected_state`, and `error` is absent (otherwise rendering distinct safe messages for denial/cancellation, missing authorization code, or state mismatch), then flushes/shuts down cleanly before returning `LoopbackError::BadRequest` / `LoopbackError::Io` on failures.

### Authorization and token exchange (`lib.rs`)
- `AuthClient::new(client_id, client_secret)` targets Google endpoints `AUTH_URL` (`https://accounts.google.com/o/oauth2/v2/auth`) and `TOKEN_URL` (`https://oauth2.googleapis.com/token`); `AuthClient::with_endpoints` allows custom test/mock URLs.
- `authorization_url(redirect_uri, challenge, state, read_only)` selects scope `https://www.googleapis.com/auth/drive.readonly` when `read_only` is true or `https://www.googleapis.com/auth/drive` otherwise, with `response_type=code`, `access_type=offline`, `prompt=consent`, `code_challenge_method=S256`.
- `authorize(read_only)` binds loopback, opens the authorization URL in the system browser via `open::that`, waits up to 180 seconds for the redirect, and exchanges the code for `(access_token, refresh_token)`.
- `exchange_code(code, verifier, redirect_uri)` POSTs form parameters (`grant_type=authorization_code`) to `token_url` and requires both `access_token` and `refresh_token` in the JSON response (`expires_in` is parsed but discarded).
- `refresh(refresh_token)` POSTs `grant_type=refresh_token` and returns `access_token`.
- `post_token_form` maps JSON `"error": "invalid_grant"` to `AuthError::InvalidGrant`, and any other JSON `error` (including optional `error_description`) or HTTP/JSON parse error to `AuthError::OAuth(String)`. `is_credential_rejection_error(msg)` classifies `invalid_client` and `unauthorized_client` errors case-insensitively.

### Service Account JSON & RS256 JWT (`service_account.rs`)
- `ServiceAccountKey` deserializes `type`, `client_email`, `private_key`, `token_uri` (defaulting to `DEFAULT_TOKEN_URI` `"https://oauth2.googleapis.com/token"` when absent or empty), and optional `project_id`.
- `parse_pkcs8_pem(pem)` strips PEM header/footer lines and whitespace, decodes standard base64, and validates the DER payload with `ring::signature::RsaKeyPair::from_pkcs8`.
- `ServiceAccountKey::from_json_str(raw)` parses the JSON, requires `client_email` to contain `'@'`, defaults empty `token_uri`, and validates `private_key` via `parse_pkcs8_pem`.
- `ServiceAccountKey::from_json_or_path(input)` accepts raw JSON when trimmed input starts with `'{'`, or treats trimmed input (stripping surrounding `"` or `'`) as a file path, reading and parsing it into `(ServiceAccountKey, raw_json)`.
- `ServiceAccountKey::sign_jwt(read_only, now_epoch_secs)` constructs header `{"alg":"RS256","typ":"JWT"}` and claims (`iss`, `scope` via `scope_for_access`, `aud`, `iat`, `exp = now_epoch_secs + 3600`), signing with `RSA_PKCS1_SHA256` and `SystemRandom`.
- `ServiceAccountKey::fetch_access_token(read_only)` signs a JWT at current `SystemTime` and POSTs `grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer` with `assertion=<jwt>` to `self.token_uri` via `post_token_form`, returning `access_token`.

### SecretStore & OS keyring storage (`secret_store.rs`, `keyring.rs`)
- `SecretStore: Send + Sync` defines `store_refresh_token`, `load_refresh_token`, `delete_refresh_token`, `store_client_secret`, `load_client_secret`, `store_service_account_json`, `load_service_account_json`, `delete_service_account_json`, and default `delete_account_secrets(account_id)` (best-effort deletes both refresh token and service account JSON).
- `MemorySecretStore` provides an in-memory `Mutex<HashMap<String, String>>` implementation for deterministic tests.
- `OsKeyringStore` implements `SecretStore` using keyring service `"opendrive"` (plus an `#[ignore]` live OS keyring roundtrip/cleanup unit test in `keyring.rs`):
  - Refresh tokens: `"acct/{account_id}/refresh_token"` (`store_refresh_token`, `load_refresh_token`, `delete_refresh_token` via `set_password`/`get_password`).
  - OAuth client secrets: `"cred/{cred_id}/secret"` (`store_client_secret`, `load_client_secret` via `set_password`/`get_password`).
  - Service Account JSON: `"acct/{account_id}/service_account_json"` (`store_service_account_json` compacts valid JSON via `serde_json` and stores UTF-8 bytes with `Entry::set_secret` so typical 2048-bit keys stay within Windows Credential Manager's 2560-byte `CRED_MAX_CREDENTIAL_BLOB_SIZE` limit instead of UTF-16LE doubling via `set_password`; `load_service_account_json` reads `Entry::get_secret` with `get_password` fallback).
- `keyring::Error::NoEntry` maps to `Ok(None)` on load and `Ok(())` on delete; other keyring failures map to `AuthError::Keychain(String)`.

### Password-Encrypted Cross-Machine Credential Vault (`vault.rs`)
- `validate_vault_password(password)` requires `password.chars().count() > 8` (`9+` chars) with at least one uppercase letter, one lowercase letter, one ASCII digit, and one non-whitespace symbol.
- `encrypt_vault(payload, password)` derives a 256-bit key via `PBKDF2-HMAC-SHA256` (`600,000` rounds, random `16`-byte salt) and seals JSON `VaultPayload` (`ExportedCredentialSet` + `ExportedAccount` including secrets) via `AES-256-GCM` (`12`-byte CSPRNG nonce, AAD `b"opendrive-vault-v1"`), emitting pretty JSON `EncryptedVaultFile`.
- `decrypt_vault(bytes, password)` validates the envelope version/KDF/cipher metadata, derives the key, opens the AEAD ciphertext, and maps wrong password/tampering/parse failures to `AuthError::Vault(String)`.

## Workflow
1. **Browser OAuth PKCE**: Caller generates PKCE + state (`generate_pkce`, `random_state`), binds `bind_loopback()`, builds `authorization_url`, opens browser or copies URL, waits via `wait_for_redirect(..., 180)`, calls `exchange_code`, and stores secrets via `SecretStore`.
2. **Service Account (No Login)**: Caller parses raw JSON or `.json` file via `ServiceAccountKey::from_json_or_path`, calls `fetch_access_token(read_only)` (which signs an RS256 JWT locally and exchanges it at `token_uri`), and persists raw JSON via `SecretStore::store_service_account_json`.

## Code map
- `crates/opendrive-auth/src/lib.rs` — Re-exports, `AuthError`, `AuthClient`, `scope_for_access`, `post_token_form`, `is_credential_rejection_error`, `authorization_url`, `authorize`, `exchange_code`, `refresh`.
- `crates/opendrive-auth/src/pkce.rs` — `PkceError`, `PkcePair`, `generate_pkce`, `random_state`, and 1 unit test.
- `crates/opendrive-auth/src/loopback.rs` — `LoopbackError`, `RedirectParams`, `bind_loopback`, `wait_for_redirect`, `render_callback_html`, `urlencoding_decode`, and 5 unit tests.
- `crates/opendrive-auth/src/secret_store.rs` — `SecretStore` trait (`delete_account_secrets`) and `MemorySecretStore`.
- `crates/opendrive-auth/src/keyring.rs` — `OsKeyringStore` and `"opendrive"` keyring wrappers (`store_refresh_token`, `load_refresh_token`, `delete_refresh_token`, `store_client_secret`, `load_client_secret`, `store_service_account_json`, `load_service_account_json`, `delete_service_account_json`).
- `crates/opendrive-auth/src/service_account.rs` — `DEFAULT_TOKEN_URI`, `parse_pkcs8_pem`, `ServiceAccountKey` (`from_json_str`, `from_json_or_path`, `sign_jwt`, `fetch_access_token`).
- `crates/opendrive-auth/src/vault.rs` — `validate_vault_password`, `VaultPayload`, `ExportedCredentialSet`, `ExportedAccount`, `EncryptedVaultFile`, `encrypt_vault`, `decrypt_vault`.
- `crates/opendrive-auth/tests/{auth_coverage_tests,auth_tests,vault_tests}.rs` — Integration tests covering OAuth URLs, PKCE, loopback redirect edge cases, mock token errors, `MemorySecretStore`, `ServiceAccountKey` error/token paths, and `.odcred` vault encryption/validation.

## Verification
Verified in `opendrive-auth` (`cargo test -p opendrive-auth --locked`):
- `crates/opendrive-auth/tests/{auth_tests,auth_coverage_tests,vault_tests}.rs` and unit tests in `crates/opendrive-auth/src/{lib,loopback,pkce,service_account}.rs` pass.
- Gap: live OS keyring daemon round-trips in `OsKeyringStore` (`keyring.rs`) and real Google OAuth/token cloud endpoints are not exercised by unit tests.
- Note on `AuthError::StateMismatch`: `AuthError` defines a `StateMismatch` variant, while `wait_for_redirect` currently returns `LoopbackError::BadRequest` on state mismatch (propagated as `AuthError::Loopback(LoopbackError::BadRequest)`).

## Related documentation
- [Workspace overview](../architecture/workspace-overview.md)
- [Google Drive v3 client](../features/drive-v3-client.md)
- [CLI and synchronous control bridge](../interfaces/cli-and-http-bridge.md)
- [Control Center UI](../features/control-center-ui.md)
