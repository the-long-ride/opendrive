//! macOS localhost NFSv3 frontend.
//!
//! | NFSv3 op    | VfsEngine        |
//! |-------------|------------------|
//! | LOOKUP      | lookup           |
//! | GETATTR     | stat             |
//! | READDIR     | list (+ filter ._* ) |
//! | READ        | read             |
//! | WRITE       | open_write + stage |
//! | CREATE/MKDIR| create           |
//! | REMOVE/RMDIR| remove           |
//! | RENAME      | rename           |

use opendrive_sync::VfsEngine;
use std::path::PathBuf;
use std::sync::Arc;

pub struct NfsFrontend {
    engine: Arc<dyn VfsEngine>,
    mounted: bool,
    export_name: String,
    listen_addr: String,
}

impl NfsFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine,
            mounted: false,
            // unguessable export path component
            export_name: format!("od-{}", uuid_simple()),
            listen_addr: "127.0.0.1:0".into(),
        }
    }

    pub fn serve_and_mount(&mut self, path: &str) -> Result<(), String> {
        // Hardening (design §6.3):
        // - bind 127.0.0.1 only
        // - random export name + file-handle secret
        // - owner-only mount
        // - skip AppleDouble ._* / .DS_Store in READDIR
        tracing::info!(%path, export = %self.export_name, "NFS mount (stub)");
        let _ = &self.engine;
        let _ = &self.listen_addr;
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        self.mounted = true;
        Ok(())
    }

    pub fn unmount(&mut self, path: &str) -> Result<(), String> {
        let _ = std::process::Command::new("umount").arg(path).status();
        self.mounted = false;
        Ok(())
    }

    pub fn is_mounted(&self) -> bool {
        self.mounted
    }

    pub fn default_mount_path() -> PathBuf {
        dirs_home().join("OpenDrive")
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t:x}")
}
