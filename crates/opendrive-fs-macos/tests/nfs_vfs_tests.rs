//! Integration tests for `opendrive-fs-macos` (`NfsFrontend`, `NfsBackend`, `NFSFileSystem`).

use async_trait::async_trait;
use bytes::Bytes;
use nfsserve::nfs::{filename3, nfsstat3, sattr3};
use nfsserve::vfs::NFSFileSystem;
use opendrive_fs_macos::{NfsBackend, NfsFrontend};
use opendrive_store::Store;
use opendrive_sync::{
    AccountId, DirPage, EngineEvent, EngineStats, Kind, NodeAttr, NodeId, PageToken,
    PlaceholderEngine, Result as SyncResult, SyncEngine, SyncError, VfsEngine, WriteHandle,
};
use std::ffi::OsStr;
use std::sync::Arc;
use uuid::Uuid;

struct FailingEngine;

#[async_trait]
impl VfsEngine for FailingEngine {
    async fn list(&self, _node: NodeId, _page: Option<PageToken>) -> SyncResult<DirPage> {
        Err(SyncError::Io("list err".into()))
    }
    async fn lookup(&self, _parent: NodeId, _name: &OsStr) -> SyncResult<Option<NodeAttr>> {
        Err(SyncError::Io("lookup err".into()))
    }
    async fn stat(&self, _node: NodeId) -> SyncResult<NodeAttr> {
        Err(SyncError::Io("stat err".into()))
    }
    async fn read(&self, _node: NodeId, _offset: u64, _len: u32) -> SyncResult<Bytes> {
        Err(SyncError::Io("read err".into()))
    }
    async fn open_write(&self, node: NodeId) -> SyncResult<WriteHandle> {
        Ok(WriteHandle { node })
    }
    async fn write(&self, _node: NodeId, _offset: u64, _data: &[u8]) -> SyncResult<u32> {
        Err(SyncError::Io("write err".into()))
    }
    async fn create(&self, _parent: NodeId, _name: &OsStr, _kind: Kind) -> SyncResult<NodeAttr> {
        Err(SyncError::Io("create err".into()))
    }
    async fn rename(
        &self,
        _node: NodeId,
        _new_parent: NodeId,
        _new_name: &OsStr,
    ) -> SyncResult<()> {
        Err(SyncError::Io("rename err".into()))
    }
    async fn remove(&self, _node: NodeId) -> SyncResult<()> {
        Err(SyncError::Io("remove err".into()))
    }
    async fn set_pinned(&self, _node: NodeId, _pinned: bool) -> SyncResult<()> {
        Ok(())
    }
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<EngineEvent> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        rx
    }
    async fn add_account(&self, _acct: AccountId) -> SyncResult<()> {
        Ok(())
    }
    async fn remove_account(&self, _acct: AccountId) -> SyncResult<()> {
        Ok(())
    }
    async fn stats(&self) -> EngineStats {
        EngineStats {
            transfers: 0,
            queue_depth: 0,
            errors: 0,
        }
    }
}

