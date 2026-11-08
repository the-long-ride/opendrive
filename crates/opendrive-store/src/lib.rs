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

    pub fn get_node(&self, id: i64) -> Result<Option<Node>> {
        self.conn
            .query_row(
                "SELECT id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms FROM nodes WHERE id=?1",
                params![id],
                |r| {
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
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_children(&self, parent_id: i64) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms FROM nodes WHERE parent_id=?1 AND trashed=0 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![parent_id], |r| {
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
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_change_token(&self, account_id: Uuid, token: &str) -> Result<()> {
        let now = chrono_like_now();
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
}

fn chrono_like_now() -> i64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_upsert_list() {
        let store = Store::open_in_memory().unwrap();
        let acct = Uuid::new_v4();
        let n = Node {
            id: 1,
            account_id: acct,
            drive_id: "d1".into(),
            parent_id: Some(0),
            name: "a.txt".into(),
            display_name: "a.txt".into(),
            kind: "file".into(),
            mime: Some("text/plain".into()),
            size: 3,
            md5: None,
            modified_ms: 0,
            head_revision: None,
            trashed: false,
            shared_drive_id: None,
            state: NodeState::Placeholder,
            pinned: false,
            last_access_ms: 0,
        };
        store.upsert_node(&n).unwrap();
        assert_eq!(store.list_children(0).unwrap().len(), 1);
        store.set_change_token(acct, "tok1").unwrap();
        assert_eq!(store.get_change_token(acct).unwrap().as_deref(), Some("tok1"));
    }
}
