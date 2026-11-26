//! Windows Cloud Files API frontend.
//! One sync root per account under `%USERPROFILE%\OpenDrive\` (siblings).

#[cfg(windows)]
pub mod cfapi_sys;
mod provider;

pub use provider::ProviderBridge;

use opendrive_sync::VfsEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Identity registered with the Cloud Files platform.
#[derive(Debug, Clone)]
pub struct SyncRootIdentity {
    pub account_name: String,
    pub provider_name: String,
    pub provider_version: String,
    pub root_path: PathBuf,
    pub icon_resource: Option<String>,
}

pub const SYNC_ROOT_PROVIDER: &str = "OpenDrive";

pub(crate) fn sanitize_sync_root_account(name: &str) -> String {
    name.replace('!', "_")
}

pub(crate) fn is_opendrive_sync_root_id(raw: &str) -> bool {
    raw.strip_prefix(SYNC_ROOT_PROVIDER)
        .is_some_and(|rest| rest.starts_with('!'))
}

impl SyncRootIdentity {
    pub fn new(account_name: impl Into<String>, root_path: PathBuf) -> Self {
        Self {
            account_name: account_name.into(),
            provider_name: SYNC_ROOT_PROVIDER.into(),
            provider_version: env!("CARGO_PKG_VERSION").into(),
            root_path,
            icon_resource: None,
        }
    }

    pub fn with_icon_resource(mut self, icon: impl Into<String>) -> Self {
        self.icon_resource = Some(icon.into());
        self
    }
}

pub struct CloudFilesFrontend {
    engine: Arc<dyn VfsEngine>,
    roots: Vec<SyncRootIdentity>,
    bridges: Vec<Arc<ProviderBridge>>,
    icon_resource: Option<String>,
}

impl CloudFilesFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            engine,
            roots: Vec::new(),
            bridges: Vec::new(),
            icon_resource: None,
        }
    }

    pub fn with_icon_resource(mut self, icon: impl Into<String>) -> Self {
        self.icon_resource = Some(icon.into());
        self
    }

    /// Registers sync root metadata and prepares the local folder without requiring
    /// Windows CFAPI shell registration (safe for headless tests).
    pub fn register_sync_root(&mut self, path: &str, account_name: &str) -> Result<(), String> {
        self.register_sync_root_at(path, account_name, 0)
    }

    /// Registers sync root metadata bound to a specific VFS `root_node`.
    pub fn register_sync_root_at(
        &mut self,
        path: &str,
        account_name: &str,
        root_node: opendrive_sync::NodeId,
    ) -> Result<(), String> {
        let p = PathBuf::from(path);
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;

        let mut id = SyncRootIdentity::new(account_name, p);
        if let Some(icon) = &self.icon_resource {
            id = id.with_icon_resource(icon.clone());
        }
        tracing::info!(
            path = %id.root_path.display(),
            account = %id.account_name,
            root_node,
            "register sync root"
        );

        let bridge = Arc::new(ProviderBridge::with_root(self.engine.clone(), root_node));
        self.bridges.push(bridge);
        self.roots.push(id);
        Ok(())
    }

    #[cfg(windows)]
    pub fn connect_live(
        &mut self,
        path: &str,
        account_name: &str,
        rt: tokio::runtime::Handle,
    ) -> Result<cloud_filter::root::Connection<cfapi_sys::CfSyncFilter>, String> {
        self.connect_live_at(path, account_name, 0, rt)
    }

    #[cfg(windows)]
    pub fn connect_live_at(
        &mut self,
        path: &str,
        account_name: &str,
        root_node: opendrive_sync::NodeId,
        rt: tokio::runtime::Handle,
    ) -> Result<cloud_filter::root::Connection<cfapi_sys::CfSyncFilter>, String> {
        self.register_sync_root_at(path, account_name, root_node)?;
        let id = self.roots.last().expect("just pushed");
        let bridge = self.bridges.last().expect("just pushed").clone();
        cfapi_sys::connect_sync_root(id, bridge, rt)
    }

    pub fn reconcile_placeholders(root_path: &Path, rel_dirs: &[PathBuf]) {
        #[cfg(windows)]
        cfapi_sys::reconcile_placeholder_dirs(root_path, rel_dirs);
        #[cfg(not(windows))]
        let _ = (root_path, rel_dirs);
    }

    pub fn unregister_sync_root(&mut self, path: &str) -> Result<(), String> {
        let target = Path::new(path);
        let mut idx = 0;
        while idx < self.roots.len() {
            if self.roots[idx].root_path == target {
                self.roots.remove(idx);
                if idx < self.bridges.len() {
                    self.bridges.remove(idx);
                }
            } else {
                idx += 1;
            }
        }
        Ok(())
    }

    pub fn roots(&self) -> &[SyncRootIdentity] {
        &self.roots
    }

    pub fn parent_folder() -> PathBuf {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OpenDrive")
    }

    pub fn account_root(account_folder: &str) -> PathBuf {
        Self::parent_folder().join(account_folder)
    }
}

