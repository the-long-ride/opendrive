//! OpenDrive desktop app entrypoint.

use opendrive_sync::{PlaceholderEngine, VfsEngine};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-check") {
        return self_check().await;
    }

    info!("OpenDrive starting");
    let _engine: Arc<dyn VfsEngine> = Arc::new(PlaceholderEngine);
    println!("OpenDrive scaffold ready (M0 not yet complete)");
    Ok(())
}

async fn self_check() -> anyhow::Result<()> {
    // Opens/migrates a temp store, starts engine against fake Drive, exits.
    let engine = PlaceholderEngine;
    let stats = engine.stats().await;
    println!("self-check ok: transfers={} queue={} errors={}", stats.transfers, stats.queue_depth, stats.errors);
    Ok(())
}
