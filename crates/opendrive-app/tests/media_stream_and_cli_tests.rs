//! Integration tests for MediaStreamServer HTTP ranges, StreamCache prewarm/coalescing/read-ahead, and CLI commands.

use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::cli::{
    cmd_explore, cmd_open_item, cmd_stream, cmd_sync_clear, cmd_sync_down, cmd_sync_up, dispatch,
};
use opendrive_app::runtime::media_server::MediaStreamServer;
use opendrive_app::runtime::mount_manager::MountManager;
use opendrive_core::{OpenWithRule, Settings};
use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
use opendrive_store::{NodeState, Store};
use opendrive_sync::{NodeId, SyncEngine, VfsEngine, STREAM_CHUNK_BYTES};
use reqwest::Client;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn make_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

fn add_fake_file(drive: &FakeDrive, id: &str, name: &str, mime: &str, data: Vec<u8>) {
    drive.add_file(FakeFile {
        id: id.into(),
        name: name.into(),
        mime_type: mime.into(),
        size: data.len() as u64,
        parents: vec!["root".into()],
        trashed: false,
        content: data,
        head_revision_id: Some("rev-1".into()),
    });
}

fn dc_for(addr: std::net::SocketAddr) -> DriveClient {
    DriveClient::with_base_urls(
        "tok".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    )
}

async fn seed_engine(addr: std::net::SocketAddr, sc_dir: &Path) -> (Arc<SyncEngine>, NodeId) {
    let store = Store::open_in_memory().unwrap();
    let eng = Arc::new(
        SyncEngine::new(store)
            .with_stream_cache_dir(sc_dir)
            .unwrap(),
    );
    let acct = Uuid::new_v4();
    let root = eng.seed_root_account(acct, "work").unwrap();
    eng.set_drive_client(acct, dc_for(addr));
    let list = eng.list_explorer_nodes(acct, root, true).await.unwrap();
    let vid = list.iter().find(|n| n.name == "movie.mp4").unwrap().id as NodeId;
    (eng, vid)
}

