//! OpenDrive desktop app entrypoint.

mod ui;

use opendrive_auth::{store_refresh_token, AuthClient};
use opendrive_core::{AppCommand, AppController, AppEvent, Settings};
use opendrive_drive::DriveClient;
use opendrive_platform;
use opendrive_store::Store;
use opendrive_sync::{SyncEngine, VfsEngine};
use std::env;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_default();

    match cmd.as_str() {
        "--self-check" => self_check().await,
        "authorize" => {
            let client_id = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: authorize <client_id> <client_secret>"))?;
            let client_secret = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: authorize <client_id> <client_secret>"))?;
            authorize_cmd(&client_id, &client_secret).await
        }
        "list-root" => {
            let token = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: list-root <access_token>"))?;
            list_root_cmd(&token).await
        }
        "status" => status_cmd(),
        "mount" => {
            let token = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: mount <access_token> [path]"))?;
            let path = args.next();
            mount_cmd(&token, path.as_deref()).await
        }
        "ui" => ui::run_ui(AppController::new(Settings::default())),
        "" | "help" | "--help" => {
            println!("opendrive commands:");
            println!("  --self-check");
            println!("  authorize <client_id> <client_secret>");
            println!("  list-root <access_token>");
            println!("  status");
            println!("  mount <access_token> [path]   # Linux FUSE");
            println!("  ui");
            Ok(())
        }
        other => {
            eprintln!("unknown command: {other}");
            Ok(())
        }
    }
}

async fn mount_cmd(token: &str, path: Option<&str>) -> anyhow::Result<()> {
    opendrive_platform::ensure_dirs().map_err(|e| anyhow::anyhow!(e))?;
    let mount_path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => opendrive_platform::mount_root().map_err(|e| anyhow::anyhow!(e))?,
    };
    std::fs::create_dir_all(&mount_path)?;

    let data = opendrive_platform::data_dir().map_err(|e| anyhow::anyhow!(e))?;
    let store = Store::open(data.join("meta.db").to_str().unwrap())
        .map_err(|e| anyhow::anyhow!(e))?;
    let staging = data.join("staging");
    let engine = SyncEngine::new(store)
        .with_staging_dir(&staging)
        .map_err(|e| anyhow::anyhow!(e))?;

    let drive = DriveClient::new(token.into());
    let about = drive.about().await.map_err(|e| anyhow::anyhow!(e))?;
    let acct = Uuid::new_v4();
    engine.set_drive_client(acct, DriveClient::new(token.into()));
    let root = engine
        .seed_root_account(acct, &about.display_name)
        .map_err(|e| anyhow::anyhow!(e))?;
    engine
        .hydrate_folder(acct, root)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let eng = Arc::new(engine);
    eng.spawn_worker(std::time::Duration::from_secs(5));
    eng.spawn_change_poller(acct, std::time::Duration::from_secs(30));

    info!(path = %mount_path.display(), "mounting FUSE");
    opendrive_fs_linux::mount(eng as Arc<dyn VfsEngine>, mount_path.to_str().unwrap())
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}

fn status_cmd() -> anyhow::Result<()> {
    let mut ctrl = AppController::new(Settings::default());
    let _ = ctrl.handle(AppCommand::AddAccount {
        nickname: "demo".into(),
    });
    println!("accounts: {}", ctrl.accounts.list_accounts().len());
    for a in ctrl.accounts.list_accounts() {
        println!("  {}  {:?}  {}", a.nickname, a.status, a.email);
    }
    Ok(())
}

async fn authorize_cmd(client_id: &str, client_secret: &str) -> anyhow::Result<()> {
    let auth = AuthClient::new(client_id.into(), client_secret.into());
    info!("starting OAuth (browser will open)...");
    let (access, refresh) = auth.authorize(false).await.map_err(|e| anyhow::anyhow!(e))?;
    let drive = DriveClient::new(access.clone());
    let about = drive.about().await.map_err(|e| anyhow::anyhow!(e))?;
    let account_id = Uuid::new_v4();
    store_refresh_token(account_id, &refresh).map_err(|e| anyhow::anyhow!(e))?;
    println!("ok email={} account_id={account_id}", about.email);
    println!("access_token={access}");
    Ok(())
}

async fn list_root_cmd(token: &str) -> anyhow::Result<()> {
    let drive = DriveClient::new(token.into());
    let about = drive.about().await.map_err(|e| anyhow::anyhow!(e))?;
    println!("account: {} ({})", about.display_name, about.email);
    let list = drive
        .list_files("root", None, true)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    for f in list.files {
        println!("  {}  {}", f.id, f.name);
    }
    Ok(())
}

async fn self_check() -> anyhow::Result<()> {
    let store = Store::open_in_memory().map_err(|e| anyhow::anyhow!(e))?;
    let engine = SyncEngine::new(store);
    let acct = Uuid::new_v4();
    engine
        .seed_root_account(acct, "Work")
        .map_err(|e| anyhow::anyhow!(e))?;
    let page = engine.list(0, None).await.map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(page.entries.len(), 1);
    let mut ctrl = AppController::new(Settings::default());
    let ev = ctrl.handle(AppCommand::AddAccount {
        nickname: "Work".into(),
    });
    assert!(matches!(ev[0], AppEvent::AccountUpdated(_)));
    println!(
        "self-check ok: entries={} accounts={}",
        page.entries.len(),
        ctrl.accounts.list_accounts().len()
    );
    Ok(())
}
