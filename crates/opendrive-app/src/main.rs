//! OpenDrive desktop app entrypoint.

use opendrive_sync::{PlaceholderEngine, VfsEngine};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    info!("OpenDrive starting");
    let _engine: Arc<dyn VfsEngine> = Arc::new(PlaceholderEngine);
    println!("OpenDrive scaffold ready (M0 not yet complete)");
    Ok(())
}
