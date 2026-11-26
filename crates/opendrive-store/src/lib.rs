//! SQLite metadata + content cache index + upload queue.

mod cache;
mod queue_store;
mod types;

pub use types::{now_ms, Node, NodeState, Result, StoreError, UploadRow, SCHEMA_SQL};

use rusqlite::{params, Connection, OptionalExtension};
use types::row_to_node;
use uuid::Uuid;

const NODE_COLS: &str = "id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms";

pub struct Store {
    pub(crate) conn: Connection,
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
        let n: i64 =
            self.conn
                .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM nodes", [], |r| {
                    r.get(0)
                })?;
        Ok(n)
    }

    pub fn upsert_node(&self, n: &Node) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO nodes (id, account_id, drive_id, parent_id, name, display_name, kind, mime, size, md5, modified_ms, head_revision, trashed, shared_drive_id, state, pinned, last_access_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
             ON CONFLICT(id) DO UPDATE SET
               drive_id=excluded.drive_id, parent_id=excluded.parent_id, name=excluded.name,
               display_name=excluded.display_name, kind=excluded.kind, mime=excluded.mime,
               size=excluded.size, md5=excluded.md5, modified_ms=excluded.modified_ms,
               head_revision=excluded.head_revision, trashed=excluded.trashed,
               state=excluded.state, pinned=excluded.pinned, last_access_ms=excluded.last_access_ms",
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
        let sql = format!("SELECT {NODE_COLS} FROM nodes WHERE account_id=?1 AND drive_id=?2");
        self.conn
            .query_row(&sql, params![account_id.to_string(), drive_id], row_to_node)
            .optional()
            .map_err(Into::into)
    }

    pub fn get_node(&self, id: i64) -> Result<Option<Node>> {
        let sql = format!("SELECT {NODE_COLS} FROM nodes WHERE id=?1");
        self.conn
            .query_row(&sql, params![id], row_to_node)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_children(&self, parent_id: i64) -> Result<Vec<Node>> {
        let sql =
            format!("SELECT {NODE_COLS} FROM nodes WHERE parent_id=?1 AND trashed=0 ORDER BY name");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id], row_to_node)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_children_sorted(&self, parent_id: i64) -> Result<Vec<Node>> {
        let sql = format!(
            "SELECT {NODE_COLS} FROM nodes WHERE parent_id=?1 AND trashed=0
             ORDER BY (kind = 'folder') DESC, LOWER(display_name) ASC, LOWER(name) ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id], row_to_node)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_node_state(
        &self,
        node_id: i64,
        state: NodeState,
        pinned: Option<bool>,
    ) -> Result<bool> {
        let updated = match pinned {
            Some(p) => self.conn.execute(
                "UPDATE nodes SET state=?1, pinned=?2 WHERE id=?3",
                params![state.as_str(), p as i32, node_id],
            )?,
            None => self.conn.execute(
                "UPDATE nodes SET state=?1 WHERE id=?2",
                params![state.as_str(), node_id],
            )?,
        };
        Ok(updated > 0)
    }

    pub fn mark_drive_id_trashed(&self, account_id: Uuid, drive_id: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE nodes SET trashed=1 WHERE account_id=?1 AND drive_id=?2",
            params![account_id.to_string(), drive_id],
        )?;
        Ok(n > 0)
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
}