pub fn try_dehydrate_placeholder(path: &Path) -> bool {
    #[cfg(windows)]
    {
        cfapi_sys::dehydrate_placeholder(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

pub fn unregister_os_sync_root(path: &Path) {
    #[cfg(windows)]
    let _ = cloud_filter::root::SyncRootId::from_path(path).and_then(|id| id.unregister());
    #[cfg(not(windows))]
    let _ = path;
}

pub fn unregister_all_opendrive_roots() {
    #[cfg(windows)]
    if let Ok(roots) = cloud_filter::root::active_roots() {
        for r in roots {
            let id = r.id();
            if is_opendrive_sync_root_id(&id.to_os_string().to_string_lossy()) {
                let _ = id.unregister();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_sync::PlaceholderEngine;
    use uuid::Uuid;

    #[test]
    fn identity_defaults_and_account_paths() {
        let id = SyncRootIdentity::new("alice@example.com", PathBuf::from("C:\\OpenDrive\\Alice"));
        assert_eq!(id.provider_name, SYNC_ROOT_PROVIDER);
        assert_eq!(id.account_name, "alice@example.com");
        assert!(!id.provider_version.is_empty());
        assert_eq!(sanitize_sync_root_account("a!b"), "a_b");
        assert!(is_opendrive_sync_root_id("OpenDrive!S-1-5!acct"));
        assert!(!is_opendrive_sync_root_id("Other!S-1-5!acct"));
        let parent = CloudFilesFrontend::parent_folder();
        assert_eq!(
            parent.file_name().and_then(|s| s.to_str()),
            Some("OpenDrive")
        );
        assert_eq!(
            CloudFilesFrontend::account_root("Personal"),
            parent.join("Personal")
        );
    }

    #[test]
    fn register_and_unregister_sync_root_tracks_state() {
        let dir = std::env::temp_dir().join(format!("od-win-root-{}", Uuid::new_v4()));
        let path_str = dir.to_str().unwrap().to_string();
        let mut frontend = CloudFilesFrontend::new(Arc::new(PlaceholderEngine));
        frontend.register_sync_root(&path_str, "alice").unwrap();
        assert!(dir.is_dir());
        assert_eq!(frontend.roots().len(), 1);
        assert_eq!(frontend.roots()[0].account_name, "alice");
        frontend.unregister_sync_root(&path_str).unwrap();
        assert!(frontend.roots().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn provider_bridge_with_root_and_register_at() {
        let dir = std::env::temp_dir().join(format!("od-win-root-at-{}", Uuid::new_v4()));
        let path_str = dir.to_str().unwrap().to_string();
        let engine: Arc<dyn VfsEngine> = Arc::new(PlaceholderEngine);
        let bridge = ProviderBridge::with_root(engine.clone(), 42);
        assert_eq!(bridge.root_node(), 42);
        let mut frontend = CloudFilesFrontend::new(engine);
        frontend
            .register_sync_root_at(&path_str, "bob", 42)
            .unwrap();
        assert_eq!(frontend.roots().len(), 1);
        assert_eq!(frontend.roots()[0].account_name, "bob");
        assert_eq!(frontend.bridges[0].root_node(), 42);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn try_dehydrate_placeholder_returns_false_for_regular_and_missing_files() {
        let dir = std::env::temp_dir().join(format!("od-win-dehyd-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let regular = dir.join("regular.txt");
        std::fs::write(&regular, b"hello").unwrap();
        assert!(!try_dehydrate_placeholder(&regular));
        assert!(!try_dehydrate_placeholder(&dir));
        assert!(!try_dehydrate_placeholder(&dir.join("missing.txt")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn icon_resource_reconcile_and_unregister_non_matching_root() {
        let base = std::env::temp_dir().join(format!("od-win-icon-{}", Uuid::new_v4()));
        let (r1, r2) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(r1.join("sub")).unwrap();
        let id = SyncRootIdentity::new("a", r1.clone()).with_icon_resource("app.ico,0");
        assert_eq!(id.icon_resource.as_deref(), Some("app.ico,0"));
        let mut frontend =
            CloudFilesFrontend::new(Arc::new(PlaceholderEngine)).with_icon_resource("fe.ico,0");
        frontend
            .register_sync_root(r1.to_str().unwrap(), "alice")
            .unwrap();
        frontend
            .register_sync_root(r2.to_str().unwrap(), "bob")
            .unwrap();
        assert_eq!(
            frontend.roots()[0].icon_resource.as_deref(),
            Some("fe.ico,0")
        );
        CloudFilesFrontend::reconcile_placeholders(&r1, &[PathBuf::from("sub")]);
        frontend.unregister_sync_root(r2.to_str().unwrap()).unwrap();
        assert_eq!(frontend.roots().len(), 1);
        assert_eq!(frontend.roots()[0].account_name, "alice");
        let _ = std::fs::remove_dir_all(base);
    }
}
