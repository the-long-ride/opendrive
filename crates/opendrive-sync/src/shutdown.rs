//! Graceful shutdown: wait for or report pending uploads.

use crate::engine::SyncEngine;
use std::time::{Duration, Instant};
use tracing::{info, warn};

impl SyncEngine {
    /// Block until upload queue is empty or `timeout` elapses.
    /// Returns remaining queue depth.
    pub async fn wait_for_uploads(&self, timeout: Duration) -> usize {
        let start = Instant::now();
        loop {
            let depth = self.store.lock().queue_depth().unwrap_or(0);
            if depth == 0 {
                info!("upload queue drained");
                return 0;
            }
            if start.elapsed() >= timeout {
                warn!(remaining = depth, "shutdown with pending uploads");
                return depth;
            }
            // try to make progress
            let _ = self.process_one_upload().await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub fn pending_uploads(&self) -> usize {
        self.store.lock().queue_depth().unwrap_or(0)
    }
}
