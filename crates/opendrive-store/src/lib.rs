//! SQLite metadata + content cache index + upload queue.

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("db: {0}")]
    Db(String),
    #[error("not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store;

impl Store {
    pub fn open(_path: &str) -> Result<Self> {
        Ok(Self)
    }

    pub fn migrate(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: i64,
    pub account_id: Uuid,
    pub drive_id: String,
    pub parent_id: Option<i64>,
    pub name: String,
    pub display_name: String,
    pub kind: String,
    pub mime: Option<String>,
    pub size: i64,
    pub md5: Option<String>,
    pub modified_ms: i64,
    pub head_revision: Option<String>,
    pub trashed: bool,
    pub shared_drive_id: Option<String>,
    pub state: String,
    pub pinned: bool,
    pub last_access_ms: i64,
}

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS nodes (
  id INTEGER PRIMARY KEY,
  account_id TEXT NOT NULL,
  drive_id TEXT NOT NULL,
  parent_id INTEGER,
  name TEXT NOT NULL,
  display_name TEXT NOT NULL,
  kind TEXT NOT NULL,
  mime TEXT,
  size INTEGER NOT NULL DEFAULT 0,
  md5 TEXT,
  modified_ms INTEGER NOT NULL DEFAULT 0,
  head_revision TEXT,
  trashed INTEGER NOT NULL DEFAULT 0,
  shared_drive_id TEXT,
  state TEXT NOT NULL DEFAULT 'placeholder',
  pinned INTEGER NOT NULL DEFAULT 0,
  last_access_ms INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS change_tokens (
  account_id TEXT PRIMARY KEY,
  page_token TEXT NOT NULL,
  updated_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS cache_files (
  node_id INTEGER PRIMARY KEY,
  path TEXT NOT NULL,
  bytes_present INTEGER NOT NULL DEFAULT 0,
  ranges BLOB,
  pinned INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS upload_queue (
  op_id INTEGER PRIMARY KEY,
  node_id INTEGER NOT NULL,
  op TEXT NOT NULL,
  state TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_try_ms INTEGER NOT NULL DEFAULT 0,
  resumable_uri TEXT,
  base_revision TEXT
);
"#;
