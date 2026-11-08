//! OpenDrive desktop app entrypoint.

use opendrive_core::{Account, AccountManager, AccountStatus};
use opendrive_store::Store;
use opendrive_sync::{SyncEngine, VfsEngine};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-check") {
        return self_check().await;
    }

    info!("OpenDrive starting");
    let store = Store::open_in_memory().map_err(|e| anyhow::anyhow!(e))?;
    let engine = Arc::new(SyncEngine::new(store));
    let _accounts = AccountManager::new();
    let _ = engine.stats().await;
    println!("OpenDrive scaffold ready (M0 in progress)");
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
        "self-check ok: entries={} accounts={} transfers={}",
        page.entries.len(),
        mgr.list_accounts().len(),
        stats.transfers
    );
    Ok(())
}
