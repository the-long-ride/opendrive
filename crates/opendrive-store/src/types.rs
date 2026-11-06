//! Store types, error definitions, and SQLite schema.

use rusqlite::Row;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("db: {0}")]
    Db(String),
    #[error("not found")]
    NotFound,
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeState {
    Placeholder,
    Hydrating,
    Cached,
    Dirty,
    Conflict,
}

impl NodeState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Placeholder => "placeholder",
            Self::Hydrating => "hydrating",
            Self::Cached => "cached",
            Self::Dirty => "dirty",
            Self::Conflict => "conflict",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "hydrating" => Self::Hydrating,
            "cached" => Self::Cached,
            "dirty" => Self::Dirty,
            "conflict" => Self::Conflict,
            _ => Self::Placeholder,
        }
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
    pub state: NodeState,
    pub pinned: bool,
    pub last_access_ms: i64,
}

#[derive(Debug, Clone)]
pub struct UploadRow {
    pub op_id: i64,
    pub node_id: i64,
    pub op: String,
    pub state: String,
    pub attempts: i32,
    pub next_try_ms: i64,
    pub resumable_uri: Option<String>,
    pub base_revision: Option<String>,
}

pub(crate) fn row_to_node(r: &Row<'_>) -> rusqlite::Result<Node> {
    Ok(Node {
        id: r.get(0)?,
        account_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap_or_default(),
        drive_id: r.get(2)?,
        parent_id: r.get(3)?,
        name: r.get(4)?,
        display_name: r.get(5)?,
        kind: r.get(6)?,
        mime: r.get(7)?,
        size: r.get(8)?,
        md5: r.get(9)?,
        modified_ms: r.get(10)?,
        head_revision: r.get(11)?,
        trashed: r.get::<_, i32>(12)? != 0,
        shared_drive_id: r.get(13)?,
        state: NodeState::parse(&r.get::<_, String>(14)?),
        pinned: r.get::<_, i32>(15)? != 0,
        last_access_ms: r.get(16)?,
    })
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_acct_drive ON nodes(account_id, drive_id) WHERE drive_id != '';
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
  op_id INTEGER PRIMARY KEY AUTOINCREMENT,
  node_id INTEGER NOT NULL,
  op TEXT NOT NULL,
  state TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_try_ms INTEGER NOT NULL DEFAULT 0,
  resumable_uri TEXT,
  base_revision TEXT
);
"#;
