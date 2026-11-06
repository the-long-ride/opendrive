//! Load/save Settings as JSON under the config dir.

use crate::{CoreError, Result, Settings};
use std::fs;
use std::path::{Path, PathBuf};

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load_settings(config_dir: &Path) -> Result<Settings> {
    let path = settings_path(config_dir);
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| CoreError::Other(e.to_string()))?;
    serde_json::from_str(&data).map_err(|e| CoreError::Other(e.to_string()))
}

pub fn save_settings(config_dir: &Path, settings: &Settings) -> Result<()> {
    fs::create_dir_all(config_dir).map_err(|e| CoreError::Other(e.to_string()))?;
    let path = settings_path(config_dir);
    let data =
        serde_json::to_string_pretty(settings).map_err(|e| CoreError::Other(e.to_string()))?;
    fs::write(path, data).map_err(|e| CoreError::Other(e.to_string()))
}
