//! Cross-platform FUSE inode <-> VfsEngine NodeId mapping and stat helpers.

use opendrive_sync::{Kind, NodeId};
use std::collections::HashMap;
use std::sync::Mutex;

/// Bidirectional-ready inode table mapping FUSE inodes (`1` = root) to `NodeId` (`0` = root).
pub struct InodeTable {
    ino_to_node: Mutex<HashMap<u64, NodeId>>,
    next_ino: Mutex<u64>,
}

impl Default for InodeTable {
    fn default() -> Self {
        Self::new()
    }
}

impl InodeTable {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        map.insert(1, 0);
        Self {
            ino_to_node: Mutex::new(map),
            next_ino: Mutex::new(2),
        }
    }

    pub fn alloc_ino(&self, node: NodeId) -> u64 {
        let mut map = self.ino_to_node.lock().unwrap();
        if let Some((&ino, _)) = map.iter().find(|(_, n)| **n == node) {
            return ino;
        }
        let mut next = self.next_ino.lock().unwrap();
        let ino = *next;
        *next += 1;
        map.insert(ino, node);
        ino
    }

    pub fn node_of(&self, ino: u64) -> Option<NodeId> {
        self.ino_to_node.lock().unwrap().get(&ino).copied()
    }

    pub fn perm_for_kind(kind: &Kind) -> u16 {
        match kind {
            Kind::Folder => 0o755,
            _ => 0o644,
        }
    }

    pub fn blocks_for_size(size: u64) -> u64 {
        size.div_ceil(512)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_inode_is_pre_seeded() {
        let table = InodeTable::default();
        assert_eq!(table.node_of(1), Some(0));
        assert_eq!(table.alloc_ino(0), 1);
    }

    #[test]
    fn alloc_ino_is_sequential_and_idempotent() {
        let table = InodeTable::new();
        let ino_a = table.alloc_ino(42);
        let ino_b = table.alloc_ino(99);
        assert_eq!(ino_a, 2);
        assert_eq!(ino_b, 3);
        assert_eq!(table.alloc_ino(42), ino_a);
        assert_eq!(table.alloc_ino(99), ino_b);
        assert_eq!(table.node_of(ino_a), Some(42));
        assert_eq!(table.node_of(ino_b), Some(99));
    }

    #[test]
    fn unknown_inode_returns_none() {
        let table = InodeTable::new();
        assert_eq!(table.node_of(999), None);
    }

    #[test]
    fn permissions_match_entry_kind() {
        assert_eq!(InodeTable::perm_for_kind(&Kind::Folder), 0o755);
        assert_eq!(InodeTable::perm_for_kind(&Kind::File), 0o644);
        assert_eq!(InodeTable::perm_for_kind(&Kind::NativeDoc), 0o644);
        assert_eq!(InodeTable::perm_for_kind(&Kind::Shortcut), 0o644);
    }

    #[test]
    fn blocks_for_size_rounds_up_512_byte_sectors() {
        assert_eq!(InodeTable::blocks_for_size(0), 0);
        assert_eq!(InodeTable::blocks_for_size(1), 1);
        assert_eq!(InodeTable::blocks_for_size(512), 1);
        assert_eq!(InodeTable::blocks_for_size(513), 2);
        assert_eq!(InodeTable::blocks_for_size(1024), 2);
    }
}
