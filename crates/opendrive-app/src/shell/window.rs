//! Native eframe desktop window runner with custom frameless chrome, icon, and System Tray (excluded from headless coverage).

use crate::design::logo::logo_icon_data;
use crate::design::theme::{apply_studio_theme, ThemeMode};
use crate::desktop::ControlCenterApp;
use crate::runtime::bridge::Bridge;
use crate::shell::tray::{DesktopTray, TrayUiEvent};
use eframe::{egui, NativeOptions};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub fn run_native_window(bridge: Bridge) -> anyhow::Result<()> {
    let icon = Arc::new(logo_icon_data(64));
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenDrive")
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(icon)
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([610.0, 480.0]),
        centered: true,
        ..Default::default()
    };
    let tray_holder: Rc<RefCell<Option<DesktopTray>>> = Rc::new(RefCell::new(None));
    let tray_holder_ref = Rc::clone(&tray_holder);
    eframe::run_native(
        "OpenDrive",
        options,
        Box::new(move |cc| {
            apply_studio_theme(&cc.egui_ctx, ThemeMode::default());
            let queue = Arc::new(Mutex::new(VecDeque::<TrayUiEvent>::new()));
            let tray = DesktopTray::start(cc.egui_ctx.clone(), Arc::clone(&queue));
            let active = tray.is_some();
            let summary_updater = tray.as_ref().map(DesktopTray::summary_updater);
            *tray_holder_ref.borrow_mut() = tray;
            let mut app = ControlCenterApp::new(bridge).with_tray_events(queue, active);
            if let Some(updater) = summary_updater {
                app = app.with_tray_summary_updater(updater);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|err| anyhow::anyhow!("desktop window error: {err}"))?;
    Ok(())
}
