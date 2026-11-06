//! Sync engine; implements VfsEngine trait.
//!
//! Safety invariants (release blockers, design §5.5):
//! 1. Never delete/overwrite remotely unless user action caused it.
//! 2. Queued uploads survive crash and resume without loss/duplication.
//! 3. Eviction never drops dirty data.
//! 4. Remote changes never clobber unsynced local edits (conflict copy).
//! 5. Quit waits for or reports pending uploads.

use async_trait::async_trait;
use bytes::Bytes;
use std::ffi::OsStr;
use uuid::Uuid;

pub type NodeId = u64;
pub type AccountId = Uuid;
pub type PageToken = String;

#[derive(Debug, Clone)]
pub enum Kind {
    File,
    Folder,
    NativeDoc,
    Shortcut,
}

#[derive(Debug, Clone)]
pub struct NodeAttr {
    pub id: NodeId,
    pub name: String,
    pub kind: Kind,
    pub size: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone)]
pub struct DirPage {
    pub entries: Vec<NodeAttr>,
    pub next_page: Option<PageToken>,
}

#[derive(Debug)]
pub struct WriteHandle {
    pub node: NodeId,
}

#[derive(Debug, Clone)]
pub struct EngineStats {
    pub transfers: u64,
    pub queue_depth: usize,
    pub errors: u64,
}

#[async_trait]
pub trait VfsEngine: Send + Sync {
    async fn list(&self, node: NodeId, page: Option<PageToken>) -> Result<DirPage>;
    async fn lookup(&self, parent: NodeId, name: &OsStr) -> Result<Option<NodeAttr>>;
    async fn stat(&self, node: NodeId) -> Result<NodeAttr>;
    async fn read(&self, node: NodeId, offset: u64, len: u32) -> Result<Bytes>;
    async fn open_write(&self, node: NodeId) -> Result<WriteHandle>;
    async fn create(&self, parent: NodeId, name: &OsStr, kind: Kind) -> Result<NodeAttr>;
    async fn rename(&self, node: NodeId, new_parent: NodeId, new_name: &OsStr) -> Result<()>;
    async fn remove(&self, node: NodeId) -> Result<()>;
    async fn set_pinned(&self, node: NodeId, pinned: bool) -> Result<()>;
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<EngineEvent>;
    async fn add_account(&self, acct: AccountId) -> Result<()>;
    async fn remove_account(&self, acct: AccountId) -> Result<()>;
    async fn stats(&self) -> EngineStats;
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    NodeChanged(NodeId),
    AccountAdded(AccountId),
    AccountRemoved(AccountId),
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("not found")]
    NotFound,
    #[error("io: {0}")]
    Io(String),
    #[error("conflict")]
    Conflict,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Placeholder engine for M0 scaffolding. Real engine lands in M1.
pub struct PlaceholderEngine;

#[async_trait]
impl VfsEngine for PlaceholderEngine {
    async fn list(&self, _node: NodeId, _page: Option<PageToken>) -> Result<DirPage> {
        Ok(DirPage {
            entries: vec![],
            next_page: None,
        })
    }
    async fn lookup(&self, _parent: NodeId, _name: &OsStr) -> Result<Option<NodeAttr>> {
        Ok(None)
    }
    async fn stat(&self, _node: NodeId) -> Result<NodeAttr> {
        Err(SyncError::NotFound)
    }
    async fn read(&self, _node: NodeId, _offset: u64, _len: u32) -> Result<Bytes> {
        Err(SyncError::NotFound)
    }
    async fn open_write(&self, node: NodeId) -> Result<WriteHandle> {
        Ok(WriteHandle { node })
    }
    async fn create(&self, _parent: NodeId, _name: &OsStr, _kind: Kind) -> Result<NodeAttr> {
        Err(SyncError::Other("not implemented".into()))
    }
    async fn rename(&self, _node: NodeId, _new_parent: NodeId, _new_name: &OsStr) -> Result<()> {
        Ok(())
    }
    async fn remove(&self, _node: NodeId) -> Result<()> {
        Ok(())
    }
    async fn set_pinned(&self, _node: NodeId, _pinned: bool) -> Result<()> {
        Ok(())
    }
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<EngineEvent> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        rx
    }
    async fn add_account(&self, _acct: AccountId) -> Result<()> {
        Ok(())
    }
    async fn remove_account(&self, _acct: AccountId) -> Result<()> {
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
