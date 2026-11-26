//! Integration tests for `opendrive-fs-windows` error paths and multi-root tracking.

use bytes::Bytes;
use opendrive_fs_windows::{CloudFilesFrontend, ProviderBridge};
use opendrive_sync::{
    AccountId, DirPage, EngineEvent, EngineStats, Kind, NodeAttr, NodeId, PageToken,
    PlaceholderEngine, Result as SyncResult, SyncError, VfsEngine, WriteHandle,
};
use std::ffi::OsStr;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

type AsyncRet<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

struct FailingEngine;

impl VfsEngine for FailingEngine {
    fn list<'life0, 'async_trait>(
        &'life0 self,
        _node: NodeId,
        _page: Option<PageToken>,
    ) -> AsyncRet<'async_trait, SyncResult<DirPage>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("list boom".into())) })
    }
    fn lookup<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _parent: NodeId,
        _name: &'life1 OsStr,
    ) -> AsyncRet<'async_trait, SyncResult<Option<NodeAttr>>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("lookup boom".into())) })
    }
    fn stat<'life0, 'async_trait>(
        &'life0 self,
        _node: NodeId,
    ) -> AsyncRet<'async_trait, SyncResult<NodeAttr>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Err(SyncError::NotFound) })
    }
    fn read<'life0, 'async_trait>(
        &'life0 self,
        _node: NodeId,
        _offset: u64,
        _len: u32,
    ) -> AsyncRet<'async_trait, SyncResult<Bytes>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("read boom".into())) })
    }
    fn open_write<'life0, 'async_trait>(
        &'life0 self,
        node: NodeId,
    ) -> AsyncRet<'async_trait, SyncResult<WriteHandle>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async move { Ok(WriteHandle { node }) })
    }
    fn write<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _node: NodeId,
        _offset: u64,
        _data: &'life1 [u8],
    ) -> AsyncRet<'async_trait, SyncResult<u32>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("write boom".into())) })
    }
    fn create<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _parent: NodeId,
        _name: &'life1 OsStr,
        _kind: Kind,
    ) -> AsyncRet<'async_trait, SyncResult<NodeAttr>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("create boom".into())) })
    }
    fn rename<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _node: NodeId,
        _new_parent: NodeId,
        _new_name: &'life1 OsStr,
    ) -> AsyncRet<'async_trait, SyncResult<()>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("rename boom".into())) })
    }
    fn remove<'life0, 'async_trait>(
        &'life0 self,
        _node: NodeId,
    ) -> AsyncRet<'async_trait, SyncResult<()>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("remove boom".into())) })
    }
    fn set_pinned<'life0, 'async_trait>(
        &'life0 self,
        _node: NodeId,
        _pinned: bool,
    ) -> AsyncRet<'async_trait, SyncResult<()>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Err(SyncError::Io("pin boom".into())) })
    }
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<EngineEvent> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        rx
    }
    fn add_account<'life0, 'async_trait>(
        &'life0 self,
        _acct: AccountId,
    ) -> AsyncRet<'async_trait, SyncResult<()>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
    fn remove_account<'life0, 'async_trait>(
        &'life0 self,
        _acct: AccountId,
    ) -> AsyncRet<'async_trait, SyncResult<()>>
    where
        'life0: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
    fn stats<'life0, 'async_trait>(&'life0 self) -> AsyncRet<'async_trait, EngineStats>
    where
        'life0: 'async_trait,
    {
        Box::pin(async {
            EngineStats {
                transfers: 0,
                queue_depth: 0,
                errors: 0,
            }
        })
    }
}

#[tokio::test]
async fn provider_bridge_propagates_engine_errors_and_none_lookup() {
    let placeholder = ProviderBridge::new(Arc::new(PlaceholderEngine));
    assert_eq!(
        placeholder
            .lookup_child(0, OsStr::new("missing"))
            .await
            .unwrap(),
        None
    );

    let failing = ProviderBridge::new(Arc::new(FailingEngine));
    assert!(failing
        .fetch_placeholders(0)
        .await
        .unwrap_err()
        .contains("list boom"));
    assert!(failing
        .fetch_data(1, 0, 16)
        .await
        .unwrap_err()
        .contains("read boom"));
    assert!(failing
        .lookup_child(0, OsStr::new("a"))
        .await
        .unwrap_err()
        .contains("lookup boom"));
    assert!(failing
        .create_entry(0, OsStr::new("a"), false)
        .await
        .unwrap_err()
        .contains("create boom"));
    assert!(failing
        .write_data(1, 0, b"hi")
        .await
        .unwrap_err()
        .contains("write boom"));
    assert!(failing
        .rename_entry(1, 0, OsStr::new("b"))
        .await
        .unwrap_err()
        .contains("rename boom"));
    assert!(failing
        .delete_entry(1)
        .await
        .unwrap_err()
        .contains("remove boom"));
    assert!(failing
        .set_pinned(1, true)
        .await
        .unwrap_err()
        .contains("pin boom"));
    let fe = FailingEngine;
    let _rx = fe.subscribe();
    fe.add_account(Uuid::nil()).await.unwrap();
    fe.remove_account(Uuid::nil()).await.unwrap();
    assert_eq!(fe.stats().await.transfers, 0);
    assert!(fe.stat(1).await.is_err());
    assert!(fe.read(1, 0, 1).await.is_err());
    assert_eq!(fe.open_write(2).await.unwrap().node, 2);
    assert!(fe.write(1, 0, b"a").await.is_err());
    assert!(fe.lookup(0, OsStr::new("x")).await.is_err());
    assert!(fe.rename(1, 0, OsStr::new("x")).await.is_err());
    assert!(fe.remove(1).await.is_err());
}

#[test]
fn multi_root_register_and_selective_unregister() {
    let base = std::env::temp_dir().join(format!("od-win-multi-{}", Uuid::new_v4()));
    let dir_a = base.join("alice");
    let dir_b = base.join("bob");

    let mut frontend = CloudFilesFrontend::new(Arc::new(PlaceholderEngine));
    frontend
        .register_sync_root(dir_a.to_str().unwrap(), "alice")
        .unwrap();
    frontend
        .register_sync_root(dir_b.to_str().unwrap(), "bob")
        .unwrap();
    assert_eq!(frontend.roots().len(), 2);

    frontend
        .unregister_sync_root(dir_b.to_str().unwrap())
        .unwrap();
    assert_eq!(frontend.roots().len(), 1);
    assert_eq!(frontend.roots()[0].account_name, "alice");

    frontend
        .unregister_sync_root(dir_a.to_str().unwrap())
        .unwrap();
    assert!(frontend.roots().is_empty());

    let _ = std::fs::remove_dir_all(PathBuf::from(&base));
}

#[cfg(windows)]
#[tokio::test]
async fn connect_live_fails_cleanly_on_empty_path() {
    let mut frontend = CloudFilesFrontend::new(Arc::new(PlaceholderEngine));
    let handle = tokio::runtime::Handle::current();
    assert!(frontend.connect_live("", "alice", handle).is_err());
}
