//! OpenDrive desktop app and CLI entrypoint.

use opendrive_app::runtime::{cli, ui};
use opendrive_auth::{store_client_secret, store_refresh_token, AuthClient};
use opendrive_core::{Account, AccountStatus, CredentialSet};
use opendrive_drive::DriveClient;
use opendrive_store::Store;
use opendrive_sync::{NodeId, SyncEngine, VfsEngine};
use std::env;
use std::path::Path;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    match cmd.as_str() {
        "" | "ui" => ui::run_desktop_ui(ui::load_controller()),
        #[cfg(feature = "tray")]
        "tray" => ui::run_tray_loop(ui::load_controller()),
        "--mount-watchdog" => {
            if let (Some(pid_s), Some(root_s)) = (args.next(), args.next()) {
                if let Ok(pid) = pid_s.parse::<u32>() {
                    let letter = args.next().unwrap_or_else(|| {
                        opendrive_core::Settings::default().virtual_drive_letter
                    });
                    opendrive_platform::wait_for_process_exit(pid);
                    opendrive_app::runtime::mount_manager::cleanup_os_mounts_for_pid(
                        pid,
                        Path::new(&root_s),
                        &letter,
                    );
                }
            }
            Ok(())
        }
        "authorize" => {
            let cid = cli::next_arg(&mut args, "authorize <client_id> <client_secret>")?;
            let csec = cli::next_arg(&mut args, "authorize <client_id> <client_secret>")?;
            cli::run_async(authorize_cmd(&cid, &csec))
        }
        "mount" => {
            let tok = cli::next_arg(&mut args, "mount <access_token> [path]")?;
            let path = args.next();
            cli::run_async(mount_cmd(&tok, path.as_deref()))
        }
        other => cli::dispatch(other, args),
    }
}

async fn authorize_cmd(client_id: &str, client_secret: &str) -> anyhow::Result<()> {
    opendrive_platform::ensure_dirs().map_err(|e| anyhow::anyhow!(e))?;
    let auth = AuthClient::new(client_id.into(), client_secret.into());
    info!("starting OAuth (browser will open)...");
    let (access, refresh) = auth
        .authorize(false)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let drive = DriveClient::new(access.clone());
    let about = drive.about().await.map_err(|e| anyhow::anyhow!(e))?;
    let account_id = Uuid::new_v4();
    let mut ctrl = ui::load_controller();
    let cred_id = ctrl
        .accounts
        .list_credentials()
        .iter()
        .find(|c| c.client_id == client_id.trim())
        .map(|c| c.id)
        .unwrap_or_else(|| {
            let id = Uuid::new_v4();
            ctrl.accounts.add_credential(CredentialSet {
                id,
                label: "Desktop OAuth".into(),
                client_id: client_id.trim().to_string(),
            });
            id
        });
    store_client_secret(cred_id, client_secret.trim()).map_err(|e| anyhow::anyhow!(e))?;
    store_refresh_token(account_id, &refresh).map_err(|e| anyhow::anyhow!(e))?;

    let folder = about.email.split('@').next().unwrap_or("drive").to_string();
    ctrl.accounts.add_account(Account {
        id: account_id,
        nickname: about.display_name.clone(),
        email: about.email.clone(),
        credential_set_id: cred_id,
        folder_name: folder,
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: about.storage_used,
        storage_quota: about.storage_quota,
        avatar_url: about.photo_url,
    });
    if let Ok(dir) = opendrive_platform::config_dir() {
        ctrl.accounts
            .save_to(&dir)
            .map_err(|e| anyhow::anyhow!(e))?;
        println!("saved account to {}", dir.display());
    }
    println!("ok email={} account_id={account_id}", about.email);
    println!("access_token={access}");
    Ok(())
}

async fn mount_cmd(token: &str, path: Option<&str>) -> anyhow::Result<()> {
    opendrive_platform::ensure_dirs().map_err(|e| anyhow::anyhow!(e))?;
    let mount_path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => opendrive_platform::mount_root().map_err(|e| anyhow::anyhow!(e))?,
    };
    std::fs::create_dir_all(&mount_path)?;
    let data = opendrive_platform::data_dir().map_err(|e| anyhow::anyhow!(e))?;
    let store =
        Store::open(data.join("meta.db").to_str().unwrap()).map_err(|e| anyhow::anyhow!(e))?;
    let engine = SyncEngine::new(store)
        .with_staging_dir(data.join("staging"))
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
    mount_platform(
        eng as Arc<dyn VfsEngine>,
        &mount_path,
        &about.display_name,
        root,
    )
    .await
}

async fn mount_platform(
    engine: Arc<dyn VfsEngine>,
    mount_path: &Path,
    account_name: &str,
    root_node: NodeId,
) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let _ = (account_name, root_node);
        info!(path = %mount_path.display(), "mounting FUSE");
        opendrive_fs_linux::mount(engine, mount_path.to_str().unwrap())
            .map_err(|e| anyhow::anyhow!(e))
    }
    #[cfg(windows)]
    {
        let mut frontend = opendrive_fs_windows::CloudFilesFrontend::new(engine);
        let root_path = mount_path.join(account_name);
        let root_str = root_path.to_string_lossy().to_string();
        let conn = frontend
            .connect_live_at(
                &root_str,
                account_name,
                root_node,
                tokio::runtime::Handle::current(),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        info!(path = %root_path.display(), "Windows CFAPI sync root connected");
        opendrive_app::runtime::mount_manager::spawn_mount_watchdog(mount_path, None);
        tokio::signal::ctrl_c().await?;
        drop(conn);
        let _ = frontend.unregister_sync_root(&root_str);
        opendrive_fs_windows::unregister_os_sync_root(&root_path);
        opendrive_platform::notify_shell_mounts_changed();
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (account_name, root_node);
        let mut frontend = opendrive_fs_macos::NfsFrontend::new(engine);
        let addr = frontend
            .serve_loopback()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let path_str = mount_path.to_string_lossy().to_string();
        frontend
            .serve_and_mount(&path_str)
            .map_err(|e| anyhow::anyhow!(e))?;
        info!(%addr, hint = %frontend.mount_command_hint(&path_str), "macOS NFS server ready");
        tokio::signal::ctrl_c().await?;
        let _ = frontend.unmount(&path_str);
        Ok(())
    }
}
