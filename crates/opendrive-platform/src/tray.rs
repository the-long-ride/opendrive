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

pub fn desktop_tray_menu() -> Vec<TrayMenuItem> {
    let mut items = vec![
        TrayMenuItem {
            id: "summary".into(),
            label: "Standby · 0 B used".into(),
            action: TrayAction::Summary,
            enabled: false,
        },
        TrayMenuItem {
            id: "show".into(),
            label: "Show OpenDrive".into(),
            action: TrayAction::ShowWindow,
            enabled: true,
        },
    ];
    items.extend(default_menu());
    items
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_and_stub_have_expected_items() {
        let menu = default_menu();
        assert_eq!(menu.len(), 4);
        assert_eq!(menu[0].id, "open");
        assert_eq!(menu[0].action, TrayAction::OpenFolder);
        assert_eq!(menu[1].action, TrayAction::PauseAll);
        assert_eq!(menu[2].action, TrayAction::ResumeAll);
        assert_eq!(menu[3].action, TrayAction::Quit);
        assert!(menu.iter().all(|m| m.enabled));

        let stub = show_tray_stub();
        assert_eq!(stub.len(), 4);

        let desktop = desktop_tray_menu();
        assert_eq!(desktop.len(), 6);
        assert_eq!(desktop[0].action, TrayAction::Summary);
        assert!(!desktop[0].enabled);
        assert_eq!(desktop[1].action, TrayAction::ShowWindow);
    }
}
