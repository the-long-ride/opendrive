//! Upload queue persistence helpers.

use crate::types::{now_ms, Result, UploadRow};
use crate::Store;
use rusqlite::{params, OptionalExtension};

impl Store {
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
