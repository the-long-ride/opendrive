//! Chunk storage (in-memory LRU + disk cache) and single-flight slot helpers for `StreamCache`.

use crate::engine::{sync_err, SyncEngine};
use crate::{NodeId, SyncError};
use bytes::Bytes;
use opendrive_drive::DriveClient;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Notify;

const MAX_MEM_CHUNKS: usize = 24;

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct ChunkKey {
    pub(crate) node_id: NodeId,
    pub(crate) rev_tag: String,
    pub(crate) chunk_idx: u64,
}

pub(crate) struct InFlightSlot {
    pub(crate) result: Mutex<Option<std::result::Result<Bytes, String>>>,
    pub(crate) notify: Notify,
}

impl InFlightSlot {
    pub(crate) fn new() -> Self {
        Self {
            result: Mutex::new(None),
            notify: Notify::new(),
        }
    }
}

pub(crate) type InFlightMap = Arc<Mutex<HashMap<(NodeId, u64), Arc<InFlightSlot>>>>;

pub(crate) struct FlightGuard {
    pub(crate) inflight: InFlightMap,
    pub(crate) key: (NodeId, u64),
    pub(crate) slot: Arc<InFlightSlot>,
    pub(crate) finished: bool,
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.inflight.lock().remove(&self.key);
            let mut res = self.slot.result.lock();
            if res.is_none() {
                *res = Some(Err("stream chunk fetch cancelled".into()));
            }
            self.slot.notify.notify_waiters();
        }
    }
}

pub(crate) struct MemChunkLru {
    entries: HashMap<ChunkKey, Bytes>,
    order: VecDeque<ChunkKey>,
}

impl MemChunkLru {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn get(&mut self, key: &ChunkKey) -> Option<Bytes> {
        let bytes = self.entries.get(key)?.clone();
        self.order.retain(|k| k != key);
        self.order.push_back(key.clone());
        Some(bytes)
    }

    pub(crate) fn insert(&mut self, key: ChunkKey, bytes: Bytes) {
        if self.entries.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }
        self.entries.insert(key.clone(), bytes);
        self.order.push_back(key);
        while self.entries.len() > MAX_MEM_CHUNKS {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }

    pub(crate) fn evict_node(&mut self, node_id: NodeId) {
        self.entries.retain(|k, _| k.node_id != node_id);
        self.order.retain(|k| k.node_id != node_id);
    }
}

pub(crate) fn read_disk_chunk(dir: &Path, key: &ChunkKey) -> Option<Bytes> {
    let node_dir = dir.join(key.node_id.to_string());
    if !disk_rev_matches(&node_dir, &key.rev_tag) {
        return None;
    }
    fs::read(node_dir.join(format!("{}.chk", key.chunk_idx)))
        .ok()
        .map(Bytes::from)
}

pub(crate) fn write_disk_chunk(dir: &Path, key: &ChunkKey, bytes: &Bytes) -> crate::Result<()> {
    let node_dir = dir.join(key.node_id.to_string());
    ensure_disk_rev(&node_dir, &key.rev_tag).map_err(|e| SyncError::Io(e.to_string()))?;
    let tmp = node_dir.join(format!(
        "{}.chk.{}.tmp",
        key.chunk_idx,
        uuid::Uuid::new_v4()
    ));
    let dst = node_dir.join(format!("{}.chk", key.chunk_idx));
    if let Err(e) = fs::write(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(SyncError::Io(e.to_string()));
    }
    if let Err(e) = fs::rename(&tmp, &dst) {
        let _ = fs::remove_file(&tmp);
        return Err(SyncError::Io(e.to_string()));
    }
    Ok(())
}

pub(crate) fn ensure_dir(path: PathBuf) -> crate::Result<PathBuf> {
    fs::create_dir_all(&path).map_err(|e| SyncError::Io(e.to_string()))?;
    Ok(path)
}

pub(crate) fn slice_chunk(chunk: &Bytes, base: u64, offset: u64, end: u64) -> Bytes {
    let s = (offset.saturating_sub(base) as usize).min(chunk.len());
    let e = (end.saturating_sub(base) as usize).min(chunk.len());
    if s < e {
        chunk.slice(s..e)
    } else {
        Bytes::new()
    }
}

