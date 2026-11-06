# OAuth setup (BYO credentials)

1. Create a project in [Google Cloud Console](https://console.cloud.google.com/).
2. Enable **Google Drive API**.
3. OAuth consent screen → External; add scope `.../auth/drive`.
4. **Publish → In production** (Testing expires refresh tokens ~7 days).
5. Credentials → OAuth client ID → **Desktop app**.
6. Paste Client ID + Client Secret into OpenDrive.

Unverified-app warning is expected for personal use (<100 users).

See design §7 and Google docs linked in the design document.
