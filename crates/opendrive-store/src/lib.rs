//! SQLite metadata + content cache index + upload queue.

use rusqlite::{params, Connection, OptionalExtension};
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

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    pub fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_SQL)?;
        Ok(())
    }

    pub fn next_id(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM nodes", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn upsert_node(&self, n: &Node) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO nodes (id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
             ON CONFLICT(id) DO UPDATE SET
               parent_id=excluded.parent_id, name=excluded.name, display_name=excluded.display_name,
               kind=excluded.kind, mime=excluded.mime, size=excluded.size, md5=excluded.md5,
               modified_ms=excluded.modified_ms, head_revision=excluded.head_revision,
               trashed=excluded.trashed, state=excluded.state, pinned=excluded.pinned, last_access_ms=excluded.last_access_ms",
            params![
                n.id,
                n.account_id.to_string(),
                n.drive_id,
                n.parent_id,
                n.name,
                n.display_name,
                n.kind,
                n.mime,
                n.size,
                n.md5,
                n.modified_ms,
                n.head_revision,
                n.trashed as i32,
                n.shared_drive_id,
                n.state.as_str(),
                n.pinned as i32,
                n.last_access_ms,
            ],
        )?;
        Ok(n.id)
    }

    pub fn find_by_drive_id(&self, account_id: Uuid, drive_id: &str) -> Result<Option<Node>> {
        self.conn
            .query_row(
                "SELECT id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms FROM nodes WHERE account_id=?1 AND drive_id=?2",
                params![account_id.to_string(), drive_id],
                row_to_node,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_node(&self, id: i64) -> Result<Option<Node>> {
        self.conn
            .query_row(
                "SELECT id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms FROM nodes WHERE id=?1",
                params![id],
                row_to_node,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_children(&self, parent_id: i64) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms FROM nodes WHERE parent_id=?1 AND trashed=0 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![parent_id], row_to_node)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_change_token(&self, account_id: Uuid, token: &str) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO change_tokens (account_id, page_token, updated_ms) VALUES (?1,?2,?3)
             ON CONFLICT(account_id) DO UPDATE SET page_token=excluded.page_token, updated_ms=excluded.updated_ms",
            params![account_id.to_string(), token, now],
        )?;
        Ok(())
    }

    pub fn get_change_token(&self, account_id: Uuid) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT page_token FROM change_tokens WHERE account_id=?1",
                params![account_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn enqueue_upload(
        &self,
        node_id: i64,
        op: &str,
        base_revision: Option<&str>,
    ) -> Result<i64> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO upload_queue (node_id, op, state, attempts, next_try_ms, base_revision)
             VALUES (?1, ?2, 'pending', 0, ?3, ?4)",
            params![node_id, op, now, base_revision],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn next_upload(&self) -> Result<Option<UploadRow>> {
        let now = now_ms();
        self.conn
            .query_row(
                "SELECT op_id, node_id, op, state, attempts, next_try_ms, resumable_uri, base_revision
                 FROM upload_queue WHERE state IN ('pending','in_progress') AND next_try_ms <= ?1
                 ORDER BY next_try_ms ASC LIMIT 1",
                params![now],
                |r| {
                    Ok(UploadRow {
                        op_id: r.get(0)?,
                        node_id: r.get(1)?,
                        op: r.get(2)?,
                        state: r.get(3)?,
                        attempts: r.get(4)?,
                        next_try_ms: r.get(5)?,
                        resumable_uri: r.get(6)?,
                        base_revision: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_upload(
        &self,
        op_id: i64,
        state: &str,
        attempts: i32,
        next_try_ms: i64,
        resumable_uri: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE upload_queue SET state=?1, attempts=?2, next_try_ms=?3, resumable_uri=?4 WHERE op_id=?5",
            params![state, attempts, next_try_ms, resumable_uri, op_id],
        )?;
        Ok(())
    }

    pub fn queue_depth(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM upload_queue WHERE state IN ('pending','in_progress')",
            [],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

fn row_to_node(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
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

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_acct_drive ON nodes(account_id, drive_id);
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
