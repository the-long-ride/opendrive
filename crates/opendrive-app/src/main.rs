//! OpenDrive desktop app entrypoint.

use opendrive_auth::{store_refresh_token, AuthClient};
use opendrive_core::{Account, AccountManager, AccountStatus};
use opendrive_drive::DriveClient;
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
            let client_id = args.next().ok_or_else(|| anyhow::anyhow!("usage: authorize <client_id> <client_secret>"))?;
            let client_secret = args.next().ok_or_else(|| anyhow::anyhow!("usage: authorize <client_id> <client_secret>"))?;
            authorize_cmd(&client_id, &client_secret).await
        }
        "list-root" => {
            let token = args.next().ok_or_else(|| anyhow::anyhow!("usage: list-root <access_token>"))?;
            list_root_cmd(&token).await
        }
        "" | "help" | "--help" => {
            println!("opendrive commands:");
            println!("  --self-check");
            println!("  authorize <client_id> <client_secret>");
            println!("  list-root <access_token>");
            Ok(())
        }
        other => {
            eprintln!("unknown command: {other}");
            Ok(())
        }
    }
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
    if let Some(t) = list.next_page_token {
        println!("(next page token: {t})");
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

    let mut mgr = AccountManager::new();
    mgr.add_account(Account {
        id: acct,
        nickname: "Work".into(),
        email: "check@local".into(),
        credential_set_id: Uuid::new_v4(),
        folder_name: "Work".into(),
        read_only: true,
        enabled: true,
        status: AccountStatus::Connected,
    });

    let stats = engine.stats().await;
    println!(
        "self-check ok: entries={} accounts={} queue={}",
        page.entries.len(),
        mgr.list_accounts().len(),
        stats.queue_depth
    );
    Ok(())
}
