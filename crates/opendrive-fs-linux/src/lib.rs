//! FUSE3 frontend (Linux).

use opendrive_sync::{Kind, NodeAttr, NodeId, VfsEngine};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

#[cfg(target_os = "linux")]
mod fuse_impl {
    use super::*;
    use fuser::{
        FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData,
        ReplyDirectory, ReplyEntry, ReplyWrite, Request,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    const TTL: Duration = Duration::from_secs(1);

    pub struct FuseFs {
        engine: Arc<dyn VfsEngine>,
        ino_map: Mutex<HashMap<u64, NodeId>>,
        next_ino: Mutex<u64>,
        rt: tokio::runtime::Handle,
    }

    impl FuseFs {
        pub fn new(engine: Arc<dyn VfsEngine>, rt: tokio::runtime::Handle) -> Self {
            let mut map = HashMap::new();
            map.insert(1, 0);
            Self {
                engine,
                ino_map: Mutex::new(map),
                next_ino: Mutex::new(2),
                rt,
            }
        }

        fn alloc_ino(&self, node: NodeId) -> u64 {
            let mut map = self.ino_map.lock().unwrap();
            if let Some((&ino, _)) = map.iter().find(|(_, n)| **n == node) {
                return ino;
            }
            let mut next = self.next_ino.lock().unwrap();
            let ino = *next;
            *next += 1;
            map.insert(ino, node);
            ino
        }

        fn node_of(&self, ino: u64) -> Option<NodeId> {
            self.ino_map.lock().unwrap().get(&ino).copied()
        }

        fn attr_from(attr: &NodeAttr, ino: u64) -> FileAttr {
            let kind = match attr.kind {
                Kind::Folder => FileType::Directory,
                _ => FileType::RegularFile,
            };
            let mtime = UNIX_EPOCH + Duration::from_millis(attr.modified_ms as u64);
            FileAttr {
                ino,
                size: attr.size,
                blocks: (attr.size + 511) / 512,
                atime: mtime,
                mtime,
                ctime: mtime,
                crtime: mtime,
                kind,
                perm: if matches!(attr.kind, Kind::Folder) {
                    0o755
                } else {
                    0o644
                },
                nlink: 1,
                uid: unsafe { libc::getuid() },
                gid: unsafe { libc::getgid() },
                rdev: 0,
                flags: 0,
                blksize: 512,
            }
        }
    }

    impl Filesystem for FuseFs {
        fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
            let Some(parent_node) = self.node_of(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let eng = self.engine.clone();
            let name = name.to_os_string();
            match self.rt.block_on(async move { eng.lookup(parent_node, &name).await }) {
                Ok(Some(attr)) => {
                    let ino = self.alloc_ino(attr.id);
                    reply.entry(&TTL, &Self::attr_from(&attr, ino), 0);
                }
                Ok(None) => reply.error(libc::ENOENT),
                Err(e) => {
                    error!(?e, "lookup");
                    reply.error(libc::EIO);
                }
            }
        }

        fn getattr(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyAttr) {
            if ino == 1 {
                let now = SystemTime::now();
                reply.attr(
                    &TTL,
                    &FileAttr {
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
                    },
                );
                return;
            }
            let Some(node) = self.node_of(ino) else {
                reply.error(libc::ENOENT);
                return;
            };
            let eng = self.engine.clone();
            match self.rt.block_on(async move { eng.stat(node).await }) {
                Ok(attr) => reply.attr(&TTL, &Self::attr_from(&attr, ino)),
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
            let Some(node) = self.node_of(ino) else {
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
                        let child_ino = self.alloc_ino(e.id);
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
            let Some(node) = self.node_of(ino) else {
                reply.error(libc::ENOENT);
                return;
            };
            let eng = self.engine.clone();
            match self.rt.block_on(async move { eng.read(node, offset as u64, size).await }) {
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
            let Some(node) = self.node_of(ino) else {
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
            let Some(parent_node) = self.node_of(parent) else {
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
                    let ino = self.alloc_ino(attr.id);
                    reply.created(&TTL, &Self::attr_from(&attr, ino), 0, 0, 0);
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
            let Some(parent_node) = self.node_of(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let eng = self.engine.clone();
            let name = name.to_os_string();
            match self
                .rt
                .block_on(async move { eng.create(parent_node, &name, Kind::Folder).await })
            {
                Ok(attr) => {
                    let ino = self.alloc_ino(attr.id);
                    reply.entry(&TTL, &Self::attr_from(&attr, ino), 0);
                }
                Err(e) => {
                    error!(?e, "mkdir");
                    reply.error(libc::EIO);
                }
            }
        }

        fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
            let Some(parent_node) = self.node_of(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let eng = self.engine.clone();
            let name = name.to_os_string();
            let res = self.rt.block_on(async move {
                if let Some(attr) = eng.lookup(parent_node, &name).await? {
                    eng.remove(attr.id).await?;
                }
                Ok::<(), opendrive_sync::SyncError>(())
            });
            match res {
                Ok(()) => reply.ok(),
                Err(_) => reply.error(libc::EIO),
            }
        }

        fn rmdir(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
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
            reply: fuser::ReplyEmpty,
        ) {
            let Some(parent_node) = self.node_of(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let Some(new_parent) = self.node_of(newparent) else {
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
                    .ok_or(opendrive_sync::SyncError::NotFound)?;
                eng.rename(attr.id, new_parent, &newname).await
            });
            match res {
                Ok(()) => reply.ok(),
                Err(_) => reply.error(libc::EIO),
            }
        }
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
}

#[cfg(target_os = "linux")]
pub use fuse_impl::mount;

#[cfg(not(target_os = "linux"))]
pub fn mount(_engine: Arc<dyn VfsEngine>, _path: &str) -> Result<(), String> {
    Err("FUSE only on Linux".into())
}

pub struct FuseFrontend {
    engine: Arc<dyn VfsEngine>,
}

impl FuseFrontend {
    pub fn new(engine: Arc<dyn VfsEngine>) -> Self {
        Self { engine }
    }

    pub fn mount(&self, path: &str) -> Result<(), String> {
        mount(self.engine.clone(), path)
    }

    pub fn unmount(&self, path: &str) -> Result<(), String> {
        let _ = std::process::Command::new("fusermount3")
            .args(["-u", path])
            .status();
        Ok(())
    }
}
