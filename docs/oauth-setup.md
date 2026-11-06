# OAuth & Service Account Setup (BYO Credentials)

OpenDrive supports both **Desktop App OAuth 2.0 (Browser Sign-In)** and **Service Account JSON (No-Login Mode)** in the native Control Center, as well as the CLI `authorize` command.

## Google Cloud Console OAuth Setup

1. Create a project in [Google Cloud Console](https://console.cloud.google.com/).
2. Enable **Google Drive API**.
3. OAuth consent screen → External; scopes:
   - `https://www.googleapis.com/auth/drive` (full)
   - or `https://www.googleapis.com/auth/drive.readonly`
4. **Publish → In production** (Testing expires refresh tokens ~7 days).
5. Credentials → OAuth client ID → **Desktop app**.
6. Note Client ID + Client Secret (stored in OS keyring via `SecretStore`).

### Redirect URI

OpenDrive uses loopback: `http://127.0.0.1:<ephemeral>/callback`.

For Desktop clients Google allows loopback without listing every port. If the console asks for a redirect, add:

```
http://127.0.0.1
```

or the exact port shown when running `authorize` (if your project type requires it).

## Desktop App — Browser Sign-In (with Copy Link)

1. Launch the Control Center (`npm run desktop` or `cargo run -p opendrive-app --locked`) and click **Connect Account** (or **Add First Account**).
2. Keep **Browser Sign-In** selected and enter your **OAuth Client ID** and **OAuth Client Secret** (toggle **Show**/**Hide** as needed).
   - As soon as you start **Sign in with Google** or **Copy Link**, OpenDrive saves `client_id` in `credentials.json` and `client_secret` securely in your OS keyring via `SecretStore` (`OsKeyringStore`, e.g., Windows Credential Manager DPAPI / macOS Keychain / Linux Secret Service), pre-filling both fields on subsequent launches or retries (with `OPENDRIVE_CLIENT_ID` / `OPENDRIVE_CLIENT_SECRET` env fallback).
3. Optionally enter **Account Nickname**, **Mount Folder**, and **Mount as Read-Only**.
4. Choose how to complete Google consent:
   - **Sign in with Google**: starts the ephemeral `127.0.0.1` PKCE listener and launches your default browser. If browser launch fails, OpenDrive copies the sign-in URL to your clipboard automatically.
   - **Copy Link**: starts the ephemeral PKCE listener and copies the authorization URL to your clipboard without auto-opening a browser so you can paste it into any browser or profile.
5. While waiting, the pending banner provides **Copy Link**, **Open Browser**, and **Cancel Sign-In** controls. After consent, OpenDrive exchanges the code, queries Drive `about`, saves the refresh token in the OS keyring (`OsKeyringStore`), and hydrates the root folder.

### Re-authenticating Expired / NeedsReauth Accounts

- If an account's refresh token is missing or revoked (`Needs Reauth` or `Error`), each account row displays **`Sign In`** (launches browser directly) and **`Copy Link`** (copies the PKCE authorization link to clipboard) buttons.
- When the account's `Client ID` and `Client Secret` are saved, OpenDrive starts the OAuth loopback flow immediately inline on that row and updates the existing account in place (preserving its local mount path, nickname, and SQLite cache).
- If `Client ID` or `Client Secret` is missing (for example, an account created before OS keyring persistence was enabled), clicking **`Sign In`** or **`Copy Link`** opens a pre-populated **Re-authenticate** drawer so you can enter the OAuth credentials once and reconnect in place.

## Desktop App — Service Account JSON (No-Login Mode)

1. In Google Cloud Console (**IAM & Admin → Service Accounts**), create a Service Account and download a **JSON** key file.
2. In Google Drive, share the target folders or Shared Drives with the service account's `client_email` (Viewer for read-only, Editor for read-write).
3. In the Control Center **Connect Google Drive Account** drawer, switch to **Service Account JSON (No Login)**.
4. Paste the raw JSON or type / drag-and-drop the `.json` file path into **Service Account JSON Path or Raw JSON**, optionally set Nickname / Mount Folder / Read-Only, and click **Connect Service Account**.
5. OpenDrive parses the PKCS#8 RSA key (`ServiceAccountKey`), signs an RS256 JWT locally without browser login, exchanges it for a bearer token, verifies Drive access, and stores the service account JSON in the OS keyring (`SecretStore`).

## CLI test

```bash
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- list-root <access_token>
```

Unverified-app warning is expected for personal use (<100 users).
