//! Bridge and CLI file manipulation operations (`delete`, `rename`, `copy`, `move`) across Cloud & Local.

use crate::runtime::bridge::Bridge;
use crate::runtime::bridge_types::str_err;
use crate::runtime::cli_commands::{resolve_account_id, resolve_remote_node};
use crate::runtime::explorer_helpers::{block_on_rt, node_to_entry};
use crate::runtime::explorer_ops::ExplorerEntry;
use opendrive_sync::NodeId;
use uuid::Uuid;

impl Bridge {
    pub(super) fn ensure_writable_account(&self, account_id: Uuid) -> Result<(), String> {
        let guard = self.ctrl.lock().expect("controller lock");
        let acct = guard
            .accounts
            .get_account(account_id)
            .map_err(|_| "account not found".to_string())?;
        if acct.read_only {
            return Err("account is read-only".into());
        }
        Ok(())
    }

    fn sync_mount_placeholders(&self, account_id: Uuid, root_node: NodeId) {
        if self.mounts.is_account_mounted(account_id) {
            let rel_dirs = self.engine.placeholder_folder_rel_paths(root_node);
            self.mounts
                .reconcile_account_placeholders(account_id, &rel_dirs);
        }
    }

    pub fn explorer_delete_item(&self, account_id: Uuid, node_id: NodeId) -> Result<(), String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        block_on_rt(
            rt.handle(),
            self.engine.delete_node_everywhere(
                account_id,
                root_node,
                node_id,
                mounted_root.as_deref(),
            ),
        )
        .map_err(str_err)?;
        self.sync_mount_placeholders(account_id, root_node);
        Ok(())
    }

    pub fn explorer_rename_item(
        &self,
        account_id: Uuid,
        node_id: NodeId,
        new_name: &str,
    ) -> Result<ExplorerEntry, String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        let node = block_on_rt(
            rt.handle(),
            self.engine.rename_node_everywhere(
                account_id,
                root_node,
                node_id,
                new_name,
                mounted_root.as_deref(),
            ),
        )
        .map_err(str_err)?;
        self.sync_mount_placeholders(account_id, root_node);
        Ok(node_to_entry(node))
    }

    pub fn explorer_move_item(
        &self,
        account_id: Uuid,
        node_id: NodeId,
        dest_folder_node: NodeId,
    ) -> Result<ExplorerEntry, String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        let node = block_on_rt(
            rt.handle(),
            self.engine.move_node_everywhere(
                account_id,
                root_node,
                node_id,
                dest_folder_node,
                mounted_root.as_deref(),
            ),
        )
        .map_err(str_err)?;
        self.sync_mount_placeholders(account_id, root_node);
        Ok(node_to_entry(node))
    }

    pub fn explorer_copy_item(
        &self,
        account_id: Uuid,
        node_id: NodeId,
        dest_folder_node: NodeId,
    ) -> Result<ExplorerEntry, String> {
        self.ensure_writable_account(account_id)?;
        let rt = self.mounts.ensure_runtime()?;
        self.ensure_account_drive_client(account_id, rt.handle())?;
        let root_node = self.explorer_root_node(account_id)?;
        let mounted_root = self.mounts.mounted_account_path(account_id);
        let cache_dir = self.cache_root_dir();
        let node = block_on_rt(
            rt.handle(),
            self.engine.copy_node_everywhere(
                account_id,
                root_node,
                node_id,
                dest_folder_node,
                mounted_root.as_deref(),
                &cache_dir,
            ),
        )
        .map_err(str_err)?;
        self.sync_mount_placeholders(account_id, root_node);
        Ok(node_to_entry(node))
    }
}

pub fn cmd_rename(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    new_name: &str,
) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let entry = bridge
        .explorer_rename_item(acct_id, node_id, new_name)
        .map_err(anyhow::Error::msg)?;
    println!("rename ok: id={} name={}", entry.id, entry.name);
    Ok(())
}

pub fn cmd_rm(bridge: &Bridge, acct: &str, remote_path: &str) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    bridge
        .explorer_delete_item(acct_id, node_id)
        .map_err(anyhow::Error::msg)?;
    println!("rm ok: node={node_id}");
    Ok(())
}

pub fn cmd_cp(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    dest_folder: Option<&str>,
) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let dest_id = match dest_folder {
        Some(p) => resolve_remote_node(bridge, acct_id, p)?,
        None => {
            let root = bridge
                .explorer_root_node(acct_id)
                .map_err(anyhow::Error::msg)?;
            bridge
                .engine()
                .get_node(node_id)?
                .and_then(|n| n.parent_id)
                .filter(|p| *p > 0)
                .map(|p| p as NodeId)
                .unwrap_or(root)
        }
    };
    let entry = bridge
        .explorer_copy_item(acct_id, node_id, dest_id)
        .map_err(anyhow::Error::msg)?;
    println!("cp ok: id={} name={}", entry.id, entry.name);
    Ok(())
}

pub fn cmd_mv(
    bridge: &Bridge,
    acct: &str,
    remote_path: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    let acct_id = resolve_account_id(bridge, acct)?;
    let node_id = resolve_remote_node(bridge, acct_id, remote_path)?;
    let dest_id = resolve_remote_node(bridge, acct_id, dest_folder)?;
    let entry = bridge
        .explorer_move_item(acct_id, node_id, dest_id)
        .map_err(anyhow::Error::msg)?;
    println!("mv ok: id={} name={}", entry.id, entry.name);
    Ok(())
}
