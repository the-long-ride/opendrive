//! Autostart, paths, notifications, single-instance, tray.

mod drive_letter;
mod drive_mount;
mod explorer_quota;
mod mount_cleanup;
mod tray;

pub use drive_letter::{
    drive_icon_reg_command, drive_label_reg_command, drive_mount_command, drive_unmount_command,
    is_virtual_drive_supported, normalize_drive_letter, parse_subst_target,
    pick_available_drive_letter,
};
pub use drive_mount::{
    mount_virtual_drive, mount_virtual_drive_with_quota, unmount_virtual_drive,
    unmount_virtual_drive_for_target,
};
pub use explorer_quota::{
    build_launch_invocation, build_this_pc_reg_commands, compute_explorer_quota,
    format_bytes_compact, format_explorer_quota_label, format_explorer_tile_comment,
    launch_with_app, sync_this_pc_namespace, ExplorerQuota,
};
pub use mount_cleanup::{
    install_console_cleanup_handler, is_process_alive, notify_shell_mounts_changed,
    spawn_detached_background, wait_for_process_exit,
};
pub use tray::{default_menu, desktop_tray_menu, show_tray_stub, TrayMenuItem};

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
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

pub fn acquire_instance_lock(lock_path: &Path) -> Result<PathBuf> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PlatformError::Other(e.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
        .map_err(|e| PlatformError::Other(format!("instance lock {}: {e}", lock_path.display())))?;
    writeln!(file, "{}", std::process::id())
        .map_err(|e| PlatformError::Other(format!("write lock {}: {e}", lock_path.display())))?;
    Ok(lock_path.to_path_buf())
}

pub fn ensure_single_instance() -> Result<()> {
    let lock_path = data_dir()?.join("opendrive.lock");
    acquire_instance_lock(&lock_path).map(|_| ())
}

#[cfg(target_os = "windows")]
pub fn autostart_command(exe_path: &Path, enabled: bool) -> String {
    let exe = exe_path.display();
    if enabled {
        format!(
            r#"reg add HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenDrive /t REG_SZ /d "{exe}" /f"#
        )
    } else {
        r#"reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenDrive /f"#.into()
    }
}

#[cfg(not(target_os = "windows"))]
pub fn autostart_command(exe_path: &Path, enabled: bool) -> String {
    let exe = exe_path.display();
    if enabled {
        format!("enable-autostart:{exe}")
    } else {
        "disable-autostart:OpenDrive".into()
    }
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| PlatformError::Other(e.to_string()))?;
    let cmd = autostart_command(&exe, enabled);
    tracing::info!(enabled, %cmd, "configured autostart");
    Ok(())
}

pub fn notify(title: &str, body: &str) -> Result<()> {
    tracing::info!(%title, %body, "notify");
    Ok(())
}

pub fn open_path(path: &str) -> Result<()> {
    if path.trim().is_empty() {
        return Err(PlatformError::Other("empty path".into()));
    }
    open::that(path).map_err(|e| PlatformError::Other(e.to_string()))
}

/// Open a Google-native Doc/Sheet/Slide in the browser.
pub fn open_drive_file(file_id: &str) -> Result<()> {
    let trimmed = file_id.trim();
    if trimmed.is_empty() {
        return Err(PlatformError::Other("empty file id".into()));
    }
    let url = format!("https://drive.google.com/open?id={trimmed}");
    open_path(&url)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    Summary,
    ShowWindow,
    OpenFolder,
    PauseAll,
    ResumeAll,
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_dirs_resolve() {
        assert!(config_dir().unwrap().ends_with("opendrive"));
        assert!(cache_dir().unwrap().ends_with("opendrive"));
        assert!(data_dir().unwrap().ends_with("opendrive"));
        assert!(mount_root().unwrap().ends_with("OpenDrive"));
    }

    #[test]
    fn instance_lock_creates_and_blocks_duplicate() {
        let dir = std::env::temp_dir().join(format!("opendrive-lock-{}", std::process::id()));
        let lock = dir.join("app.lock");
        let _ = std::fs::remove_dir_all(&dir);

        let acquired = acquire_instance_lock(&lock).unwrap();
        assert_eq!(acquired, lock);
        let pid_txt = std::fs::read_to_string(&lock).unwrap();
        assert_eq!(pid_txt.trim(), std::process::id().to_string());

        assert!(acquire_instance_lock(&lock).is_err());
        std::fs::remove_file(&lock).unwrap();
        assert!(acquire_instance_lock(&lock).is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn autostart_command_and_notify() {
        let exe = Path::new("/opt/opendrive/opendrive-app");
        let on = autostart_command(exe, true);
        let off = autostart_command(exe, false);
        assert!(on.contains("opendrive-app"));
        assert_ne!(on, off);
        notify("OpenDrive", "Synced").unwrap();
        set_autostart(false).unwrap();
        ensure_dirs().unwrap();
    }

    #[test]
    fn instance_lock_fails_when_parent_is_a_file() {
        let dir = std::env::temp_dir().join(format!("opendrive-lockerr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        assert!(acquire_instance_lock(&blocker.join("child.lock")).is_err());
        assert!(acquire_instance_lock(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_path_and_open_drive_file_reject_empty_inputs() {
        assert!(open_path("").is_err());
        assert!(open_path("   \t").is_err());
        assert!(open_drive_file("").is_err());
        assert!(open_drive_file("   ").is_err());
        assert_ne!(TrayAction::Summary, TrayAction::Quit);
    }
}
