//! Explorer / Sync / Stream CLI subcommand implementations and node/account resolvers.

use crate::design::formatters::{format_bytes, format_modified_date};
use crate::runtime::bridge::Bridge;
use crate::runtime::cli::run_async;
use opendrive_sync::NodeId;
use std::path::PathBuf;
use uuid::Uuid;

pub fn resolve_account_id(bridge: &Bridge, selector: &str) -> anyhow::Result<Uuid> {
    let accounts = bridge.state_snapshot().accounts;
    if let Ok(id) = Uuid::parse_str(selector.trim()) {
        if accounts.iter().any(|a| a.id == id) {
            return Ok(id);
        }
    }
    accounts
        .iter()
        .find(|a| {
            a.nickname.eq_ignore_ascii_case(selector)
                || a.email.eq_ignore_ascii_case(selector)
                || a.folder_name.eq_ignore_ascii_case(selector)
        })
        .map(|a| a.id)
        .ok_or_else(|| anyhow::anyhow!("account not found: {selector}"))
}

pub fn resolve_remote_node(
    bridge: &Bridge,
    account_id: Uuid,
    remote_path: &str,
) -> anyhow::Result<NodeId> {
    let root = bridge
        .explorer_root_node(account_id)
        .map_err(anyhow::Error::msg)?;
    let trimmed = remote_path.trim().trim_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed == "." {
        return Ok(root);
    }
    if let Some(id) = bridge.engine().resolve_remote_path(root, trimmed) {
        return Ok(id);
    }
    let mut cur = root;
    for seg in trimmed
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
    {
        let (_, entries) = bridge
            .explorer_list(account_id, Some(cur), false)
            .map_err(anyhow::Error::msg)?;
        let hit = entries
            .iter()
            .find(|e| e.name == seg)
            .or_else(|| entries.iter().find(|e| e.name.eq_ignore_ascii_case(seg)))
            .ok_or_else(|| anyhow::anyhow!("remote path segment not found: {seg}"))?;
        cur = hit.id;
    }
    Ok(cur)
}

pub fn cmd_explore(bridge: &Bridge, acct: &str, remote_path: Option<&str>) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let folder = remote_path
        .map(|p| resolve_remote_node(bridge, acct_id, p))
        .transpose()?;
    let (node_id, entries) = bridge
        .explorer_list(acct_id, folder, false)
        .map_err(anyhow::Error::msg)?;
    println!("folder_node={node_id} entries={}", entries.len());
    for e in entries {
        let kind = if e.is_folder { "DIR " } else { "FILE" };
        println!(
            "  [{kind}] {:<32} {:>10}  {:?}  {}",
            e.name,
            format_bytes(e.size),
            e.state,
            format_modified_date(e.modified_ms)
        );
    }
    Ok(())
}

pub fn cmd_sync_down(bridge: &Bridge, acct: &str, remote_path: &str) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let (sum, path) = bridge
        .explorer_sync_down(acct_id, node_id)
        .map_err(anyhow::Error::msg)?;
    println!(
        "sync-down ok: files={} bytes={} path={}",
        sum.downloaded_files,
        sum.bytes_transferred,
        path.map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".into())
    );
    Ok(())
}

pub fn cmd_sync_up(
    bridge: &Bridge,
    acct: &str,
    local_path: &str,
    remote_parent: Option<&str>,
) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let parent = resolve_remote_node(bridge, acct_id, remote_parent.unwrap_or("/"))?;
    let sum = bridge
        .explorer_sync_up(acct_id, parent, &[PathBuf::from(local_path)])
        .map_err(anyhow::Error::msg)?;
    println!(
        "sync-up ok: files={} bytes={}",
        sum.uploaded_files, sum.bytes_transferred
    );
    Ok(())
}

pub fn cmd_sync_clear(bridge: &Bridge, acct: &str, remote_path: &str) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let sum = bridge
        .explorer_sync_up_and_clear(acct_id, node_id)
        .map_err(anyhow::Error::msg)?;
    println!(
        "sync-clear ok: uploaded={} cleared={}",
        sum.uploaded_files, sum.cleared_files
    );
    Ok(())
}

