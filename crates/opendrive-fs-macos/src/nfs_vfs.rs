//! `nfsserve` 0.11 `NFSFileSystem` implementation and localhost TCP binder.

use crate::nfs_backend::NfsBackend;
use async_trait::async_trait;
use nfsserve::nfs::{
    fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfstime3, sattr3, specdata3,
};
use nfsserve::tcp::{NFSTcp, NFSTcpListener};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tracing::error;

fn name_str(name: &filename3) -> Result<&str, nfsstat3> {
    std::str::from_utf8(name.as_ref()).map_err(|_| nfsstat3::NFS3ERR_INVAL)
}

fn to_nfstime3(modified_ms: i64) -> nfstime3 {
    let ms = modified_ms.max(0) as u64;
    nfstime3 {
        seconds: (ms / 1000) as u32,
        nseconds: ((ms % 1000) * 1_000_000) as u32,
    }
}

fn to_fattr3(fileid: fileid3, is_dir: bool, size: u64, modified_ms: i64) -> fattr3 {
    let ts = to_nfstime3(modified_ms);
    fattr3 {
        ftype: if is_dir {
            ftype3::NF3DIR
        } else {
            ftype3::NF3REG
        },
        mode: if is_dir { 0o755 } else { 0o644 },
        nlink: if is_dir { 2 } else { 1 },
        uid: 0,
        gid: 0,
        size,
        used: size,
        rdev: specdata3::default(),
        fsid: 1,
        fileid,
        atime: ts,
        mtime: ts,
        ctime: ts,
    }
}

fn map_err(err: &str) -> nfsstat3 {
    if err.contains("bad fh") || err.contains("not found") || err.contains("NotFound") {
        nfsstat3::NFS3ERR_NOENT
    } else {
        nfsstat3::NFS3ERR_IO
    }
}

#[async_trait]
impl NFSFileSystem for NfsBackend {
    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    fn root_dir(&self) -> fileid3 {
        1
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let name = name_str(filename)?;
        match self.lookup(dirid, name).await {
            Ok(Some((fh, _, _))) => Ok(fh),
            Ok(None) => Err(nfsstat3::NFS3ERR_NOENT),
            Err(e) => Err(map_err(&e)),
        }
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        let (is_dir, size, modified_ms) = self.getattr_meta(id).await.map_err(|e| map_err(&e))?;
        Ok(to_fattr3(id, is_dir, size, modified_ms))
    }

    async fn setattr(&self, id: fileid3, _setattr: sattr3) -> Result<fattr3, nfsstat3> {
        <Self as NFSFileSystem>::getattr(self, id).await
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        let (_, size, _) = self.getattr_meta(id).await.map_err(|e| map_err(&e))?;
        if offset >= size || count == 0 {
            return Ok((Vec::new(), true));
        }
        let bytes = self
            .read(id, offset, count)
            .await
            .map_err(|e| map_err(&e))?;
        let eof = offset + bytes.len() as u64 >= size || (bytes.len() as u32) < count;
        Ok((bytes.to_vec(), eof))
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        self.write(id, offset, data)
            .await
            .map_err(|e| map_err(&e))?;
        <Self as NFSFileSystem>::getattr(self, id).await
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        _attr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let name = name_str(filename)?;
        let (fh, is_dir, size) = self.create(dirid, name).await.map_err(|e| map_err(&e))?;
        Ok((fh, to_fattr3(fh, is_dir, size, 0)))
    }

    async fn create_exclusive(
        &self,
        dirid: fileid3,
        filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        let name = name_str(filename)?;
        if let Ok(Some(_)) = self.lookup(dirid, name).await {
            return Err(nfsstat3::NFS3ERR_EXIST);
        }
        let (fh, _, _) = self.create(dirid, name).await.map_err(|e| map_err(&e))?;
        Ok(fh)
    }

    async fn mkdir(
        &self,
        dirid: fileid3,
        dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let name = name_str(dirname)?;
        let (fh, is_dir, size) = self.mkdir(dirid, name).await.map_err(|e| map_err(&e))?;
        Ok((fh, to_fattr3(fh, is_dir, size, 0)))
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let name = name_str(filename)?;
        self.remove(dirid, name).await.map_err(|e| map_err(&e))
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        let from_name = name_str(from_filename)?;
        let to_name = name_str(to_filename)?;
        self.rename(from_dirid, from_name, to_dirid, to_name)
            .await
            .map_err(|e| map_err(&e))
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        let all = self.readdir_meta(dirid).await.map_err(|e| map_err(&e))?;
        let start_idx = if start_after == 0 {
            0
        } else {
            all.iter()
                .position(|(_, fh, _, _, _)| *fh == start_after)
                .map(|idx| idx + 1)
                .unwrap_or(all.len())
        };
        let remaining = &all[start_idx..];
        let end = remaining.len() <= max_entries;
        let entries = remaining
            .iter()
            .take(max_entries)
            .map(|(name, fh, is_dir, size, modified_ms)| DirEntry {
                fileid: *fh,
                name: name.as_bytes().into(),
                attr: to_fattr3(*fh, *is_dir, *size, *modified_ms),
            })
            .collect();
        Ok(ReadDirResult { entries, end })
    }

    async fn symlink(
        &self,
        _dirid: fileid3,
        _linkname: &filename3,
        _symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        Err(nfsstat3::NFS3ERR_NOTSUPP)
    }

    async fn readlink(&self, _id: fileid3) -> Result<nfspath3, nfsstat3> {
        Err(nfsstat3::NFS3ERR_NOTSUPP)
    }
}

pub async fn bind_loopback_nfs(
    backend: NfsBackend,
    export_name: &str,
) -> Result<(SocketAddr, JoinHandle<()>), String> {
    let mut listener = NFSTcpListener::bind("127.0.0.1:0", backend)
        .await
        .map_err(|e| e.to_string())?;
    listener.with_export_name(export_name);
    let addr = SocketAddr::new(listener.get_listen_ip(), listener.get_listen_port());
    let handle = tokio::spawn(async move {
        if let Err(e) = listener.handle_forever().await {
            error!(?e, "NFS TCP listener stopped");
        }
    });
    Ok((addr, handle))
}
