//! Coverage tests for `opendrive-fs-macos` frontend, backend errors, and NFSv3 VFS edge cases.

use nfsserve::nfs::{filename3, nfsstat3, sattr3};
use nfsserve::vfs::NFSFileSystem;
use opendrive_fs_macos::{bind_loopback_nfs, NfsBackend, NfsFrontend};
use opendrive_store::Store;
use opendrive_sync::{PlaceholderEngine, SyncEngine};
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn frontend_default_mount_path_hint_and_sync_serve_and_mount() {
    let base = std::env::temp_dir().join(format!("od-mac-cov-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let blocker = base.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();

    let mut frontend = NfsFrontend::new(Arc::new(PlaceholderEngine));
    assert!(frontend.backend().is_some());
    assert!(frontend.mount_command_hint("/tmp/od").contains("PORT"));
    assert_eq!(
        NfsFrontend::default_mount_path()
            .file_name()
            .and_then(|s| s.to_str()),
        Some("OpenDrive")
    );
    let prev_home = std::env::var_os("HOME");
    std::env::remove_var("HOME");
    assert_eq!(
        NfsFrontend::default_mount_path(),
        std::path::Path::new(".").join("OpenDrive")
    );
    if let Some(h) = prev_home {
        std::env::set_var("HOME", h);
    }
    assert!(frontend
        .serve_and_mount(blocker.join("child").to_str().unwrap())
        .is_err());

    let mnt = base.join("mnt");
    frontend.serve_and_mount(mnt.to_str().unwrap()).unwrap();
    assert!(frontend.is_mounted());
    assert!(frontend.listen_addr().is_some());
    frontend.unmount(mnt.to_str().unwrap()).unwrap();
    assert!(!frontend.is_mounted());

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn frontend_serve_loopback_idempotent_and_bad_fh_errors() {
    let mut frontend = NfsFrontend::new(Arc::new(PlaceholderEngine));
    let addr1 = frontend.serve_loopback().await.unwrap();
    let addr2 = frontend.serve_loopback().await.unwrap();
    assert_eq!(addr1, addr2);
    frontend.unmount("/tmp/unused").unwrap();

    let staging = std::env::temp_dir().join(format!("od-mac-badfh-{}", Uuid::new_v4()));
    let engine = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&staging)
        .unwrap();
    let backend = NfsBackend::new(Arc::new(engine));

    assert!(backend.lookup(999, "a").await.is_err());
    assert!(backend.getattr(999).await.is_err());
    assert!(backend.readdir(999).await.is_err());
    assert!(backend.read(999, 0, 4).await.is_err());
    assert!(backend.write(999, 0, b"a").await.is_err());
    assert!(backend.create(999, "a").await.is_err());
    assert!(backend.mkdir(999, "d").await.is_err());
    assert!(backend.remove(999, "a").await.is_err());
    assert!(backend.rename(999, "a", 1, "b").await.is_err());
    assert!(backend.rename(1, "a", 999, "b").await.is_err());
    assert!(backend.rename(1, "missing", 1, "b").await.is_err());

    let _ = std::fs::remove_dir_all(&staging);
}

#[tokio::test]
async fn nfs_vfs_invalid_utf8_pagination_exclusive_and_bind_loopback() {
    let staging = std::env::temp_dir().join(format!("od-mac-vfs2-{}", Uuid::new_v4()));
    let engine = SyncEngine::new(Store::open_in_memory().unwrap())
        .with_staging_dir(&staging)
        .unwrap();
    let backend = NfsBackend::new(Arc::new(engine));
    let vfs: &dyn NFSFileSystem = &backend;

    let bad_utf8: filename3 = [0xffu8, 0xfeu8].as_slice().into();
    assert!(matches!(
        vfs.lookup(1, &bad_utf8).await,
        Err(nfsstat3::NFS3ERR_INVAL)
    ));
    assert!(matches!(
        vfs.lookup(1, &b"missing.txt".as_slice().into()).await,
        Err(nfsstat3::NFS3ERR_NOENT)
    ));
    assert!(matches!(
        vfs.getattr(999).await,
        Err(nfsstat3::NFS3ERR_NOENT)
    ));

    let f1 = vfs
        .create_exclusive(1, &b"one.txt".as_slice().into())
        .await
        .unwrap();
    let (f2, _) = vfs
        .create(1, &b"two.txt".as_slice().into(), sattr3::default())
        .await
        .unwrap();
    vfs.write(f1, 0, b"abc").await.unwrap();

    // Zero count and past-EOF reads.
    assert_eq!(vfs.read(f1, 0, 0).await.unwrap(), (Vec::new(), true));
    assert_eq!(vfs.read(f1, 99, 4).await.unwrap(), (Vec::new(), true));

    // Readdir pagination with max_entries=0, start_after=f1, and unknown start_after.
    let p0 = vfs.readdir(1, 0, 0).await.unwrap();
    assert!(p0.entries.is_empty() && !p0.end);
    let p1 = vfs.readdir(1, f1, 10).await.unwrap();
    assert_eq!(p1.entries.len(), 1);
    assert_eq!(p1.entries[0].fileid, f2);
    assert!(p1.end);
    let p_end = vfs.readdir(1, 99999, 10).await.unwrap();
    assert!(p_end.entries.is_empty() && p_end.end);

    // Bad dir / missing targets map to NFS3ERR_NOENT.
    assert!(matches!(
        vfs.create(999, &b"x.txt".as_slice().into(), sattr3::default())
            .await,
        Err(nfsstat3::NFS3ERR_NOENT)
    ));
    assert!(matches!(
        vfs.write(999, 0, b"a").await,
        Err(nfsstat3::NFS3ERR_NOENT)
    ));
    assert!(matches!(
        vfs.remove(1, &b"missing.txt".as_slice().into()).await,
        Err(nfsstat3::NFS3ERR_NOENT)
    ));

    let (addr, handle) = bind_loopback_nfs(backend, "od-test").await.unwrap();
    let conn = tokio::net::TcpStream::connect(addr).await.unwrap();
    drop(conn);
    handle.abort();

    let _ = std::fs::remove_dir_all(&staging);
}