#[test]
fn frontend_sync_mount_outside_runtime_and_default_path() {
    let dir = std::env::temp_dir().join(format!("od-mac-sync-{}", Uuid::new_v4()));
    let mut frontend = NfsFrontend::new(Arc::new(PlaceholderEngine));
    assert!(frontend.backend().is_some());
    assert_eq!(
        NfsFrontend::default_mount_path()
            .file_name()
            .and_then(|s| s.to_str()),
        Some("OpenDrive")
    );
    assert!(frontend.mount_command_hint("/tmp/od").contains("PORT"));

    frontend.serve_and_mount(dir.to_str().unwrap()).unwrap();
    assert!(frontend.is_mounted());
    assert!(frontend.listen_addr().is_some());
    frontend.unmount(dir.to_str().unwrap()).unwrap();
    assert!(!frontend.is_mounted());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn frontend_serve_loopback_is_idempotent_and_accepts_tcp() {
    let mut frontend = NfsFrontend::new(Arc::new(PlaceholderEngine));
    let addr1 = frontend.serve_loopback().await.unwrap();
    let addr2 = frontend.serve_loopback().await.unwrap();
    assert_eq!(addr1, addr2);

    let stream = tokio::net::TcpStream::connect(addr1).await.unwrap();
    drop(stream);
    frontend.unmount("/tmp/nonexistent-od-mac").unwrap();
}

#[tokio::test]
async fn nfs_vfs_handles_pagination_eof_getattr_and_errors() {
    let staging = std::env::temp_dir().join(format!("od-mac-edge-{}", Uuid::new_v4()));
    let store = Store::open_in_memory().unwrap();
    let engine = SyncEngine::new(store).with_staging_dir(&staging).unwrap();
    let backend = NfsBackend::new(Arc::new(engine));
    let vfs: &dyn NFSFileSystem = &backend;

    assert_eq!(vfs.getattr(1).await.unwrap().fileid, 1);
    assert!(matches!(
        vfs.getattr(999).await.unwrap_err(),
        nfsstat3::NFS3ERR_NOENT
    ));
    assert!(matches!(
        vfs.lookup(999, &b"x".as_slice().into()).await.unwrap_err(),
        nfsstat3::NFS3ERR_NOENT
    ));
    assert!(matches!(
        vfs.lookup(1, &vec![0xff].into()).await.unwrap_err(),
        nfsstat3::NFS3ERR_INVAL
    ));

    let f1: filename3 = b"one.txt".as_slice().into();
    let f2: filename3 = b"two.txt".as_slice().into();
    let id1 = vfs.create_exclusive(1, &f1).await.unwrap();
    let id2 = vfs.create_exclusive(1, &f2).await.unwrap();
    vfs.write(id1, 0, b"0123456789").await.unwrap();

    let (empty, eof0) = vfs.read(id1, 0, 0).await.unwrap();
    assert!(empty.is_empty() && eof0);
    let (past, eof_past) = vfs.read(id1, 50, 4).await.unwrap();
    assert!(past.is_empty() && eof_past);
    let (partial, eof_partial) = vfs.read(id1, 0, 4).await.unwrap();
    assert_eq!(partial, b"0123");
    assert!(!eof_partial);

    let p1 = vfs.readdir(1, 0, 1).await.unwrap();
    assert_eq!(p1.entries.len(), 1);
    assert!(!p1.end);
    let p2 = vfs.readdir(1, p1.entries[0].fileid, 10).await.unwrap();
    assert_eq!(p2.entries.len(), 1);
    assert_eq!(p2.entries[0].fileid, id2);
    assert!(p2.end);
    let p_end = vfs.readdir(1, 9999, 10).await.unwrap();
    assert!(p_end.entries.is_empty() && p_end.end);

    let _ = std::fs::remove_dir_all(staging);
}

#[tokio::test]
async fn nfs_vfs_and_backend_propagate_engine_io_errors() {
    let backend = NfsBackend::new(Arc::new(FailingEngine));
    let vfs: &dyn NFSFileSystem = &backend;
    let name: filename3 = b"f".as_slice().into();

    assert!(matches!(
        vfs.lookup(1, &name).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.readdir(1, 0, 10).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.create(1, &name, sattr3::default()).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.create_exclusive(1, &name).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.mkdir(1, &name).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.remove(1, &name).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.rename(1, &name, 1, &name).await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.write(1, 0, b"a").await.unwrap_err(),
        nfsstat3::NFS3ERR_IO
    ));
    assert!(matches!(
        vfs.read(999, 0, 4).await.unwrap_err(),
        nfsstat3::NFS3ERR_NOENT
    ));
    assert!(backend.getattr_meta(999).await.is_err());
    assert!(backend.read(999, 0, 4).await.is_err());
    assert!(backend.write(999, 0, b"x").await.is_err());
}
