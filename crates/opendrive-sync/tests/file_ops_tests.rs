//! Cloud & Local delete, rename, copy, and move integration tests on SyncEngine + FakeDrive.

use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{SyncEngine, SHARED_WITH_ME_DRIVE_ID};
use std::sync::Arc;
use uuid::Uuid;

const FDIR: &str = "application/vnd.google-apps.folder";

fn setup_fake_file(drive: &FakeDrive, id: &str, name: &str, mime: &str, parent: &str, data: &[u8]) {
    drive.add_file(FakeFile {
        id: id.into(),
        name: name.into(),
        mime_type: mime.into(),
        size: data.len() as u64,
        parents: vec![parent.into()],
        content: data.to_vec(),
        head_revision_id: Some("rev-1".into()),
        ..Default::default()
    });
}

#[tokio::test]
async fn cloud_and_local_delete_rename_copy_move_and_guards() {
    let tmp = std::env::temp_dir().join(format!("od-fops-{}", Uuid::new_v4()));
    let (mount, cache) = (tmp.join("mnt"), tmp.join("cache"));
    std::fs::create_dir_all(&mount).unwrap();
    let drive = Arc::new(FakeDrive::new());
    setup_fake_file(&drive, "d-src", "Source", FDIR, "root", &[]);
    setup_fake_file(&drive, "d-dst", "Archive", FDIR, "root", &[]);
    setup_fake_file(&drive, "d-sub", "Nested", FDIR, "d-src", &[]);
    setup_fake_file(
        &drive,
        "f-cloud",
        "cloud.txt",
        "text/plain",
        "root",
        b"cloud-body",
    );
    setup_fake_file(
        &drive,
        "f-deep",
        "deep.txt",
        "text/plain",
        "d-sub",
        b"deep-body",
    );
    setup_fake_file(
        &drive,
        "f-loc",
        "local.txt",
        "text/plain",
        "root",
        b"local-body",
    );
    let (addr, srv) = FakeDriveServer::start(Arc::clone(&drive)).await;

    let acct_id = Uuid::new_v4();
    let eng = SyncEngine::new(Store::open_in_memory().unwrap());
    eng.set_drive_client(
        acct_id,
        DriveClient::with_base_urls(
            "tok".into(),
            format!("http://{addr}/drive/v3"),
            format!("http://{addr}/upload/drive/v3"),
        ),
    );
    let root = eng.seed_root_account(acct_id, "A").unwrap();
    let top = eng.list_explorer_nodes(acct_id, root, true).await.unwrap();
    let id_of = |nodes: &[opendrive_store::Node], n: &str| {
        nodes.iter().find(|x| x.name == n).unwrap().id as u64
    };
    assert!(!top.iter().any(|x| x.name == "Shared with me"));
    let src_id = id_of(&top, "Source");
    let dst_id = id_of(&top, "Archive");
    let f_cloud = id_of(&top, "cloud.txt");
    let f_loc = id_of(&top, "local.txt");
    let swm_id = eng.seed_shared_with_me_root(acct_id).unwrap();
    let acct_mnt = mount.join("A");
    std::fs::create_dir_all(&acct_mnt).unwrap();

    // Sync down local.txt so it becomes Local (Cached + on disk + cache_files).
    let (_, loc_path) = eng
        .sync_down_node(acct_id, root, f_loc, Some(&acct_mnt), &cache)
        .await
        .unwrap();
    let loc_path = loc_path.unwrap();
    assert!(loc_path.exists());
    assert_eq!(
        eng.store()
            .lock()
            .get_node(f_loc as i64)
            .unwrap()
            .unwrap()
            .state,
        NodeState::Cached
    );

    // 1. Guards: cannot mutate root / Shared with me or invalid names or cycle moves.
    assert!(eng
        .rename_node_everywhere(acct_id, root, root, "X", Some(&acct_mnt))
        .await
        .is_err());
    assert!(eng
        .delete_node_everywhere(acct_id, root, swm_id, Some(&acct_mnt))
        .await
        .is_err());
    assert!(eng
        .rename_node_everywhere(acct_id, root, f_cloud, "bad/name", None)
        .await
        .is_err());
    let src_children = eng
        .list_explorer_nodes(acct_id, src_id, false)
        .await
        .unwrap();
    let nested_id = id_of(&src_children, "Nested");
    assert!(eng
        .move_node_everywhere(acct_id, root, src_id, nested_id, Some(&acct_mnt))
        .await
        .is_err());
    assert!(eng
        .copy_node_everywhere(acct_id, root, src_id, nested_id, Some(&acct_mnt), &cache)
        .await
        .is_err());
    assert!(eng
        .move_node_everywhere(acct_id, root, f_cloud, swm_id, None)
        .await
        .is_err());
    assert_eq!(SHARED_WITH_ME_DRIVE_ID, "sharedWithMe");

    // 2. Rename Cloud & Local files -> Drive + SQLite + disk + cache_files all update.
    let ren_c = eng
        .rename_node_everywhere(acct_id, root, f_cloud, "cloud-ren.txt", Some(&acct_mnt))
        .await
        .unwrap();
    assert_eq!(ren_c.name, "cloud-ren.txt");
    assert_eq!(drive.get_file("f-cloud").unwrap().name, "cloud-ren.txt");

    let ren_l = eng
        .rename_node_everywhere(acct_id, root, f_loc, "local-ren.txt", Some(&acct_mnt))
        .await
        .unwrap();
    assert_eq!(ren_l.name, "local-ren.txt");
    let renamed_disk = acct_mnt.join("local-ren.txt");
    assert!(!loc_path.exists() && renamed_disk.exists());
    assert_eq!(
        eng.store()
            .lock()
            .get_cache_path(f_loc as i64)
            .unwrap()
            .as_deref(),
        Some(renamed_disk.to_string_lossy().as_ref())
    );

    // 3. Copy Cloud file (disambiguated in same folder) & Local file (copies bytes on disk too!).
    let cp_c = eng
        .copy_node_everywhere(acct_id, root, f_cloud, root, Some(&acct_mnt), &cache)
        .await
        .unwrap();
    assert_eq!(cp_c.name, "cloud-ren (copy).txt");
    assert_eq!(cp_c.state, NodeState::Placeholder);
    assert!(drive
        .list_children("root")
        .iter()
        .any(|f| f.name == "cloud-ren (copy).txt"));

    let cp_l = eng
        .copy_node_everywhere(acct_id, root, f_loc, root, Some(&acct_mnt), &cache)
        .await
        .unwrap();
    assert_eq!(cp_l.name, "local-ren (copy).txt");
    assert_eq!(cp_l.state, NodeState::Cached);
    assert_eq!(
        std::fs::read(acct_mnt.join("local-ren (copy).txt")).unwrap(),
        b"local-body"
    );

    // 4. Copy unhydrated Cloud folder subtree into Archive -> copies nested folder & file remotely!
    let cp_dir = eng
        .copy_node_everywhere(acct_id, root, src_id, dst_id, Some(&acct_mnt), &cache)
        .await
        .unwrap();
    assert_eq!(cp_dir.name, "Source");
    let archived = eng
        .list_explorer_nodes(acct_id, cp_dir.id as u64, false)
        .await
        .unwrap();
    let cp_nested = id_of(&archived, "Nested");
    let cp_deep = eng
        .list_explorer_nodes(acct_id, cp_nested, false)
        .await
        .unwrap();
    assert!(cp_deep
        .iter()
        .any(|n| n.name == "deep.txt" && !n.drive_id.is_empty()));

    // 5. Move both Cloud copy & Local file into Archive -> remote parents, SQLite, disk & cache move!
    let mv_c = eng
        .move_node_everywhere(acct_id, root, cp_c.id as u64, dst_id, Some(&acct_mnt))
        .await
        .unwrap();
    assert_eq!(mv_c.parent_id, Some(dst_id as i64));
    assert!(drive
        .list_children("d-dst")
        .iter()
        .any(|f| f.id == cp_c.drive_id));
    assert!(!drive
        .list_children("root")
        .iter()
        .any(|f| f.id == cp_c.drive_id));

    let mv_l = eng
        .move_node_everywhere(acct_id, root, f_loc, dst_id, Some(&acct_mnt))
        .await
        .unwrap();
    assert_eq!(mv_l.parent_id, Some(dst_id as i64));
    let moved_disk = acct_mnt.join("Archive").join("local-ren.txt");
    assert!(!renamed_disk.exists() && moved_disk.exists());
    assert_eq!(
        eng.store()
            .lock()
            .get_cache_path(f_loc as i64)
            .unwrap()
            .as_deref(),
        Some(moved_disk.to_string_lossy().as_ref())
    );

    // 6. Delete Cloud file, Local file, and Folder subtree -> Drive trashed + disk/cache/children gone.
    eng.delete_node_everywhere(acct_id, root, f_cloud, Some(&acct_mnt))
        .await
        .unwrap();
    assert!(drive.get_file("f-cloud").unwrap().trashed);
    eng.delete_node_everywhere(acct_id, root, f_loc, Some(&acct_mnt))
        .await
        .unwrap();
    assert!(!moved_disk.exists());
    assert!(eng
        .store()
        .lock()
        .get_cache_path(f_loc as i64)
        .unwrap()
        .is_none());
    eng.delete_node_everywhere(acct_id, root, dst_id, Some(&acct_mnt))
        .await
        .unwrap();
    assert!(drive.get_file("d-dst").unwrap().trashed);
    let final_root = eng.list_explorer_nodes(acct_id, root, false).await.unwrap();
    assert!(!final_root
        .iter()
        .any(|n| n.name == "cloud-ren.txt" || n.name == "Archive"));

    srv.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}
