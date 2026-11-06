//! Linux FUSE attribute conversion, node helpers, and mount entry point.

use crate::fuse_sys::FuseFs;
use crate::inode_table::InodeTable;
use fuser::{FileAttr, FileType, MountOption, ReplyEntry};
use opendrive_sync::{Kind, NodeAttr, NodeId, VfsEngine};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::error;

pub const TTL: Duration = Duration::from_secs(1);

pub fn root_attr() -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino: 1,
        size: 0,
        blocks: 0,
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: FileType::Directory,
        perm: 0o755,
        nlink: 2,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn attr_from(attr: &NodeAttr, ino: u64) -> FileAttr {
    let kind = match attr.kind {
        Kind::Folder => FileType::Directory,
        _ => FileType::RegularFile,
    };
    let mtime = UNIX_EPOCH + Duration::from_millis(attr.modified_ms.max(0) as u64);
    FileAttr {
        ino,
        size: attr.size,
        blocks: InodeTable::blocks_for_size(attr.size),
        atime: mtime,
        mtime,
        ctime: mtime,
        crtime: mtime,
        kind,
        perm: InodeTable::perm_for_kind(&attr.kind),
        nlink: 1,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn create_dir_entry(
    engine: &Arc<dyn VfsEngine>,
    inodes: &InodeTable,
    rt: &tokio::runtime::Handle,
    parent: u64,
    name: &OsStr,
    kind: Kind,
    reply: ReplyEntry,
) {
    let Some(parent_node) = inodes.node_of(parent) else {
        reply.error(libc::ENOENT);
        return;
    };
    let eng = engine.clone();
    let name = name.to_os_string();
    match rt.block_on(async move { eng.create(parent_node, &name, kind).await }) {
        Ok(attr) => {
            let ino = inodes.alloc_ino(attr.id);
            reply.entry(&TTL, &attr_from(&attr, ino), 0);
        }
        Err(e) => {
            error!(?e, "create entry");
            reply.error(libc::EIO);
        }
    }
}

pub fn resolve_node(inodes: &InodeTable, ino: u64) -> Option<NodeId> {
    inodes.node_of(ino)
}

pub fn mount(engine: Arc<dyn VfsEngine>, path: &str) -> Result<(), String> {
    let rt = tokio::runtime::Handle::try_current()
        .map_err(|_| "must run inside tokio runtime".to_string())?;
    let fs = FuseFs::new(engine, rt);
    let opts = vec![
        MountOption::FSName("opendrive".into()),
        MountOption::AutoUnmount,
        MountOption::DefaultPermissions,
    ];
    fuser::mount2(fs, path, &opts).map_err(|e| e.to_string())
}
