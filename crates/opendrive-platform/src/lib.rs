//! Autostart, paths, notifications, single-instance, tray.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PlatformError>;

pub fn config_dir() -> Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from("."))
}

pub fn cache_dir() -> Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from("./cache"))
}

pub fn ensure_single_instance() -> Result<()> {
    Ok(())
}