pub fn cmd_open_item_opts(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    dry_run: bool,
    hold_stream: bool,
) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let res = bridge
        .explorer_open_item(acct_id, node_id, dry_run)
        .map_err(anyhow::Error::msg)?;
    println!(
        "app={} target={} stream={}",
        res.app_command.as_deref().unwrap_or("default"),
        res.target,
        res.stream_url.as_deref().unwrap_or("-")
    );
    if !dry_run && hold_stream && res.is_stream_url {
        println!("serving local stream (press Ctrl+C to stop)...");
        run_async(async {
            let _ = tokio::signal::ctrl_c().await;
            Ok(())
        })?;
    }
    Ok(())
}

pub fn cmd_open_item(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    cmd_open_item_opts(bridge, acct, remote_path, dry_run, false)
}

pub fn cmd_stream_opts(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    probe: bool,
    hold_until_ctrl_c: bool,
) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let url = bridge
        .explorer_stream_url(acct_id, node_id)
        .map_err(anyhow::Error::msg)?;
    println!("stream_url={url}");
    if probe {
        run_async(async move {
            let resp = reqwest::Client::new()
                .get(&url)
                .header("Range", "bytes=0-65535")
                .send()
                .await?;
            let status = resp.status().as_u16();
            let len = resp.bytes().await?.len();
            println!("probe status={status} bytes={len}");
            anyhow::ensure!(status == 206, "expected 206 Partial Content, got {status}");
            Ok(())
        })?;
    } else if hold_until_ctrl_c {
        println!("serving local stream (press Ctrl+C to stop)...");
        run_async(async {
            let _ = tokio::signal::ctrl_c().await;
            Ok(())
        })?;
    }
    Ok(())
}

pub fn cmd_stream(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    probe: bool,
) -> anyhow::Result<()> {
    cmd_stream_opts(bridge, acct, remote_path, probe, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrive_core::Settings;
    use opendrive_store::Store;
    use opendrive_sync::SyncEngine;
    use std::sync::Arc;

    #[test]
    fn resolve_account_and_remote_node_paths() {
        let store = Store::open_in_memory().unwrap();
        let eng = Arc::new(SyncEngine::new(store));
        let (br, _) = Bridge::new_with_engine(Settings::default(), eng, None);
        let id = br
            .add_account("Work".into(), "w@e.com".into(), "work".into(), false)
            .unwrap()
            .accounts[0]
            .id;
        assert_eq!(resolve_account_id(&br, &id.to_string()).unwrap(), id);
        assert_eq!(resolve_account_id(&br, "w@e.com").unwrap(), id);
        assert_eq!(resolve_account_id(&br, "work").unwrap(), id);
        assert!(resolve_account_id(&br, &Uuid::new_v4().to_string()).is_err());
        assert!(resolve_account_id(&br, "missing").is_err());
        let root = br.explorer_root_node(id).unwrap();
        let mut n = br
            .engine()
            .store()
            .lock()
            .get_node(root as i64)
            .unwrap()
            .unwrap();
        (
            n.id,
            n.parent_id,
            n.drive_id,
            n.kind,
            n.name,
            n.display_name,
        ) = (
            0,
            Some(root as i64),
            "d1".into(),
            "file".into(),
            "Note.txt".into(),
            "Note.txt".into(),
        );
        let nid = br.engine().store().lock().upsert_node(&n).unwrap() as u64;
        assert_eq!(resolve_remote_node(&br, id, ".").unwrap(), root);
        assert_eq!(resolve_remote_node(&br, id, "/").unwrap(), root);
        assert_eq!(resolve_remote_node(&br, id, "note.txt").unwrap(), nid);
        assert!(resolve_remote_node(&br, id, "missing.txt").is_err());
        assert!(cmd_open_item(&br, "work", "missing.txt", true).is_err());
        assert!(cmd_stream(&br, "work", "missing.txt", false).is_err());
    }
}
