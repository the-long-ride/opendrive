//! Linux FUSE attribute conversion, node helpers, and mount entry point.

use crate::fuse_sys::FuseFs;
use crate::inode_table::InodeTable;
use fuser::{FileAttr, FileType, MountOption};
use opendrive_sync::{Kind, NodeAttr, VfsEngine};
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

pub fn create_inode_attr(
    engine: &Arc<dyn VfsEngine>,
    inodes: &InodeTable,
    rt: &tokio::runtime::Handle,
    parent: u64,
    name: &OsStr,
    kind: Kind,
) -> Result<(u64, FileAttr), libc::c_int> {
    let parent_node = inodes.node_of(parent).ok_or(libc::ENOENT)?;
    let eng = engine.clone();
    let name = name.to_os_string();
    let attr = rt
        .block_on(async move { eng.create(parent_node, &name, kind).await })
        .map_err(|e| {
            error!(?e, "create entry");
            libc::EIO
        })?;
    let ino = inodes.alloc_ino(attr.id);
    Ok((ino, attr_from(&attr, ino)))
}

pub fn mount(engine: Arc<dyn VfsEngine>, path: &str) -> Result<(), String> {
    let rt = tokio::runtime::Handle::try_current()
        .map_err(|_| "must run inside tokio runtime".to_string())?;
    let fs = FuseFs::new(engine, rt);
    let opts = [
        MountOption::FSName("opendrive".into()),
        MountOption::AutoUnmount,
        MountOption::DefaultPermissions,
    ];
    fuser::mount2(fs, path, &opts).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_sync::PlaceholderEngine;

    #[test]
    fn attr_helpers_create_inode_and_mount_error_branches() {
        let root = root_attr();
        assert_eq!(
            (root.ino, root.kind, root.perm, root.blksize),
            (1, FileType::Directory, 0o755, 512)
        );

        let folder = attr_from(
            &NodeAttr {
                id: 7,
                name: "docs".into(),
                kind: Kind::Folder,
                size: 0,
                modified_ms: -5,
            },
            2,
        );
        assert_eq!(
            (folder.ino, folder.kind, folder.perm),
            (2, FileType::Directory, 0o755)
        );

        let file = attr_from(
            &NodeAttr {
                id: 8,
                name: "a.bin".into(),
                kind: Kind::File,
                size: 600,
                modified_ms: 1_000,
            },
            3,
        );
        assert_eq!(
            (file.ino, file.kind, file.perm, file.blocks),
            (3, FileType::RegularFile, 0o644, 2)
        );

        let eng: Arc<dyn VfsEngine> = Arc::new(PlaceholderEngine);
        let inodes = InodeTable::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            create_inode_attr(&eng, &inodes, rt.handle(), 999, OsStr::new("x"), Kind::File)
                .unwrap_err(),
            libc::ENOENT
        );
        assert_eq!(
            create_inode_attr(
                &eng,
                &inodes,
                rt.handle(),
                1,
                OsStr::new("dir"),
                Kind::Folder
            )
            .unwrap_err(),
            libc::EIO
        );

        let fe = crate::FuseFrontend::new(eng);
        let missing = std::env::temp_dir().join(format!("od-fuse-missing-{}", std::process::id()));
        let missing_str = missing.to_string_lossy().into_owned();
        let err = fe.mount(&missing_str).unwrap_err();
        assert!(err.contains("tokio"));

        let err2 = rt.block_on(async { fe.mount(&missing_str) }).unwrap_err();
        assert!(!err2.is_empty());
    }
}
