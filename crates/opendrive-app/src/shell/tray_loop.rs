//! System tray event loop wired to `Bridge`.

use crate::runtime::bridge::Bridge;
use opendrive_core::AppCommand;
use opendrive_platform::{default_menu, TrayAction};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};

pub fn run_tray(bridge: Bridge) -> anyhow::Result<()> {
    let menu = Menu::new();
    let items = default_menu();
    let mut id_map = std::collections::HashMap::new();
    for item in &items {
        let mi = MenuItem::new(&item.label, item.enabled, None);
        id_map.insert(mi.id().clone(), item.action.clone());
        menu.append(&mi)?;
    }

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("OpenDrive")
        .build()?;

    let rx = MenuEvent::receiver();
    loop {
        if let Ok(ev) = rx.recv() {
            if let Some(action) = id_map.get(&ev.id) {
                match action {
                    TrayAction::Summary | TrayAction::ShowWindow => {}
                    TrayAction::OpenFolder => {
                        if let Ok(p) = opendrive_platform::mount_root() {
                            let _ = opendrive_platform::open_path(p.to_str().unwrap_or("."));
                        }
                    }
                    TrayAction::PauseAll => {
                        let _ = bridge.handle_blocking(AppCommand::PauseAll);
                        tracing::info!("tray: pause all");
                    }
                    TrayAction::ResumeAll => {
                        let _ = bridge.handle_blocking(AppCommand::ResumeAll);
                        tracing::info!("tray: resume all");
                    }
                    TrayAction::Quit => {
                        bridge.mounts().shutdown_all();
                        let _ = bridge.handle_blocking(AppCommand::Quit);
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}