pub(crate) fn acquire_flight(
    inflight: &InFlightMap,
    node_id: NodeId,
    chunk_idx: u64,
) -> Result<Arc<InFlightSlot>, Arc<InFlightSlot>> {
    let mut map = inflight.lock();
    match map.get(&(node_id, chunk_idx)) {
        Some(existing) => Err(Arc::clone(existing)),
        None => {
            let s = Arc::new(InFlightSlot::new());
            map.insert((node_id, chunk_idx), Arc::clone(&s));
            Ok(s)
        }
    }
}

pub(crate) async fn wait_flight(slot: &InFlightSlot) -> crate::Result<Bytes> {
    loop {
        let notified = slot.notify.notified();
        if let Some(res) = slot.result.lock().clone() {
            return res.map_err(SyncError::Io);
        }
        notified.await;
    }
}

pub(crate) struct StreamTarget {
    pub(crate) client: DriveClient,
    pub(crate) drive_id: String,
    pub(crate) size: u64,
    pub(crate) rev_tag: String,
}

impl SyncEngine {
    pub(crate) fn stream_target(&self, node_id: NodeId) -> crate::Result<StreamTarget> {
        let (account_id, drive_id, size, rev_tag) = {
            let store = self.store.lock();
            let n = store
                .get_node(node_id as i64)
                .map_err(sync_err)?
                .ok_or(SyncError::NotFound)?;
            let _ = store.touch_access(node_id as i64);
            let rev = format!("{}:{}", n.head_revision.as_deref().unwrap_or(""), n.size);
            (n.account_id, n.drive_id, n.size.max(0) as u64, rev)
        };
        Ok(StreamTarget {
            client: self.drive_client_for(account_id)?,
            drive_id,
            size,
            rev_tag,
        })
    }
}

fn disk_rev_matches(node_dir: &Path, rev_tag: &str) -> bool {
    fs::read_to_string(node_dir.join(".rev"))
        .map(|s| s == rev_tag)
        .unwrap_or(false)
}

fn ensure_disk_rev(node_dir: &Path, rev_tag: &str) -> std::io::Result<()> {
    if disk_rev_matches(node_dir, rev_tag) {
        return Ok(());
    }
    if node_dir.exists() {
        let _ = fs::remove_dir_all(node_dir);
    }
    fs::create_dir_all(node_dir)?;
    fs::write(node_dir.join(".rev"), rev_tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_lru_evicts_oldest_and_flight_guard_cancels_on_drop() {
        let mut lru = MemChunkLru::new();
        let dup_key = ChunkKey {
            node_id: 1,
            rev_tag: "r1".into(),
            chunk_idx: 0,
        };
        lru.insert(dup_key.clone(), Bytes::from_static(b"first"));
        lru.insert(dup_key, Bytes::from_static(b"second"));
        for idx in 1..26u64 {
            lru.insert(
                ChunkKey {
                    node_id: 1,
                    rev_tag: "r1".into(),
                    chunk_idx: idx,
                },
                Bytes::from_static(b"abc"),
            );
        }
        let tmp_blocker = std::env::temp_dir().join(format!("od-chk-blk-{}", uuid::Uuid::new_v4()));
        std::fs::write(&tmp_blocker, b"x").unwrap();
        assert!(write_disk_chunk(
            &tmp_blocker,
            &ChunkKey {
                node_id: 1,
                rev_tag: "r1".into(),
                chunk_idx: 0
            },
            &Bytes::from_static(b"x")
        )
        .is_err());
        let _ = std::fs::remove_file(&tmp_blocker);
        assert!(lru
            .get(&ChunkKey {
                node_id: 1,
                rev_tag: "r1".into(),
                chunk_idx: 0,
            })
            .is_none());
        assert!(lru
            .get(&ChunkKey {
                node_id: 1,
                rev_tag: "r1".into(),
                chunk_idx: 25,
            })
            .is_some());
        lru.evict_node(1);
        assert!(slice_chunk(&Bytes::from_static(b"hi"), 10, 0, 2).is_empty());

        let inflight: InFlightMap = Arc::new(Mutex::new(HashMap::new()));
        let Ok(slot) = acquire_flight(&inflight, 7, 0) else {
            panic!("expected flight slot");
        };
        let guard = FlightGuard {
            inflight: Arc::clone(&inflight),
            key: (7, 0),
            slot: Arc::clone(&slot),
            finished: false,
        };
        drop(guard);
        assert!(slot.result.lock().is_some());
        assert!(inflight.lock().is_empty());
    }
}
