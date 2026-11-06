//! Persist Account list (no tokens — those stay in keychain).

use crate::{Account, CoreError, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn accounts_path(config_dir: &Path) -> PathBuf {
    config_dir.join("accounts.json")
}

pub fn load_accounts(config_dir: &Path) -> Result<Vec<Account>> {
    let path = accounts_path(config_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| CoreError::Other(e.to_string()))?;
    serde_json::from_str(&data).map_err(|e| CoreError::Other(e.to_string()))
}

pub fn save_accounts(config_dir: &Path, accounts: &[Account]) -> Result<()> {
    fs::create_dir_all(config_dir).map_err(|e| CoreError::Other(e.to_string()))?;
    let path = accounts_path(config_dir);
    let data =
        serde_json::to_string_pretty(accounts).map_err(|e| CoreError::Other(e.to_string()))?;
    fs::write(path, data).map_err(|e| CoreError::Other(e.to_string()))
}
