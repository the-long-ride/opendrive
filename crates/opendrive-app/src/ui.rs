//! UI entry (GPUI later).
//!
//! Enable with `--features gpui` once the dependency is pinned.
//! Until then, CLI is the supported interface.

use opendrive_core::AppController;

/// Placeholder: will run GPUI event loop and bridge AppCommand/AppEvent.
pub fn run_ui(_ctrl: AppController) -> anyhow::Result<()> {
    anyhow::bail!("GPUI UI not enabled; use CLI commands (authorize, list-root, status)")
}
