//! macOS localhost NFSv3 frontend.

mod nfs_backend;
mod nfs_vfs;

pub use nfs_backend::NfsBackend;
pub use nfs_vfs::bind_loopback_nfs;

use opendrive_sync::VfsEngine;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct NfsFrontend {
    backend: Option<NfsBackend>,
    mounted: bool,
    export_name: String,
    listen: Option<SocketAddr>,
    server_task: Option<JoinHandle<()>>,
}

impl NfsFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self {
            backend: Some(NfsBackend::new(engine)),
            mounted: false,
            export_name: format!("od-{}", simple_id()),
            listen: None,
            server_task: None,
        }
    }

    pub fn export_name(&self) -> &str {
        &self.export_name
    }

    pub fn listen_addr(&self) -> Option<SocketAddr> {
        self.listen
    }

    /// Binds an NFSv3 TCP server on `127.0.0.1:0` and spawns its accept loop.
    pub async fn serve_loopback(&mut self) -> Result<SocketAddr, String> {
        if let Some(addr) = self.listen {
            return Ok(addr);
        }
        let backend = self.backend.clone().ok_or("NFS backend unavailable")?;
        let (addr, handle) = bind_loopback_nfs(backend, &self.export_name).await?;
        self.listen = Some(addr);
        self.server_task = Some(handle);
        Ok(addr)
    }

    /// Ensures the mount directory exists and binds loopback if called outside a Tokio runtime.
    pub fn serve_and_mount(&mut self, path: &str) -> Result<(), String> {
        tracing::info!(%path, export = %self.export_name, "NFS serve_and_mount");
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        if self.listen.is_none() && tokio::runtime::Handle::try_current().is_err() {
            let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            let _ = rt.block_on(self.serve_loopback())?;
        }
        self.mounted = true;
        Ok(())
    }

    pub fn mount_command_hint(&self, path: &str) -> String {
        let port = self
            .listen
            .map(|a| a.port().to_string())
            .unwrap_or_else(|| "PORT".into());
        format!(
            "mount_nfs -o resvport,rw 127.0.0.1:{port}:/{} {}",
            self.export_name, path
        )
    }

    pub fn unmount(&mut self, path: &str) -> Result<(), String> {
        let _ = std::process::Command::new("umount").arg(path).status();
        if let Some(handle) = self.server_task.take() {
            handle.abort();
        }
        self.listen = None;
        self.mounted = false;
        Ok(())
    }

    pub fn is_mounted(&self) -> bool {
        self.mounted
    }

    pub fn default_mount_path() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OpenDrive")
    }

    pub fn backend(&self) -> Option<&NfsBackend> {
        self.backend.as_ref()
    }
}

fn simple_id() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{:x}", d.as_nanos()))
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfsserve::nfs::{filename3, nfsstat3, sattr3};
    use nfsserve::vfs::{NFSFileSystem, VFSCapabilities};
    use opendrive_store::Store;
    use opendrive_sync::{PlaceholderEngine, SyncEngine};
    use uuid::Uuid;

    #[tokio::test]
    async fn serve_loopback_binds_ephemeral_port_and_formats_hint() {
        let dir = std::env::temp_dir().join(format!("od-mac-mnt-{}", Uuid::new_v4()));
        let mut frontend = NfsFrontend::new(Arc::new(PlaceholderEngine));
        let addr = frontend.serve_loopback().await.unwrap();
        assert!(addr.ip().is_loopback());
        assert!(addr.port() > 0);
        assert_eq!(frontend.listen_addr(), Some(addr));

        frontend.serve_and_mount(dir.to_str().unwrap()).unwrap();
        assert!(frontend.is_mounted());
        let hint = frontend.mount_command_hint("/tmp/od");
        assert!(hint.contains(&format!(
            "127.0.0.1:{}:/{}",
            addr.port(),
            frontend.export_name()
        )));

        frontend.unmount(dir.to_str().unwrap()).unwrap();
        assert!(!frontend.is_mounted());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn nfs_filesystem_trait_operations_work_end_to_end() {
        let staging = std::env::temp_dir().join(format!("od-mac-vfs-{}", Uuid::new_v4()));
        let store = Store::open_in_memory().unwrap();
        let engine = SyncEngine::new(store).with_staging_dir(&staging).unwrap();
        let backend = NfsBackend::new(Arc::new(engine));
        let vfs: &dyn NFSFileSystem = &backend;

        assert!(matches!(vfs.capabilities(), VFSCapabilities::ReadWrite));
        assert_eq!(vfs.root_dir(), 1);

        let dir_name: filename3 = b"folder".as_slice().into();
        let (dir_id, dir_attr) = vfs.mkdir(1, &dir_name).await.unwrap();
        assert_eq!(dir_attr.fileid, dir_id);

        let file_name: filename3 = b"a.txt".as_slice().into();
        let (file_id, _) = vfs
            .create(dir_id, &file_name, sattr3::default())
            .await
            .unwrap();
        let dup_err = vfs.create_exclusive(dir_id, &file_name).await.unwrap_err();
        assert!(matches!(dup_err, nfsstat3::NFS3ERR_EXIST));

        let updated = vfs.write(file_id, 0, b"abcdef").await.unwrap();
        assert_eq!(updated.size, 6);
        let (chunk, eof) = vfs.read(file_id, 2, 10).await.unwrap();
        assert_eq!(chunk, b"cdef");
        assert!(eof);
        assert_eq!(
            vfs.setattr(file_id, sattr3::default()).await.unwrap().size,
            6
        );

        let page1 = vfs.readdir(dir_id, 0, 1).await.unwrap();
        assert_eq!(page1.entries.len(), 1);
        assert!(page1.end);

        let new_name: filename3 = b"b.txt".as_slice().into();
        vfs.rename(dir_id, &file_name, 1, &new_name).await.unwrap();
        assert_eq!(vfs.lookup(1, &new_name).await.unwrap(), file_id);
        vfs.remove(1, &new_name).await.unwrap();

        let link_err = vfs
            .symlink(
                1,
                &b"l".as_slice().into(),
                &b"t".as_slice().into(),
                &sattr3::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(link_err, nfsstat3::NFS3ERR_NOTSUPP));
        assert!(matches!(
            vfs.readlink(1).await.unwrap_err(),
            nfsstat3::NFS3ERR_NOTSUPP
        ));

        let _ = std::fs::remove_dir_all(staging);
    }
}
