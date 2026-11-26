//! CLI command dispatcher and headless Explorer / Sync / Stream subcommands.

use crate::runtime::auth_runner::GOOGLE_DRIVE_API_BASE;
use crate::runtime::bridge::Bridge;
use crate::runtime::cli_self_check::run_self_check;
use crate::runtime::ui::{build_persistent_bridge, load_controller};
use opendrive_drive::DriveClient;
use std::future::Future;

pub use crate::runtime::cli_commands::{
    cmd_explore, cmd_open_item, cmd_open_item_opts, cmd_stream, cmd_stream_opts, cmd_sync_clear,
    cmd_sync_down, cmd_sync_up, resolve_account_id, resolve_remote_node,
};
pub use crate::runtime::explorer_file_ops::{cmd_cp, cmd_mv, cmd_rename, cmd_rm};

pub fn run_async<F: Future<Output = anyhow::Result<()>>>(fut: F) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(fut)
}

pub fn next_arg(args: &mut impl Iterator<Item = String>, usage: &str) -> anyhow::Result<String> {
    args.next().ok_or_else(|| anyhow::anyhow!("usage: {usage}"))
}

fn parse_flagged(
    args: impl Iterator<Item = String>,
    flag: &str,
    usage: &str,
) -> anyhow::Result<(String, String, bool)> {
    let mut has_flag = false;
    let mut pos = Vec::new();
    for a in args {
        if a == flag {
            has_flag = true;
        } else {
            pos.push(a);
        }
    }
    anyhow::ensure!(pos.len() >= 2, "usage: {usage}");
    Ok((pos.remove(0), pos.remove(0), has_flag))
}

pub fn status_cmd() -> anyhow::Result<()> {
    let ctrl = load_controller();
    println!("accounts: {}", ctrl.accounts.list_accounts().len());
    for a in ctrl.accounts.list_accounts() {
        println!("  {}  {:?}  {}  id={}", a.nickname, a.status, a.email, a.id);
    }
    if let Ok(dir) = opendrive_platform::config_dir() {
        println!("config: {}", dir.display());
    }
    Ok(())
}

