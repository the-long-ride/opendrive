//! Linux `fuser::Filesystem` implementation backed by `InodeTable`.

use crate::fuse_helpers::{attr_from, create_dir_entry, root_attr, TTL};
use crate::inode_table::InodeTable;
use fuser::{
    FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyWrite, Request,
};
use opendrive_sync::{Kind, SyncError, VfsEngine};
use std::ffi::OsStr;
use std::sync::Arc;
use tracing::{debug, error};

pub struct FuseFs {
    engine: Arc<dyn VfsEngine>,
    inodes: InodeTable,
    rt: tokio::runtime::Handle,
}

impl FuseFs {
    pub fn new(engine: Arc<dyn VfsEngine>, rt: tokio::runtime::Handle) -> Self {
        Self {
            engine,
            inodes: InodeTable::new(),
            rt,
        }
    }
}

impl Filesystem for FuseFs {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let Some(parent_node) = self.inodes.node_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        let name = name.to_os_string();
        match self
            .rt
            .block_on(async move { eng.lookup(parent_node, &name).await })
        {
            Ok(Some(attr)) => {
                let ino = self.inodes.alloc_ino(attr.id);
                reply.entry(&TTL, &attr_from(&attr, ino), 0);
            }
            Ok(None) => reply.error(libc::ENOENT),
            Err(e) => {
                error!(?e, "lookup");
                reply.error(libc::EIO);
            }
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        if ino == 1 {
            reply.attr(&TTL, &root_attr());
            return;
        }
        let Some(node) = self.inodes.node_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        match self.rt.block_on(async move { eng.stat(node).await }) {
            Ok(attr) => reply.attr(&TTL, &attr_from(&attr, ino)),
            Err(_) => reply.error(libc::ENOENT),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let Some(node) = self.inodes.node_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        match self.rt.block_on(async move { eng.list(node, None).await }) {
            Ok(page) => {
                let mut entries: Vec<(u64, FileType, String)> = vec![
                    (ino, FileType::Directory, ".".into()),
                    (1, FileType::Directory, "..".into()),
                ];
                for e in page.entries {
                    let child_ino = self.inodes.alloc_ino(e.id);
                    let ft = match e.kind {
                        Kind::Folder => FileType::Directory,
                        _ => FileType::RegularFile,
                    };
                    entries.push((child_ino, ft, e.name));
                }
                for (i, (cino, ft, name)) in entries.into_iter().enumerate().skip(offset as usize) {
                    if reply.add(cino, (i + 1) as i64, ft, name) {
                        break;
                    }
                }
                reply.ok();
            }
            Err(e) => {
                error!(?e, "readdir");
                reply.error(libc::EIO);
            }
        }
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let Some(node) = self.inodes.node_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        match self
            .rt
            .block_on(async move { eng.read(node, offset as u64, size).await })
        {
            Ok(data) => reply.data(&data),
            Err(e) => {
                debug!(?e, "read");
                reply.error(libc::EIO);
            }
        }
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        let Some(node) = self.inodes.node_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        let data = data.to_vec();
        match self
            .rt
            .block_on(async move { eng.write(node, offset as u64, &data).await })
        {
            Ok(n) => reply.written(n),
            Err(e) => {
                error!(?e, "write");
                reply.error(libc::EIO);
            }
        }
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        let Some(parent_node) = self.inodes.node_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        let name = name.to_os_string();
        match self
            .rt
            .block_on(async move { eng.create(parent_node, &name, Kind::File).await })
        {
            Ok(attr) => {
                let ino = self.inodes.alloc_ino(attr.id);
                reply.created(&TTL, &attr_from(&attr, ino), 0, 0, 0);
            }
            Err(e) => {
                error!(?e, "create");
                reply.error(libc::EIO);
            }
        }
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        create_dir_entry(
            &self.engine,
            &self.inodes,
            &self.rt,
            parent,
            name,
            Kind::Folder,
            reply,
        );
    }

    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let Some(parent_node) = self.inodes.node_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        let name = name.to_os_string();
        let res = self.rt.block_on(async move {
            if let Some(attr) = eng.lookup(parent_node, &name).await? {
                eng.remove(attr.id).await?;
            }
            Ok::<(), SyncError>(())
        });
        match res {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn rmdir(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.unlink(req, parent, name, reply);
    }

    fn rename(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        let Some(parent_node) = self.inodes.node_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let Some(new_parent) = self.inodes.node_of(newparent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let eng = self.engine.clone();
        let name = name.to_os_string();
        let newname = newname.to_os_string();
        let res = self.rt.block_on(async move {
            let attr = eng
                .lookup(parent_node, &name)
                .await?
                .ok_or(SyncError::NotFound)?;
            eng.rename(attr.id, new_parent, &newname).await
        });
        match res {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(libc::EIO),
        }
    }
}
