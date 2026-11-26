//! Configurable runtime endpoints, file pickers, and app launchers for auth and explorer actions.

use super::{GOOGLE_DRIVE_API_BASE, GOOGLE_DRIVE_UPLOAD_BASE};
use crate::runtime::release_check::GITHUB_LATEST_RELEASE_URL;
use opendrive_auth::{AUTH_URL, TOKEN_URL};
use std::path::PathBuf;
use std::sync::Arc;

pub type BrowserOpener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
pub type VaultFilePicker = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;
pub type MultiFilePicker = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;
pub type ItemLauncher = Arc<dyn Fn(Option<&str>, &str) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct AuthEndpoints {
    pub auth_url: String,
    pub token_url: String,
    pub drive_api_base: String,
    pub drive_upload_base: String,
    pub redirect_timeout_secs: u64,
    pub browser_opener: BrowserOpener,
    pub folder_opener: BrowserOpener,
    pub vault_save_picker: VaultFilePicker,
    pub vault_open_picker: VaultFilePicker,
    pub upload_files_picker: MultiFilePicker,
    pub upload_folder_picker: VaultFilePicker,
    pub app_exe_picker: VaultFilePicker,
    pub item_launcher: ItemLauncher,
    pub release_api_url: String,
}

impl Default for AuthEndpoints {
    fn default() -> Self {
        Self {
            auth_url: AUTH_URL.to_string(),
            token_url: TOKEN_URL.to_string(),
            drive_api_base: GOOGLE_DRIVE_API_BASE.to_string(),
            drive_upload_base: GOOGLE_DRIVE_UPLOAD_BASE.to_string(),
            redirect_timeout_secs: 180,
            browser_opener: Arc::new(|url| {
                opendrive_platform::open_path(url).map_err(|e| e.to_string())
            }),
            folder_opener: Arc::new(|path| {
                opendrive_platform::open_path(path).map_err(|e| e.to_string())
            }),
            vault_save_picker: Arc::new(|| {
                rfd::FileDialog::new()
                    .add_filter("OpenDrive Encrypted Vault", &["odcred"])
                    .set_file_name("opendrive-credentials.odcred")
                    .save_file()
            }),
            vault_open_picker: Arc::new(|| {
                rfd::FileDialog::new()
                    .add_filter("OpenDrive Encrypted Vault", &["odcred"])
                    .pick_file()
            }),
            upload_files_picker: Arc::new(|| {
                rfd::FileDialog::new().pick_files().unwrap_or_default()
            }),
            upload_folder_picker: Arc::new(|| rfd::FileDialog::new().pick_folder()),
            app_exe_picker: Arc::new(|| rfd::FileDialog::new().pick_file()),
            item_launcher: Arc::new(|cmd, target| {
                opendrive_platform::launch_with_app(cmd, target).map_err(|e| e.to_string())
            }),
            release_api_url: GITHUB_LATEST_RELEASE_URL.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AuthEndpoints;

    #[test]
    fn default_endpoints_and_safe_error_closures() {
        let ep = AuthEndpoints::default();
        assert_eq!(ep.redirect_timeout_secs, 180);
        assert!(ep.auth_url.starts_with("https://"));
        assert!(ep.token_url.starts_with("https://"));
        assert!(ep.drive_api_base.starts_with("https://"));
        assert!(ep.drive_upload_base.starts_with("https://"));
        assert!((ep.browser_opener)("").is_err());
        assert!((ep.folder_opener)("   ").is_err());
        assert!((ep.item_launcher)(None, "").is_err());
        assert!((ep.item_launcher)(Some("opendrive-missing-bin-xyz"), "target").is_err());
    }
}
