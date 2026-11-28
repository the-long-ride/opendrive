# Connect Google Drive Account Guide

OpenDrive uses your own free **Google Cloud** project credentials (BYO) so no third-party server ever sees your OAuth tokens or Google Drive files.

---

## Option A — Browser Sign-In (OAuth 2.0 Desktop App)

Recommended for personal Gmail and Google Workspace Drive accounts.

### Personal mode (one Google account)

For a new setup where only the Google account creating the Cloud project will sign in, use [Google's Personal mode setup guide](https://support.google.com/cloud/answer/15544987?hl=en#quick-setup-for-personal-use-personal-mode).

1. Create a Personal mode project and select **Desktop app** as the OAuth client type. Save the Client ID and Client Secret.
2. Enable the **Google Drive API** in that project if it isn't already enabled (see Step 2 below).
3. Follow **Paste credentials & connect in OpenDrive** (Step 6 below). Sign in with the same Google account that created the Personal mode project.

Personal mode does not show the unverified-app warning or have the seven-day test token expiry. It works only for the account that created the app. It cannot be switched to or from External. If you already have an External project, you can keep using it.

### External mode (multiple Google accounts or an existing External project)

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
     - Click **Publish app → Confirm** to avoid the seven-day testing token expiry.
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
   - In a shared **My Drive** folder, give the service account **Viewer** access to read existing items or **Editor** access to edit them. Editor access does not give the service account storage or let it own newly uploaded files.
   - To upload new files, give the service account a role that allows adding files in a **Shared Drive**, or use **Browser Sign-In** (Option A) to upload to My Drive as a Google user. See [Google's Shared Drive storage guidance](https://developers.google.com/workspace/drive/api/guides/about-shareddrives#availability_and_storage).
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
  - Use an **External** OAuth client for multiple Google accounts. A **Personal mode** client only works for its creator and returns `client_user_mismatch` for other accounts.
