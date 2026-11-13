//! System tray bridge.
//!
//! Live implementation will use `tray-icon` + platform event loop.
//! This module defines the menu model the UI/controller expects.

use super::TrayAction;

#[derive(Debug, Clone)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub action: TrayAction,
    pub enabled: bool,
}

pub fn default_menu() -> Vec<TrayMenuItem> {
    vec![
        TrayMenuItem {
            id: "open".into(),
            label: "Open OpenDrive folder".into(),
            action: TrayAction::OpenFolder,
            enabled: true,
        },
        TrayMenuItem {
            id: "pause".into(),
            label: "Pause sync".into(),
            action: TrayAction::PauseAll,
            enabled: true,
        },
        TrayMenuItem {
            id: "resume".into(),
            label: "Resume sync".into(),
            action: TrayAction::ResumeAll,
            enabled: true,
        },
        TrayMenuItem {
            id: "quit".into(),
            label: "Quit".into(),
            action: TrayAction::Quit,
            enabled: true,
        },
    ]
}

/// Placeholder: returns menu without showing a real tray icon.
pub fn show_tray_stub() -> Vec<TrayMenuItem> {
    let menu = default_menu();
    tracing::info!(items = menu.len(), "tray stub active (no icon)");
    menu
}
