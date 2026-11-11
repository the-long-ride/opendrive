//! Content cache index helpers.

use super::{Result, Store};
use rusqlite::params;

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

    pub fn cache_bytes_total(&self) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(bytes_present), 0) FROM cache_files",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Candidates for eviction: unpinned, not dirty nodes, LRU by last_access_ms.
    /// Never returns dirty nodes (safety invariant #3).
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "UPDATE nodes SET last_access_ms=?1 WHERE id=?2",
            params![now, node_id],
        )?;
        Ok(())
    }
}
