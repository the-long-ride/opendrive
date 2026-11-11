# OAuth setup (BYO credentials)

1. Create a project in [Google Cloud Console](https://console.cloud.google.com/).
2. Enable **Google Drive API**.
3. OAuth consent screen → External; scopes:
   - `https://www.googleapis.com/auth/drive` (full)
   - or `https://www.googleapis.com/auth/drive.readonly`
4. **Publish → In production** (Testing expires refresh tokens ~7 days).
5. Credentials → OAuth client ID → **Desktop app**.
6. Note Client ID + Client Secret (secret stored in OS keychain by OpenDrive).

## Redirect URI

OpenDrive uses loopback: `http://127.0.0.1:<ephemeral>/callback`.

For Desktop clients Google allows loopback without listing every port. If the console asks for a redirect, add:

```
http://127.0.0.1
```

or the exact port shown when running `authorize` (if your project type requires it).

## CLI test

```bash
cargo run -p opendrive-app -- authorize <client_id> <client_secret>
cargo run -p opendrive-app -- list-root <access_token>
```

Unverified-app warning is expected for personal use (<100 users).
