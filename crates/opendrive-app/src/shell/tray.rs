//! Integrated desktop System Tray setup, event routing, and pure action mapping.

use crate::desktop::UiAction;
use opendrive_platform::TrayAction;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayUiEvent {
    ShowWindow,
    OpenFolder,
    PauseAll,
    ResumeAll,
    Quit,
}

pub fn format_tray_summary(accounts: &[opendrive_core::Account]) -> String {
    let health = crate::design::formatters::summarize_sync_health(accounts);
    let total = crate::design::formatters::summarize_total_storage(accounts);
    let pct = (total.ratio * 100.0).round() as u32;
    if total.quota > 0 {
        format!(
            "{} · {}% used ({} / {})",
            health.label,
            pct,
            crate::design::formatters::format_bytes(total.used),
            crate::design::formatters::format_bytes(total.quota)
        )
    } else {
        format!(
            "{} · {} used",
            health.label,
            crate::design::formatters::format_bytes(total.used)
        )
    }
}

pub fn route_tray_action(action: &TrayAction) -> Option<TrayUiEvent> {
    match action {
        TrayAction::Summary => None,
        TrayAction::ShowWindow => Some(TrayUiEvent::ShowWindow),
        TrayAction::OpenFolder => Some(TrayUiEvent::OpenFolder),
        TrayAction::PauseAll => Some(TrayUiEvent::PauseAll),
        TrayAction::ResumeAll => Some(TrayUiEvent::ResumeAll),
        TrayAction::Quit => Some(TrayUiEvent::Quit),
    }
}

pub fn apply_tray_event(ev: TrayUiEvent, actions: &mut Vec<UiAction>) {
    match ev {
        TrayUiEvent::ShowWindow => actions.push(UiAction::WindowShowFromTray),
        TrayUiEvent::OpenFolder => actions.push(UiAction::OpenMountRoot),
        TrayUiEvent::PauseAll => actions.push(UiAction::AccountAction {
            action: "pause_all",
            id: None,
        }),
        TrayUiEvent::ResumeAll => actions.push(UiAction::AccountAction {
            action: "resume_all",
            id: None,
        }),
        TrayUiEvent::Quit => actions.push(UiAction::WindowForceQuit),
    }
}

pub type TrayEventQueue = Arc<Mutex<VecDeque<TrayUiEvent>>>;
pub type TraySummaryUpdater = std::rc::Rc<dyn Fn(&str)>;

#[cfg(any(feature = "tray", windows, target_os = "macos"))]
pub struct DesktopTray {
    _tray: tray_icon::TrayIcon,
    summary_item: Option<tray_icon::menu::MenuItem>,
}

#[cfg(any(feature = "tray", windows, target_os = "macos"))]
impl DesktopTray {
    pub fn start(ctx: eframe::egui::Context, queue: TrayEventQueue) -> Option<Self> {
        use opendrive_platform::desktop_tray_menu;
        use std::collections::HashMap;
        use tray_icon::menu::{Menu, MenuEvent, MenuItem};
        use tray_icon::{Icon, MouseButton, TrayIconBuilder, TrayIconEvent};

        let menu = Menu::new();
        let mut id_map = HashMap::new();
        let mut summary_item = None;
        for item in desktop_tray_menu() {
            let mi = MenuItem::new(&item.label, item.enabled, None);
            if item.action == TrayAction::Summary {
                summary_item = Some(mi.clone());
            } else if let Some(ev) = route_tray_action(&item.action) {
                id_map.insert(mi.id().clone(), ev);
            }
            if menu.append(&mi).is_err() {
                return None;
            }
        }
        let rgba = crate::design::logo::render_logo_rgba(32);
        let icon = Icon::from_rgba(rgba, 32, 32).ok()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("OpenDrive")
            .with_icon(icon)
            .build()
            .ok()?;

        let q_menu = Arc::clone(&queue);
        let ctx_menu = ctx.clone();
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            if let Some(&mapped) = id_map.get(&ev.id) {
                q_menu.lock().expect("tray queue").push_back(mapped);
                ctx_menu.request_repaint();
            }
        }));

        let q_click = queue;
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if matches!(
                ev,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                q_click
                    .lock()
                    .expect("tray queue")
                    .push_back(TrayUiEvent::ShowWindow);
                ctx.request_repaint();
            }
        }));

        Some(Self {
            _tray: tray,
            summary_item,
        })
    }

    pub fn summary_updater(&self) -> std::rc::Rc<dyn Fn(&str)> {
        let item = self.summary_item.clone();
        std::rc::Rc::new(move |text: &str| {
            if let Some(mi) = &item {
                mi.set_text(text);
            }
        })
    }
}

#[cfg(not(any(feature = "tray", windows, target_os = "macos")))]
pub struct DesktopTray;

#[cfg(not(any(feature = "tray", windows, target_os = "macos")))]
impl DesktopTray {
    pub fn start(_ctx: eframe::egui::Context, _queue: TrayEventQueue) -> Option<Self> {
        None
    }

    pub fn summary_updater(&self) -> std::rc::Rc<dyn Fn(&str)> {
        std::rc::Rc::new(|_| {})
    }
}
