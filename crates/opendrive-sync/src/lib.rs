//! Sync engine; implements VfsEngine trait.
//!
//! Safety invariants (release blockers, design §5.5):
//! 1. Never delete/overwrite remotely unless user action caused it.
//! 2. Queued uploads survive crash and resume without loss/duplication.
//! 3. Eviction never drops dirty data.
//! 4. Remote changes never clobber unsynced local edits (conflict copy).
//! 5. Quit waits for or reports pending uploads.

mod engine;
mod poller;
mod queue;
mod staging;
mod worker;

pub use engine::SyncEngine;
pub use queue::{conflict_name, QueueEntry, UploadOp};
pub use staging::{StagingDir, StagingError};

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

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_store::{Node, NodeState, Store};

    #[tokio::test]
    async fn sync_engine_list() {
        let store = Store::open_in_memory().unwrap();
        let eng = SyncEngine::new(store);
        let acct = Uuid::new_v4();
        eng.seed_root_account(acct, "Work").unwrap();
        let page = eng.list(0, None).await.unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].name, "Work");
    }

    #[tokio::test]
    async fn create_enqueues_upload() {
        let store = Store::open_in_memory().unwrap();
        let eng = SyncEngine::new(store);
        let acct = Uuid::new_v4();
        let root = eng.seed_root_account(acct, "Work").unwrap();
        let attr = eng
            .create(root, OsStr::new("note.txt"), Kind::File)
            .await
            .unwrap();
        assert_eq!(attr.name, "note.txt");
        let stats = eng.stats().await;
        assert!(stats.queue_depth >= 1);
    }

    #[test]
    fn conflict_name_format() {
        assert_eq!(
            conflict_name("file.txt", "host", "2026-01-01"),
            "file.txt (conflict host 2026-01-01)"
        );
    }

    #[test]
    fn fake_drive_into_store() {
        use opendrive_fake_drive::{FakeDrive, FakeFile};
        let drive = FakeDrive::new();
        drive.add_file(FakeFile {
            id: "f1".into(),
            name: "hello.txt".into(),
            mime_type: "text/plain".into(),
            size: 5,
            parents: vec!["root".into()],
            trashed: false,
            content: b"hello".to_vec(),
        });
        let store = Store::open_in_memory().unwrap();
        let acct = Uuid::new_v4();
        let root_id = store.next_id().unwrap();
        store
            .upsert_node(&Node {
                id: root_id,
                account_id: acct,
                drive_id: "root".into(),
                parent_id: Some(0),
                name: "My Drive".into(),
                display_name: "My Drive".into(),
                kind: "folder".into(),
                mime: None,
                size: 0,
                md5: None,
                modified_ms: 0,
                head_revision: None,
                trashed: false,
                shared_drive_id: None,
                state: NodeState::Cached,
                pinned: false,
                last_access_ms: 0,
            })
            .unwrap();
        for f in drive.list_children("root") {
            let id = store.next_id().unwrap();
            store
                .upsert_node(&Node {
                    id,
                    account_id: acct,
                    drive_id: f.id,
                    parent_id: Some(root_id),
                    name: f.name.clone(),
                    display_name: f.name,
                    kind: "file".into(),
                    mime: Some(f.mime_type),
                    size: f.size as i64,
                    md5: None,
                    modified_ms: 0,
                    head_revision: None,
                    trashed: false,
                    shared_drive_id: None,
                    state: NodeState::Placeholder,
                    pinned: false,
                    last_access_ms: 0,
                })
                .unwrap();
        }
        assert_eq!(store.list_children(root_id).unwrap().len(), 1);
    }
}
