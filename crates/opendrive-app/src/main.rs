//! OpenDrive desktop app entrypoint.

use opendrive_core::{Account, AccountManager, AccountStatus};
use opendrive_sync::{PlaceholderEngine, VfsEngine};
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
    let _engine: Arc<dyn VfsEngine> = Arc::new(PlaceholderEngine);
    let _accounts = AccountManager::new();
    println!("OpenDrive scaffold ready (M0 not yet complete)");
    Ok(())
}

async fn self_check() -> anyhow::Result<()> {
    let engine = PlaceholderEngine;
    let stats = engine.stats().await;

    let mut mgr = AccountManager::new();
    let id = Uuid::new_v4();
    mgr.add_account(Account {
        id,
        nickname: "self-check".into(),
        email: "check@local".into(),
        credential_set_id: Uuid::new_v4(),
        folder_name: "self-check".into(),
        read_only: true,
        enabled: true,
        status: AccountStatus::Connected,
    });
    assert_eq!(mgr.list_accounts().len(), 1);

    println!(
        "self-check ok: transfers={} queue={} errors={} accounts={}",
        stats.transfers,
        stats.queue_depth,
        stats.errors,
        mgr.list_accounts().len()
    );
    Ok(())
}
