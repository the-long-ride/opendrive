# Connect Google Drive Account Guide

OpenDrive uses your own free **Google Cloud** project credentials (BYO) so no third-party server ever sees your OAuth tokens or Google Drive files.

---

## Option A — Browser Sign-In (OAuth 2.0 Desktop App)

Recommended for personal Gmail and Google Workspace Drive accounts.

1. **Create or select a Google Cloud project**
   - Open [`console.cloud.google.com`](https://console.cloud.google.com/) and sign in.
   - Click the **project picker** in the top-left bar → **New Project** → name it (e.g. `OpenDrive`) → click **Create**, and make sure it is selected.

2. **Enable the Google Drive API**
   - Open **☰ → APIs & Services → Library**.
   - Search for **`Google Drive API`**, open it, and click **Enable**.

3. **Configure Google Auth platform / OAuth consent screen**
   - Open **Google Auth platform** (or **APIs & Services → OAuth consent screen**) and click **Get Started**.
   - Set **App name** (`OpenDrive`), **User support email**, **Audience: External**, and **Developer contact email**.

4. **Audience & Drive Scope (Important)**
   - Under **Audience**:
     - Add your Gmail address under **Test users** (refresh tokens expire after **7 days**), **OR**
     - Click **Publish app → Confirm** (recommended for personal use `<100` users — avoids 7-day token expiry).
   - Under **Data Access → Add or remove scopes**:
     - Add `https://www.googleapis.com/auth/drive` (full Read-Write sync) or `https://www.googleapis.com/auth/drive.readonly` (Read-Only) and click **Save**.

5. **Create a Desktop App OAuth Client**
   - Go to **Clients → + Create Client** (or **Credentials → + Create Credentials → OAuth client ID**).
   - Choose **Application type: `Desktop app`**, enter a name (e.g. `OpenDrive Desktop`), and click **Create**.
   - Loopback redirects (`http://127.0.0.1:<port>/callback`) are allowed automatically for Desktop clients.

6. **Paste credentials & connect in OpenDrive**
   - Copy the **Client ID** (`...apps.googleusercontent.com`) and **Client Secret** (`GOCSPX-...`).
   - Open OpenDrive → **Connect** → paste both fields, optionally set **Account Nickname** / **Mount Folder** / **Mount as Read-Only**, and click **Sign in with Google** (or **Copy Link**).
   - If Google shows _"Google hasn't verified this app"_, click **Advanced → Go to OpenDrive (unsafe) → Continue**.

---

## Option B — Service Account JSON (No-Login Mode)

Best for Shared Drives and headless or automated access without browser login.

1. **Enable Google Drive API** in your Google Cloud project (Option A, Steps 1–2).
2. **Create a Service Account**
   - Go to **IAM & Admin → Service Accounts → + Create Service Account**, enter a name (e.g. `opendrive-sync`), and click **Done** (no project IAM role is needed).
3. **Download the JSON Key**
   - Open the service account → **Keys** tab → **Add key → Create new key → JSON → Create** to download the `.json` key file.
4. **Share Drive folders with `client_email`**
   - Open the downloaded `.json` file and copy the `client_email` (`...@<project>.iam.gserviceaccount.com`).
   - Service accounts have no personal My Drive files, so in [`drive.google.com`](https://drive.google.com/) share your target folders or Shared Drives with that `client_email` (**Editor** for Read-Write, **Viewer** for Read-Only).
5. **Connect in OpenDrive**
   - Switch OpenDrive's Connect drawer to **Service Account JSON (No Login)**, drag-and-drop or paste the JSON key, and click **Connect Service Account**.

---

## Quick Troubleshooting

- **Missing scope / insufficient permissions (`403`)**:
  - Open **Google Auth platform → Data Access → Add or remove scopes**, verify `https://www.googleapis.com/auth/drive` (or `.../drive.readonly`) is checked and saved, then click **Sign In** on the account row in OpenDrive to re-authorize.
- **`403 access_denied` (app in Testing)**:
  - Add your Gmail address under **Audience → Test users**, or click **Publish app**.
- **`redirect_uri_mismatch`**:
  - Ensure the OAuth client Application type is **`Desktop app`**, not Web application.
- **Multiple Google accounts**:
  - One **Client ID & Client Secret** pair can authorize multiple personal Google accounts.
