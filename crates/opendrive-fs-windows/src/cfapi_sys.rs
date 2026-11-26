//! Live Windows Cloud Files (`cloud-filter` 0.0.7) `SyncFilter` adapter and root helpers.

use crate::{ProviderBridge, SyncRootIdentity};
use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{info, ticket, Request, SyncFilter};
use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder::{Placeholder, UpdateOptions};
use cloud_filter::placeholder_create_info::PlaceholderCreateInfo;
use cloud_filter::root::{Connection, HydrationType, PopulationType, ProtectionMode};
use cloud_filter::root::{SecurityId, Session, SyncRootIdBuilder, SyncRootInfo};
use cloud_filter::utility::WriteAt;
use opendrive_sync::NodeId;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tracing::error;

const CF_BLOCK_ALIGN: u64 = 4096;
const CF_CHUNK_SIZE: u64 = 1024 * 1024;
const PLACEHOLDER_BATCH_SIZE: usize = 64;
const DEFAULT_ICON: &str = "%SystemRoot%\\system32\\shell32.dll,-16769";

pub fn connect_sync_root(
    identity: &SyncRootIdentity,
    bridge: Arc<ProviderBridge>,
    rt: tokio::runtime::Handle,
) -> Result<Connection<CfSyncFilter>, String> {
    let sid = SecurityId::current_user().map_err(|e| e.to_string())?;
    let safe_acct = crate::sanitize_sync_root_account(&identity.account_name);
    let root_id = SyncRootIdBuilder::new(&identity.provider_name)
        .user_security_id(sid)
        .account_name(&safe_acct)
        .build();
    let icon = identity.icon_resource.as_deref().unwrap_or(DEFAULT_ICON);
    let reg = root_id.is_registered().map_err(|e| e.to_string())?;
    let same_icon = root_id
        .info()
        .is_ok_and(|c| c.icon().to_string_lossy() == icon);
    if !reg || !same_icon {
        let info = SyncRootInfo::default()
            .with_display_name(&identity.account_name)
            .with_icon(icon)
            .with_version(&identity.provider_version)
            .with_path(&identity.root_path)
            .map_err(|e| e.to_string())?
            .with_hydration_type(HydrationType::Full)
            .with_population_type(PopulationType::Full)
            .with_protection_mode(ProtectionMode::Personal)
            .with_allow_pinning(true)
            .with_show_siblings_as_group(true);
        root_id.register(info).map_err(|e| e.to_string())?;
    }
    let filter = CfSyncFilter {
        bridge,
        rt,
        root_path: identity.root_path.clone(),
    };
    Session::new()
        .block_implicit_hydration()
        .connect(&identity.root_path, filter)
        .map_err(|e| e.to_string())
}

pub fn reconcile_placeholder_dirs(root_path: &Path, rel_dirs: &[PathBuf]) {
    let opts = UpdateOptions::default().has_children().mark_in_sync();
    let dirs =
        std::iter::once(root_path.to_path_buf()).chain(rel_dirs.iter().map(|r| root_path.join(r)));
    for dir in dirs {
        if dir.is_dir() {
            if let Ok(mut ph) = Placeholder::open(&dir) {
                let _ = ph.update(opts.clone(), None);
            }
        }
    }
}

pub fn dehydrate_placeholder(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(mut ph) = Placeholder::options().exclusive().open(path) else {
        return false;
    };
    let opts = UpdateOptions::default().dehydrate();
    ph.info().is_ok() && ph.update(opts, None).is_ok()
}

pub struct CfSyncFilter {
    bridge: Arc<ProviderBridge>,
    rt: tokio::runtime::Handle,
    root_path: PathBuf,
}

impl CfSyncFilter {
    fn rel_path(&self, full_path: &Path) -> PathBuf {
        if let Ok(rel) = full_path.strip_prefix(&self.root_path) {
            return rel.to_path_buf();
        }
        let root_lower = self.root_path.to_string_lossy().to_ascii_lowercase();
        let full_str = full_path.to_string_lossy();
        if full_str.to_ascii_lowercase().starts_with(&root_lower) {
            return PathBuf::from(full_str[root_lower.len()..].trim_start_matches(['\\', '/']));
        }
        full_path.to_path_buf()
    }

    fn resolve_path_node(&self, rel: &Path) -> Option<NodeId> {
        if let Some(id) = self.bridge.node_for_path(rel) {
            return Some(id);
        }
        if rel.as_os_str().is_empty() {
            return Some(self.bridge.root_node());
        }
        let (mut cur, mut built) = (self.bridge.root_node(), PathBuf::new());
        for comp in rel.components() {
            if let Component::Normal(name) = comp {
                built.push(name);
                cur = match self.bridge.node_for_path(&built) {
                    Some(cached) => cached,
                    None => {
                        let found = self.rt.block_on(self.bridge.lookup_child(cur, name));
                        let id = found.ok().flatten()?.0;
                        self.bridge.index_path(built.clone(), id);
                        id
                    }
                };
            }
        }
        Some(cur)
    }

    fn resolve_request_node(&self, request: &Request) -> Option<NodeId> {
        let blob = request.file_blob();
        if blob.len() == 8 {
            return Some(u64::from_le_bytes(blob.try_into().unwrap()));
        }
        self.resolve_path_node(&self.rel_path(&request.path()))
    }

