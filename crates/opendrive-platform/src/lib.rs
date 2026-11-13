//! Autostart, paths, notifications, single-instance, tray.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PlatformError>;

pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|p| p.join("opendrive"))
        .ok_or_else(|| PlatformError::Other("no config dir".into()))
}

pub fn cache_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|p| p.join("opendrive"))
        .ok_or_else(|| PlatformError::Other("no cache dir".into()))
}

pub fn data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|p| p.join("opendrive"))
        .ok_or_else(|| PlatformError::Other("no data dir".into()))
}

pub fn mount_root() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|p| p.join("OpenDrive"))
        .ok_or_else(|| PlatformError::Other("no home dir".into()))
}

pub fn ensure_dirs() -> Result<()> {
    for d in [config_dir()?, cache_dir()?, data_dir()?] {
        std::fs::create_dir_all(&d).map_err(|e| PlatformError::Other(e.to_string()))?;
    }
    Ok(())
}

pub fn ensure_single_instance() -> Result<()> {
    // TODO: flock / named mutex
    Ok(())
}

pub fn set_autostart(_enabled: bool) -> Result<()> {
    Ok(())
}

pub fn notify(title: &str, body: &str) -> Result<()> {
    tracing::info!(%title, %body, "notify");
    Ok(())
}

pub fn open_path(path: &str) -> Result<()> {
    open::that(path).map_err(|e| PlatformError::Other(e.to_string()))
}

#[derive(Debug, Clone)]
pub enum TrayAction {
    OpenFolder,
    PauseAll,
    ResumeAll,
    Quit,
}