pub async fn list_root_with_base(token: &str, api_base: &str) -> anyhow::Result<()> {
    let drive = DriveClient::with_base_urls(token.into(), api_base, api_base);
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

pub fn dispatch(cmd: &str, mut args: impl Iterator<Item = String>) -> anyhow::Result<()> {
    match cmd {
        "--self-check" => run_async(run_self_check()),
        "status" => status_cmd(),
        "list-root" => {
            let tok = next_arg(&mut args, "list-root <access_token>")?;
            run_async(list_root_with_base(&tok, GOOGLE_DRIVE_API_BASE))
        }
        "help" | "--help" | "-h" => {
            print!("{}", HELP_TEXT);
            Ok(())
        }
        "explore" | "sync-down" | "sync-up" | "sync-clear" | "open-item" | "stream" | "rename"
        | "rm" | "delete" | "cp" | "copy" | "mv" | "move" => dispatch_bridge_opts(
            cmd,
            args,
            &build_persistent_bridge(load_controller())?,
            true,
        ),
        other => {
            eprintln!("unknown command: {other}");
            Ok(())
        }
    }
}

pub fn dispatch_bridge(
    cmd: &str,
    args: impl Iterator<Item = String>,
    bridge: &Bridge,
) -> anyhow::Result<()> {
    dispatch_bridge_opts(cmd, args, bridge, false)
}

fn dispatch_bridge_opts(
    cmd: &str,
    mut args: impl Iterator<Item = String>,
    bridge: &Bridge,
    hold_live_stream: bool,
) -> anyhow::Result<()> {
    match cmd {
        "explore" => {
            let acct = next_arg(&mut args, "explore <account> [remote_path]")?;
            cmd_explore(bridge, &acct, args.next().as_deref())
        }
        "sync-down" => {
            let acct = next_arg(&mut args, "sync-down <account> <remote_path>")?;
            let path = next_arg(&mut args, "sync-down <account> <remote_path>")?;
            cmd_sync_down(bridge, &acct, &path)
        }
        "sync-up" => {
            let acct = next_arg(&mut args, "sync-up <account> <local_path> [remote_parent]")?;
            let local = next_arg(&mut args, "sync-up <account> <local_path> [remote_parent]")?;
            cmd_sync_up(bridge, &acct, &local, args.next().as_deref())
        }
        "sync-clear" => {
            let acct = next_arg(&mut args, "sync-clear <account> <remote_path>")?;
            let path = next_arg(&mut args, "sync-clear <account> <remote_path>")?;
            cmd_sync_clear(bridge, &acct, &path)
        }
        "open-item" => {
            let (acct, path, dry) = parse_flagged(
                args,
                "--dry-run",
                "open-item [--dry-run] <account> <remote_path>",
            )?;
            cmd_open_item_opts(bridge, &acct, &path, dry, hold_live_stream)
        }
        "stream" => {
            let (acct, path, probe) =
                parse_flagged(args, "--probe", "stream [--probe] <account> <remote_path>")?;
            cmd_stream_opts(bridge, &acct, &path, probe, hold_live_stream)
        }
        "rename" => {
            let u = "rename <account> <remote_path> <new_name>";
            let (a, p, n) = (
                next_arg(&mut args, u)?,
                next_arg(&mut args, u)?,
                next_arg(&mut args, u)?,
            );
            cmd_rename(bridge, &a, &p, &n)
        }
        "rm" | "delete" => {
            let u = "rm|delete <account> <remote_path>";
            let (a, p) = (next_arg(&mut args, u)?, next_arg(&mut args, u)?);
            cmd_rm(bridge, &a, &p)
        }
        "cp" | "copy" => {
            let u = "cp|copy <account> <remote_path> [dest_folder]";
            let (a, p) = (next_arg(&mut args, u)?, next_arg(&mut args, u)?);
            cmd_cp(bridge, &a, &p, args.next().as_deref())
        }
        "mv" | "move" => {
            let u = "mv|move <account> <remote_path> <dest_folder>";
            let (a, p, d) = (
                next_arg(&mut args, u)?,
                next_arg(&mut args, u)?,
                next_arg(&mut args, u)?,
            );
            cmd_mv(bridge, &a, &p, &d)
        }
        _ => Ok(()),
    }
}

const HELP_TEXT: &str = "\
opendrive commands:\n\
  ui (default) | tray | --self-check | status\n\
  authorize <client_id> <client_secret>\n\
  list-root <access_token> | mount <access_token> [path]\n\
  explore <account> [remote_path]\n\
  sync-down <account> <remote_path>\n\
  sync-up <account> <local_path> [remote_parent]\n\
  sync-clear <account> <remote_path>\n\
  open-item [--dry-run] <account> <remote_path>\n\
  stream [--probe] <account> <remote_path>\n\
  rename <account> <remote_path> <new_name>\n\
  rm|delete <account> <remote_path>\n\
  cp|copy <account> <remote_path> [dest_folder]\n\
  mv|move <account> <remote_path> <dest_folder>\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ui::build_bridge_in_dirs;
    use opendrive_core::{AppController, Settings};
    use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
    use std::sync::Arc;

    #[test]
    fn dispatch_status_unknown_usage_and_list_root() {
        dispatch("status", std::iter::empty()).unwrap();
        dispatch("help", std::iter::empty()).unwrap();
        dispatch("unknown-subcmd", std::iter::empty()).unwrap();
        assert!(dispatch("list-root", std::iter::empty()).is_err());

        let tmp = std::env::temp_dir().join(format!("od-cli-d-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let up = tmp.join("u.txt");
        std::fs::write(&up, b"hi").unwrap();
        let br = build_bridge_in_dirs(AppController::new(Settings::default()), None, Some(&tmp))
            .unwrap();
        for c in [
            "explore",
            "sync-down",
            "sync-up",
            "sync-clear",
            "open-item",
            "stream",
        ] {
            assert!(dispatch_bridge(c, std::iter::empty(), &br).is_err());
        }
        assert!(dispatch_bridge("other", std::iter::empty(), &br).is_ok());

        let rt = tokio::runtime::Runtime::new().unwrap();
        let drive = Arc::new(FakeDrive::new());
        drive.add_file(FakeFile {
            id: "f1".into(),
            name: "hello.txt".into(),
            mime_type: "text/plain".into(),
            size: 3,
            parents: vec!["root".into()],
            trashed: false,
            content: b"hey".into(),
            head_revision_id: Some("r1".into()),
        });
        let (addr, h) = rt.block_on(FakeDriveServer::start(drive));
        rt.block_on(list_root_with_base(
            "tok",
            &format!("http://{addr}/drive/v3"),
        ))
        .unwrap();

        let id = br
            .add_account("W".into(), "w@x.com".into(), "w".into(), false)
            .unwrap()
            .accounts[0]
            .id;
        br.engine().set_drive_client(
            id,
            opendrive_drive::DriveClient::with_base_urls(
                "t".into(),
                format!("http://{addr}/drive/v3"),
                format!("http://{addr}/upload/drive/v3"),
            ),
        );
        br.explorer_list(id, None, true).unwrap();
        for args in [
            vec!["explore", "W"],
            vec!["sync-down", "W", "hello.txt"],
            vec!["sync-up", "W", up.to_str().unwrap()],
            vec!["sync-clear", "W", "hello.txt"],
            vec!["open-item", "--dry-run", "W", "hello.txt"],
            vec!["stream", "W", "hello.txt"],
        ] {
            dispatch_bridge(args[0], args[1..].iter().map(|s| s.to_string()), &br).unwrap();
        }
        h.abort();
        drop(br);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
