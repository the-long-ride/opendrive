//! Range-cached chunked streaming with head+tail prewarm, single-flight coalescing, and read-ahead.

use crate::engine::SyncEngine;
use crate::limiter::BandwidthLimiter;
use crate::stream_store::{
    acquire_flight, ensure_dir, read_disk_chunk, slice_chunk, wait_flight, write_disk_chunk,
    ChunkKey, FlightGuard, InFlightMap, MemChunkLru, StreamTarget,
};
use crate::{NodeId, Result, SyncError};
use bytes::{Bytes, BytesMut};
use opendrive_drive::DriveClient;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const STREAM_CHUNK_BYTES: u64 = 2 * 1024 * 1024;

pub struct StreamCache {
    cache_dir: Mutex<Option<PathBuf>>,
    mem_lru: Mutex<MemChunkLru>,
    inflight: InFlightMap,
}

impl StreamCache {
    pub fn new() -> Self {
        Self {
            cache_dir: Mutex::new(None),
            mem_lru: Mutex::new(MemChunkLru::new()),
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_dir(&self, path: impl Into<PathBuf>) -> Result<()> {
        *self.cache_dir.lock() = Some(ensure_dir(path.into())?);
        Ok(())
    }

    pub fn evict_node(&self, node_id: NodeId) {
        self.mem_lru.lock().evict_node(node_id);
        if let Some(dir) = self.cache_dir.lock().clone() {
            let _ = fs::remove_dir_all(dir.join(node_id.to_string()));
        }
    }

    fn get_cached(&self, key: &ChunkKey) -> Option<Bytes> {
        if let Some(b) = self.mem_lru.lock().get(key) {
            return Some(b);
        }
        let dir = self.cache_dir.lock().clone()?;
        let b = read_disk_chunk(&dir, key)?;
        self.mem_lru.lock().insert(key.clone(), b.clone());
        Some(b)
    }

    fn put_cached(&self, key: ChunkKey, bytes: Bytes) {
        if let Some(dir) = self.cache_dir.lock().clone() {
            let _ = write_disk_chunk(&dir, &key, &bytes);
        }
        self.mem_lru.lock().insert(key, bytes);
    }

    fn is_cached_or_inflight(&self, key: &ChunkKey) -> bool {
        self.inflight
            .lock()
            .contains_key(&(key.node_id, key.chunk_idx))
            || self.get_cached(key).is_some()
    }

    #[allow(clippy::too_many_arguments)]
    async fn get_or_fetch_chunk(
        &self,
        client: &DriveClient,
        drive_id: &str,
        key: ChunkKey,
        file_size: u64,
        limiter: &BandwidthLimiter,
        transfers: &AtomicU64,
        errors: &AtomicU64,
    ) -> Result<Bytes> {
        if let Some(cached) = self.get_cached(&key) {
            return Ok(cached);
        }
        let slot = match acquire_flight(&self.inflight, key.node_id, key.chunk_idx) {
            Ok(s) => s,
            Err(s) => return wait_flight(&s).await,
        };
        let mut guard = FlightGuard {
            inflight: Arc::clone(&self.inflight),
            key: (key.node_id, key.chunk_idx),
            slot: Arc::clone(&slot),
            finished: false,
        };
        let start = key.chunk_idx.saturating_mul(STREAM_CHUNK_BYTES);
        let len = file_size.saturating_sub(start).min(STREAM_CHUNK_BYTES) as u32;
        let outcome = match client.download_range(drive_id, start, len).await {
            Ok(b) => {
                limiter.take(b.len() as u64).await;
                transfers.fetch_add(1, Ordering::Relaxed);
                self.put_cached(key, b.clone());
                Ok(b)
            }
            Err(e) => {
                errors.fetch_add(1, Ordering::Relaxed);
                Err(e.to_string())
            }
        };
        self.inflight.lock().remove(&guard.key);
        *slot.result.lock() = Some(outcome.clone());
        guard.finished = true;
        slot.notify.notify_waiters();
        outcome.map_err(SyncError::Io)
    }
}

impl Default for StreamCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncEngine {
    pub fn with_stream_cache_dir(self, path: impl Into<PathBuf>) -> Result<Self> {
        self.stream_cache.set_dir(path)?;
        Ok(self)
    }

    pub fn set_stream_cache_dir(&self, path: impl Into<PathBuf>) -> Result<()> {
        self.stream_cache.set_dir(path)
    }

    pub fn evict_node_stream_chunks(&self, node_id: NodeId) {
        self.stream_cache.evict_node(node_id);
    }

    pub async fn prewarm_head_and_tail(&self, node_id: NodeId) -> Result<()> {
        let t = self.stream_target(node_id)?;
        if t.size == 0 || t.drive_id.is_empty() {
            return Ok(());
        }
        let last = (t.size - 1) / STREAM_CHUNK_BYTES;
        let k0 = ChunkKey {
            node_id,
            rev_tag: t.rev_tag.clone(),
            chunk_idx: 0,
        };
        if last == 0 {
            self.fetch_one(&t.client, &t.drive_id, k0, t.size).await?;
            return Ok(());
        }
        let k1 = ChunkKey {
            node_id,
            rev_tag: t.rev_tag,
            chunk_idx: last,
        };
        let (r0, r1) = tokio::join!(
            self.fetch_one(&t.client, &t.drive_id, k0, t.size),
            self.fetch_one(&t.client, &t.drive_id, k1, t.size)
        );
        r0?;
        r1?;
        Ok(())
    }

    pub async fn read_cached_range(
        &self,
        node_id: NodeId,
        offset: u64,
        len: u32,
        read_ahead_chunks: usize,
    ) -> Result<Bytes> {
        let t = self.stream_target(node_id)?;
        if len == 0 {
            return Ok(Bytes::new());
        }
        if t.size == 0 {
            let b = t
                .client
                .download_range(&t.drive_id, offset, len)
                .await
                .map_err(|e| {
                    self.errors.fetch_add(1, Ordering::Relaxed);
                    SyncError::Io(e.to_string())
                })?;
            self.limiter.take(b.len() as u64).await;
            self.transfers.fetch_add(1, Ordering::Relaxed);
            return Ok(b);
        }
        if offset >= t.size {
            return Ok(Bytes::new());
        }
        let end = offset.saturating_add(len as u64).min(t.size);
        let (c0, c1) = (offset / STREAM_CHUNK_BYTES, (end - 1) / STREAM_CHUNK_BYTES);
        let mut single = None;
        let mut buf = BytesMut::new();
        for c in c0..=c1 {
            let key = ChunkKey {
                node_id,
                rev_tag: t.rev_tag.clone(),
                chunk_idx: c,
            };
            let chunk = self.fetch_one(&t.client, &t.drive_id, key, t.size).await?;
            let slice = slice_chunk(&chunk, c * STREAM_CHUNK_BYTES, offset, end);
            if c0 == c1 {
                single = Some(slice);
            } else {
                buf.extend_from_slice(&slice);
            }
        }
        self.spawn_ahead(&t, node_id, c1, read_ahead_chunks);
        Ok(single.unwrap_or_else(|| buf.freeze()))
    }

    fn spawn_ahead(&self, t: &StreamTarget, node_id: NodeId, c1: u64, read_ahead: usize) {
        if read_ahead == 0 {
            return;
        }
        let last = (t.size - 1) / STREAM_CHUNK_BYTES;
        let ahead_end = (c1 + read_ahead as u64).min(last);
        let missing: Vec<ChunkKey> = ((c1 + 1)..=ahead_end)
            .map(|c| ChunkKey {
                node_id,
                rev_tag: t.rev_tag.clone(),
                chunk_idx: c,
            })
            .filter(|k| !self.stream_cache.is_cached_or_inflight(k))
            .collect();
        if missing.is_empty() {
            return;
        }
        let (sc, lim, tx, err) = (
            Arc::clone(&self.stream_cache),
            Arc::clone(&self.limiter),
            Arc::clone(&self.transfers),
            Arc::clone(&self.errors),
        );
        let (client, drive_id, size) = (t.client.clone(), t.drive_id.clone(), t.size);
        tokio::spawn(async move {
            for k in missing {
                let _ = sc
                    .get_or_fetch_chunk(&client, &drive_id, k, size, &lim, &tx, &err)
                    .await;
            }
        });
    }

    async fn fetch_one(&self, c: &DriveClient, d: &str, k: ChunkKey, sz: u64) -> Result<Bytes> {
        self.stream_cache
            .get_or_fetch_chunk(c, d, k, sz, &self.limiter, &self.transfers, &self.errors)
            .await
    }
}