async fn get_range(client: &Client, url: &str, range: &str) -> (u16, String, Vec<u8>) {
    let r = client.get(url).header("Range", range).send().await.unwrap();
    let code = r.status().as_u16();
    let cr = r
        .headers()
        .get("content-range")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let body = r.bytes().await.unwrap().to_vec();
    (code, cr, body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_server_range_requests_and_errors_work() {
    let tmp = std::env::temp_dir().join(format!("od-media-http-{}", Uuid::new_v4()));
    let total = (STREAM_CHUNK_BYTES as usize) + 512;
    let payload = make_bytes(total);
    let drive = Arc::new(FakeDrive::new());
    add_fake_file(&drive, "vid-1", "movie.mp4", "video/mp4", payload.clone());
    let (addr, handle) = FakeDriveServer::start(drive).await;
    let (engine, vid) = seed_engine(addr, &tmp.join("sc")).await;

    let rt = tokio::runtime::Handle::current();
    let server = MediaStreamServer::start(Arc::clone(&engine), &rt).unwrap();
    let url = server.stream_url(vid, "movie.mp4");
    let client = Client::new();

    let head = client.head(&url).send().await.unwrap();
    assert_eq!(head.status().as_u16(), 200);
    assert_eq!(head.headers()["accept-ranges"], "bytes");
    assert_eq!(head.headers()["content-type"], "video/mp4");
    assert_eq!(head.headers()["cache-control"], "no-store");
    assert_eq!(head.headers()["content-length"], total.to_string().as_str());
    assert!(head.bytes().await.unwrap().is_empty());

    let (s0, cr0, b0) = get_range(&client, &url, "bytes=0-63").await;
    assert_eq!(
        (s0, cr0.as_str(), b0.as_slice()),
        (206, format!("bytes 0-63/{total}").as_str(), &payload[..64])
    );

    let (s1, cr1, b1) = get_range(&client, &url, "bytes=2097152-").await;
    assert_eq!(s1, 206);
    assert_eq!(cr1, format!("bytes 2097152-{}/{total}", total - 1));
    assert_eq!(b1.as_slice(), &payload[2_097_152..]);

    let (s2, cr2, b2) = get_range(&client, &url, "bytes=-128").await;
    assert_eq!(s2, 206);
    assert_eq!(cr2, format!("bytes {}-{}/{total}", total - 128, total - 1));
    assert_eq!(b2.as_slice(), &payload[total - 128..]);

    assert_eq!(
        get_range(&client, &url, &format!("bytes={total}-")).await.0,
        416
    );
    let bad_tok = url.replacen("/stream/", "/stream/bad", 1);
    assert_eq!(
        client.get(bad_tok).send().await.unwrap().status().as_u16(),
        403
    );
    let no_node = url.replacen(&format!("/{vid}/"), "/999999/", 1);
    assert_eq!(
        client.get(no_node).send().await.unwrap().status().as_u16(),
        404
    );

    drop(server);
    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prewarm_head_tail_coalescing_and_read_ahead_minimize_downloads() {
    let tmp = std::env::temp_dir().join(format!("od-prewarm-{}", Uuid::new_v4()));
    let sc_dir = tmp.join("sc");
    let chunk = STREAM_CHUNK_BYTES as usize;
    let total = 3 * chunk + 256;
    let payload = make_bytes(total);
    let drive = Arc::new(FakeDrive::new());
    add_fake_file(&drive, "vid-big", "movie.mp4", "video/mp4", payload.clone());
    let (addr, handle) = FakeDriveServer::start(drive).await;
    let (engine, vid) = seed_engine(addr, &sc_dir).await;

    engine.prewarm_head_and_tail(vid).await.unwrap();
    assert_eq!(engine.stats().await.transfers, 2);
    let node_dir = sc_dir.join(vid.to_string());
    assert!(node_dir.join("0.chk").exists() && node_dir.join("3.chk").exists());
    assert!(!node_dir.join("1.chk").exists());

    let head = engine.read_cached_range(vid, 0, 128, 0).await.unwrap();
    let tail = engine
        .read_cached_range(vid, (total - 128) as u64, 128, 0)
        .await
        .unwrap();
    assert_eq!(
        (head.as_ref(), tail.as_ref()),
        (&payload[..128], &payload[total - 128..])
    );
    assert_eq!(engine.stats().await.transfers, 2);

    let c1 = chunk as u64;
    let (a, b, c) = tokio::join!(
        engine.read_cached_range(vid, c1, 64, 1),
        engine.read_cached_range(vid, c1 + 64, 64, 1),
        engine.read_cached_range(vid, c1 + 128, 64, 1),
    );
    assert_eq!(a.unwrap().as_ref(), &payload[chunk..chunk + 64]);
    assert_eq!(b.unwrap().as_ref(), &payload[chunk + 64..chunk + 128]);
    assert_eq!(c.unwrap().as_ref(), &payload[chunk + 128..chunk + 192]);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !node_dir.join("2.chk").exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(node_dir.join("2.chk").exists() && engine.stats().await.transfers == 4);

    let c2 = engine
        .read_cached_range(vid, (2 * chunk) as u64, 96, 0)
        .await
        .unwrap();
    assert_eq!(c2.as_ref(), &payload[2 * chunk..2 * chunk + 96]);
    assert_eq!(engine.stats().await.transfers, 4);

    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn cli_subcommands_and_self_check_work() {
    let tmp = std::env::temp_dir().join(format!("od-cli-{}", Uuid::new_v4()));
    let (cfg, data, mnt) = (tmp.join("cfg"), tmp.join("data"), tmp.join("mnt"));
    std::fs::create_dir_all(&cfg).unwrap();
    let up_file = tmp.join("cli-note.txt");
    std::fs::write(&up_file, b"cli-upload-body").unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let drive = Arc::new(FakeDrive::new());
    add_fake_file(
        &drive,
        "vid-cli",
        "clip.mp4",
        "video/mp4",
        make_bytes(131_072),
    );
    add_fake_file(
        &drive,
        "doc-cli",
        "spec.pdf",
        "application/pdf",
        b"pdf-data".to_vec(),
    );
    let (addr, handle) = rt.block_on(FakeDriveServer::start(Arc::clone(&drive)));

    let store = Store::open_in_memory().unwrap();
    let sc = tmp.join("sc");
    let engine = Arc::new(SyncEngine::new(store).with_stream_cache_dir(sc).unwrap());
    let (br, _rx) = Bridge::new_with_engine(Settings::default(), Arc::clone(&engine), Some(cfg));
    let mounts = Arc::new(MountManager::new_null().with_mount_root(mnt));
    let bridge = br.with_data_dir(data).with_mount_manager(mounts);
    let acct = bridge
        .add_account("Work".into(), "w@x.com".into(), "work".into(), false)
        .unwrap()
        .accounts[0]
        .id;
    engine.set_drive_client(acct, dc_for(addr));
    bridge.explorer_list(acct, None, true).unwrap();

    cmd_explore(&bridge, "Work", None).unwrap();
    cmd_sync_down(&bridge, "Work", "spec.pdf").unwrap();
    let root_id = bridge.explorer_root_node(acct).unwrap();
    let doc_id = engine.resolve_remote_path(root_id, "spec.pdf").unwrap();
    let cache_p = engine
        .store()
        .lock()
        .get_cache_path(doc_id as i64)
        .unwrap()
        .unwrap();
    assert!(Path::new(&cache_p).exists());

    cmd_sync_up(&bridge, "Work", up_file.to_str().unwrap(), None).unwrap();
    assert!(drive
        .list_children("root")
        .iter()
        .any(|f| f.name == "cli-note.txt"));

    cmd_sync_clear(&bridge, "Work", "spec.pdf").unwrap();
    assert!(!Path::new(&cache_p).exists());
    assert_eq!(
        engine
            .store()
            .lock()
            .get_node(doc_id as i64)
            .unwrap()
            .unwrap()
            .state,
        NodeState::Placeholder
    );

    let rule = OpenWithRule {
        pattern: "@video".into(),
        app_command: "mpv".into(),
    };
    bridge.update_open_with_rules(vec![rule]).unwrap();
    cmd_open_item(&bridge, "Work", "clip.mp4", true).unwrap();
    cmd_stream(&bridge, "Work", "clip.mp4", true).unwrap();
    dispatch("--self-check", std::iter::empty()).unwrap();
    dispatch("help", std::iter::empty()).unwrap();

    handle.abort();
    drop(rt);
    let _ = std::fs::remove_dir_all(&tmp);
}
