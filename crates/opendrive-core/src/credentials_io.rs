//! Persist CredentialSet metadata (client_id only).
//! client_secret stays in OS keychain via opendrive-auth.

use crate::{CoreError, CredentialSet, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join("credentials.json")
}

pub fn load_credentials(config_dir: &Path) -> Result<Vec<CredentialSet>> {
    let path = credentials_path(config_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| CoreError::Other(e.to_string()))?;
    serde_json::from_str(&data).map_err(|e| CoreError::Other(e.to_string()))
}

pub fn save_credentials(config_dir: &Path, creds: &[CredentialSet]) -> Result<()> {
    fs::create_dir_all(config_dir).map_err(|e| CoreError::Other(e.to_string()))?;
    let path = credentials_path(config_dir);
    let data = serde_json::to_string_pretty(creds).map_err(|e| CoreError::Other(e.to_string()))?;
    fs::write(path, data).map_err(|e| CoreError::Other(e.to_string()))
}
