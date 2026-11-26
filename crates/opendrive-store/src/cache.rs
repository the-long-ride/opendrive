//! Content cache index helpers.

use crate::types::{now_ms, NodeState, Result};
use crate::Store;
use rusqlite::{params, OptionalExtension};

impl Store {
    pub fn set_cache_entry(
        &self,
        node_id: i64,
        path: &str,
        bytes_present: i64,
        pinned: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cache_files (node_id, path, bytes_present, pinned)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(node_id) DO UPDATE SET
               path=excluded.path, bytes_present=excluded.bytes_present, pinned=excluded.pinned",
            params![node_id, path, bytes_present, pinned as i32],
        )?;
        Ok(())
    }

    pub fn get_cache_path(&self, node_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM cache_files WHERE node_id=?1",
                params![node_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn cache_bytes_total(&self) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(bytes_present), 0) FROM cache_files",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Candidates for eviction: unpinned, not dirty nodes, LRU by last_access_ms.
    /// Never returns dirty or pinned nodes (safety invariant #3).
    pub fn eviction_candidates(&self, limit: usize) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.node_id FROM cache_files c
             JOIN nodes n ON n.id = c.node_id
             WHERE c.pinned = 0 AND n.pinned = 0 AND n.state != 'dirty'
             ORDER BY n.last_access_ms ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn remove_cache_entry(&self, node_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM cache_files WHERE node_id=?1", params![node_id])?;
        Ok(())
    }

    pub fn touch_access(&self, node_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE nodes SET last_access_ms=?1 WHERE id=?2",
            params![now_ms(), node_id],
        )?;
        Ok(())
    }

    pub fn clear_node_cache_state(&self, node_id: i64) -> Result<()> {
        self.remove_cache_entry(node_id)?;
        self.conn.execute(
            "UPDATE nodes SET state=?1, pinned=0 WHERE id=?2",
            params![NodeState::Placeholder.as_str(), node_id],
        )?;
        Ok(())
    }

    pub fn trash_node_and_clear(&self, node_id: i64) -> Result<()> {
        self.remove_cache_entry(node_id)?;
        self.conn.execute(
            "UPDATE nodes SET trashed=1, pinned=0 WHERE id=?1",
            params![node_id],
        )?;
        Ok(())
    }

    pub fn relocate_cache_prefix(&self, old_path: &str, new_path: &str) -> Result<usize> {
        let old_p = std::path::Path::new(old_path);
        let new_p = std::path::Path::new(new_path);
        let mut stmt = self.conn.prepare("SELECT node_id, path FROM cache_files")?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);
        let mut moved = 0usize;
        for (nid, p) in rows {
            if let Ok(rel) = std::path::Path::new(&p).strip_prefix(old_p) {
                let updated = if rel.as_os_str().is_empty() {
                    new_p.to_path_buf()
                } else {
                    new_p.join(rel)
                };
                self.conn.execute(
                    "UPDATE cache_files SET path=?1 WHERE node_id=?2",
                    params![updated.to_string_lossy().as_ref(), nid],
                )?;
                moved += 1;
            }
        }
        Ok(moved)
    }
}