    fn handle_in_scope_rename(&self, node_id: NodeId, src_rel: &Path, target_path: &Path) {
        let dst_rel = self.rel_path(target_path);
        let parent_rel = dst_rel.parent().unwrap_or_else(|| Path::new(""));
        let np = if parent_rel.as_os_str().is_empty() {
            Some(self.bridge.root_node())
        } else {
            self.resolve_path_node(parent_rel)
        };
        if let (Some(np), Some(nn)) = (np, target_path.file_name()) {
            let ok = self
                .rt
                .block_on(self.bridge.rename_entry(node_id, np, nn))
                .is_ok();
            if ok {
                self.bridge.remove_path(src_rel);
                self.bridge.index_path(dst_rel, node_id);
            }
        }
    }
}

fn fail_fetch(ticket: &ticket::FetchData, kind: CloudErrorKind) -> CloudErrorKind {
    let _ = ticket.fail(kind);
    kind
}

impl SyncFilter for CfSyncFilter {
    fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        let node_id = self
            .resolve_request_node(&request)
            .ok_or_else(|| fail_fetch(&ticket, CloudErrorKind::MetadataCorrupt))?;
        let range = info.required_file_range();
        if range.is_empty() {
            return Ok(());
        }
        let mut offset = range.start & !(CF_BLOCK_ALIGN - 1);
        let raw_end = (range.end + (CF_BLOCK_ALIGN - 1)) & !(CF_BLOCK_ALIGN - 1);
        let fsize = request.file_size();
        let end = if fsize > 0 {
            raw_end.min(fsize)
        } else {
            range.end
        };
        while offset < end {
            let chunk_len = ((end - offset).min(CF_CHUNK_SIZE)) as u32;
            let res = self
                .rt
                .block_on(self.bridge.fetch_data(node_id, offset, chunk_len));
            let bytes = match res {
                Ok(b) if !b.is_empty() => b,
                Ok(_) => return Err(fail_fetch(&ticket, CloudErrorKind::Unsuccessful)),
                Err(e) => {
                    error!(?e, node_id, offset, "fetch_data failed");
                    return Err(fail_fetch(&ticket, CloudErrorKind::NetworkUnavailable));
                }
            };
            ticket.write_at(&bytes, offset).map_err(|e| {
                error!(?e, node_id, offset, "cfapi write_at failed");
                fail_fetch(&ticket, CloudErrorKind::Unsuccessful)
            })?;
            offset += bytes.len() as u64;
        }
        Ok(())
    }

    fn fetch_placeholders(
        &self,
        request: Request,
        ticket: ticket::FetchPlaceholders,
        _info: info::FetchPlaceholders,
    ) -> CResult<()> {
        let parent_id = self
            .resolve_request_node(&request)
            .unwrap_or_else(|| self.bridge.root_node());
        let rel_parent = self.rel_path(&request.path());
        let entries = self
            .rt
            .block_on(self.bridge.fetch_placeholders(parent_id))
            .map_err(|e| {
                error!(?e, parent_id, "fetch_placeholders failed");
                CloudErrorKind::NetworkUnavailable
            })?;
        let mut batch: Vec<PlaceholderCreateInfo> = entries
            .into_iter()
            .map(|(name, child_id, is_dir, size)| {
                self.bridge.index_path(rel_parent.join(&name), child_id);
                let meta = if is_dir {
                    Metadata::directory()
                } else {
                    Metadata::file().size(size)
                };
                PlaceholderCreateInfo::new(&name)
                    .metadata(meta)
                    .blob(child_id.to_le_bytes().to_vec())
                    .mark_in_sync()
            })
            .collect();
        let err_u = |_| CloudErrorKind::Unsuccessful;
        if batch.is_empty() {
            ticket.pass_with_placeholders(&mut []).map_err(err_u)?;
        }
        for chunk in batch.chunks_mut(PLACEHOLDER_BATCH_SIZE) {
            ticket.pass_with_placeholders(chunk).map_err(err_u)?;
        }
        Ok(())
    }

    fn dehydrate(
        &self,
        _r: Request,
        ticket: ticket::Dehydrate,
        _i: info::Dehydrate,
    ) -> CResult<()> {
        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)
    }

    fn delete(&self, request: Request, ticket: ticket::Delete, info: info::Delete) -> CResult<()> {
        if !info.is_undelete() {
            if let Some(node_id) = self.resolve_request_node(&request) {
                let _ = self.rt.block_on(self.bridge.delete_entry(node_id));
                self.bridge.remove_path(&self.rel_path(&request.path()));
            }
        }
        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)
    }

    fn rename(&self, request: Request, ticket: ticket::Rename, info: info::Rename) -> CResult<()> {
        if info.source_in_scope() {
            if let Some(node_id) = self.resolve_request_node(&request) {
                let src_rel = self.rel_path(&request.path());
                if info.target_in_scope() {
                    self.handle_in_scope_rename(node_id, &src_rel, &info.target_path());
                } else {
                    let _ = self.rt.block_on(self.bridge.delete_entry(node_id));
                    self.bridge.remove_path(&src_rel);
                }
            }
        }
        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)
    }
}
