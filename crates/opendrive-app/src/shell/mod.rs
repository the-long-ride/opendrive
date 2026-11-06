//! Native OS desktop shell integration: frameless window runner and system tray bridge.

pub mod tray;
#[cfg(feature = "tray")]
pub mod tray_loop;
pub mod window;
