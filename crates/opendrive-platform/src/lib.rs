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

pub fn data_dir() -> Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from("./data"))
}

pub fn ensure_single_instance() -> Result<()> {
    Ok(())
}

pub fn set_autostart(_enabled: bool) -> Result<()> {
    Ok(())
}

pub fn notify(_title: &str, _body: &str) -> Result<()> {
    // notify-rust later
    Ok(())
}

pub fn open_path(_path: &str) -> Result<()> {
    // open crate later
    Ok(())
}

/// Tray / menu-bar actions the UI can register.
#[derive(Debug, Clone)]
pub enum TrayAction {
    OpenFolder,
    PauseAll,
    ResumeAll,
    Quit,
}
