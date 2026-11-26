//! Integration tests for Drive v3 shortcuts, sharedWithMe queries, and avatar photo download.

use opendrive_drive::{DriveClient, DriveError};
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use std::sync::Arc;

#[tokio::test]
async fn shared_with_me_shortcuts_and_avatar_photo_fetch() {
    let drive = Arc::new(FakeDrive::new());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let client = DriveClient::with_base_urls(
        "tok-123".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );

    let photo_url = format!("http://{addr}/assets/avatar.png");
    drive.set_static_asset("/assets/avatar.png", b"png-bytes".to_vec());
    drive.set_photo_url(Some(photo_url.clone()));
    drive.add_file(FakeFile {
        id: "shortcut-1".into(),
        name: "Team Folder".into(),
        mime_type: "application/vnd.google-apps.shortcut".into(),
        size: 0,
        parents: vec!["root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-1".into()),
    });
    drive.set_shortcut(
        "shortcut-1",
        "shared-target",
        "application/vnd.google-apps.folder",
    );
    drive.add_file(FakeFile {
        id: "shared-item".into(),
        name: "Shared Notes".into(),
        mime_type: "application/vnd.google-apps.folder".into(),
        size: 0,
        parents: vec!["ext-root".into()],
        trashed: false,
        content: vec![],
        head_revision_id: Some("rev-2".into()),
    });
    drive.mark_shared_with_me("shared-item");

    let about = client.about().await.unwrap();
    assert_eq!(about.photo_url.as_deref(), Some(photo_url.as_str()));
    assert_eq!(
        client.download_bytes(&photo_url).await.unwrap(),
        b"png-bytes"
    );
    assert!(matches!(
        client
            .download_bytes(&format!("http://{addr}/assets/missing.png"))
            .await,
        Err(DriveError::NotFound)
    ));

    let listed = client.list_files("root", None, true).await.unwrap();
    assert_eq!(
        listed.files[0].shortcut_target_id.as_deref(),
        Some("shared-target")
    );
    assert_eq!(
        listed.files[0].shortcut_target_mime.as_deref(),
        Some("application/vnd.google-apps.folder")
    );

    drive.set_root_real_id("0AF-drive-root-1");
    drive.add_file(FakeFile {
        id: "loose-owned".into(),
        name: "Loose.zip".into(),
        mime_type: "application/zip".into(),
        size: 12,
        parents: vec![],
        head_revision_id: Some("r1".into()),
        ..Default::default()
    });
    drive.add_file(FakeFile {
        id: "anyone-link".into(),
        name: "SharedLink.zip".into(),
        mime_type: "application/zip".into(),
        size: 20,
        parents: vec![],
        head_revision_id: Some("r1".into()),
        ..Default::default()
    });
    drive.mark_unowned("anyone-link");

    assert_eq!(client.root_folder_id().await.unwrap(), "0AF-drive-root-1");
    let unparented = client.list_top_unparented_files().await.unwrap();
    let loose = unparented.iter().find(|f| f.id == "loose-owned").unwrap();
    assert_eq!(loose.owned_by_me, Some(true));
    assert_eq!(loose.shared_with_me, Some(false));
    let link = unparented.iter().find(|f| f.id == "anyone-link").unwrap();
    assert_eq!(link.owned_by_me, Some(false));
    assert_eq!(link.shared_with_me, Some(false));

    let shared = client.list_shared_with_me(None).await.unwrap();
    assert_eq!(shared.files.len(), 1);
    assert_eq!(shared.files[0].id, "shared-item");
    handle.abort();
}
