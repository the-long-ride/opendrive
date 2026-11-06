//! Live Windows Cloud Files (`cloud-filter` 0.0.7) `SyncFilter` adapter.

use crate::{ProviderBridge, SyncRootIdentity};
use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{info, ticket, Request, SyncFilter};
use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder::{Placeholder, UpdateOptions};
use cloud_filter::placeholder_create_info::PlaceholderCreateInfo;
use cloud_filter::root::{
    Connection, HydrationType, PopulationType, ProtectionMode, SecurityId, Session,
    SyncRootIdBuilder, SyncRootInfo,
};
use cloud_filter::utility::WriteAt;
use opendrive_sync::NodeId;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tracing::error;

const CF_BLOCK_ALIGN: u64 = 4096;
const CF_CHUNK_SIZE: u64 = 1024 * 1024;
const PLACEHOLDER_BATCH_SIZE: usize = 64;

pub struct CfSyncFilter {
    bridge: Arc<ProviderBridge>,
    rt: tokio::runtime::Handle,
    root_path: PathBuf,
}

impl CfSyncFilter {
    pub fn new(
        bridge: Arc<ProviderBridge>,
        rt: tokio::runtime::Handle,
        root_path: PathBuf,
    ) -> Self {
        Self {
            bridge,
            rt,
            root_path,
        }
    }

    fn rel_path(&self, full_path: &Path) -> PathBuf {
        if let Ok(rel) = full_path.strip_prefix(&self.root_path) {
            return rel.to_path_buf();
        }
        let root_lower = self.root_path.to_string_lossy().to_ascii_lowercase();
        let full_str = full_path.to_string_lossy();
        if full_str.to_ascii_lowercase().starts_with(&root_lower) {
            let trimmed = full_str[root_lower.len()..].trim_start_matches(['\\', '/']);
            return PathBuf::from(trimmed);
        }
        full_path.to_path_buf()
    }

    fn resolve_path_node(&self, rel: &Path) -> Option<NodeId> {
        if rel.as_os_str().is_empty() {
            return Some(
                self.bridge
                    .node_for_path(rel)
                    .unwrap_or_else(|| self.bridge.root_node()),
            );
        }
        if let Some(id) = self.bridge.node_for_path(rel) {
            return Some(id);
        }
        let mut cur = self.bridge.root_node();
        let mut built = PathBuf::new();
        for comp in rel.components() {
            if let Component::Normal(name) = comp {
                built.push(name);
                if let Some(cached) = self.bridge.node_for_path(&built) {
                    cur = cached;
                    continue;
                }
                let found = self
                    .rt
                    .block_on(self.bridge.lookup_child(cur, name))
                    .ok()
                    .flatten()?;
                cur = found.0;
                self.bridge.index_path(built.clone(), cur);
            }
        }
        Some(cur)
    }

    fn resolve_request_node(&self, request: &Request) -> Option<NodeId> {
        let blob = request.file_blob();
        if blob.len() == 8 {
            return Some(u64::from_le_bytes(blob.try_into().unwrap()));
        }
        let rel = self.rel_path(&request.path());
        self.resolve_path_node(&rel)
    }
}

impl SyncFilter for CfSyncFilter {
    fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        let Some(node_id) = self.resolve_request_node(&request) else {
            let _ = ticket.fail(CloudErrorKind::MetadataCorrupt);
            return Err(CloudErrorKind::MetadataCorrupt);
        };
        let range = info.required_file_range();
        if range.is_empty() {
            return Ok(());
        }
        let start = range.start & !(CF_BLOCK_ALIGN - 1);
        let raw_end = (range.end + (CF_BLOCK_ALIGN - 1)) & !(CF_BLOCK_ALIGN - 1);
        let file_size = request.file_size();
        let end = if file_size > 0 {
            raw_end.min(file_size)
        } else {
            range.end
        };

        let mut offset = start;
        while offset < end {
            let chunk_len = ((end - offset).min(CF_CHUNK_SIZE)) as u32;
            let bytes = match self
                .rt
                .block_on(self.bridge.fetch_data(node_id, offset, chunk_len))
            {
                Ok(b) if !b.is_empty() => b,
                Ok(_) => {
                    let _ = ticket.fail(CloudErrorKind::Unsuccessful);
                    return Err(CloudErrorKind::Unsuccessful);
                }
                Err(e) => {
                    error!(?e, node_id, offset, "fetch_data failed");
                    let _ = ticket.fail(CloudErrorKind::NetworkUnavailable);
                    return Err(CloudErrorKind::NetworkUnavailable);
                }
            };
            ticket.write_at(&bytes, offset).map_err(|e| {
                error!(?e, node_id, offset, "cfapi write_at failed");
                let _ = ticket.fail(CloudErrorKind::Unsuccessful);
                CloudErrorKind::Unsuccessful
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

        if batch.is_empty() {
            ticket
                .pass_with_placeholders(&mut [])
                .map_err(|_| CloudErrorKind::Unsuccessful)?;
        } else {
            for chunk in batch.chunks_mut(PLACEHOLDER_BATCH_SIZE) {
                ticket
                    .pass_with_placeholders(chunk)
                    .map_err(|_| CloudErrorKind::Unsuccessful)?;
            }
        }
        Ok(())
    }

    fn dehydrate(
        &self,
        _request: Request,
        ticket: ticket::Dehydrate,
        _info: info::Dehydrate,
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
                    let target_path = info.target_path();
                    let dst_rel = self.rel_path(&target_path);
                    let parent_rel = dst_rel.parent().unwrap_or_else(|| Path::new(""));
                    let new_parent = if parent_rel.as_os_str().is_empty() {
                        Some(self.bridge.root_node())
                    } else {
                        self.resolve_path_node(parent_rel)
                    };
                    if let (Some(new_parent), Some(new_name)) =
                        (new_parent, target_path.file_name())
                    {
                        if self
                            .rt
                            .block_on(self.bridge.rename_entry(node_id, new_parent, new_name))
                            .is_ok()
                        {
                            self.bridge.remove_path(&src_rel);
                            self.bridge.index_path(dst_rel, node_id);
                        }
                    }
                } else {
                    let _ = self.rt.block_on(self.bridge.delete_entry(node_id));
                    self.bridge.remove_path(&src_rel);
                }
            }
        }
        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)
    }
}

pub fn connect_sync_root(
    identity: &SyncRootIdentity,
    bridge: Arc<ProviderBridge>,
    rt: tokio::runtime::Handle,
) -> Result<Connection<CfSyncFilter>, String> {
    let sid = SecurityId::current_user().map_err(|e| e.to_string())?;
    let root_id = SyncRootIdBuilder::new(&identity.provider_name)
        .user_security_id(sid)
        .account_name(&identity.account_name)
        .build();

    let icon = identity
        .icon_resource
        .as_deref()
        .unwrap_or("%SystemRoot%\\system32\\shell32.dll,-16769");
    let needs_register = !root_id.is_registered().map_err(|e| e.to_string())?
        || root_id
            .info()
            .is_ok_and(|cur| cur.icon().to_string_lossy() != icon);
    if needs_register {
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

    let filter = CfSyncFilter::new(bridge, rt, identity.root_path.clone());
    Session::new()
        .block_implicit_hydration()
        .connect(&identity.root_path, filter)
        .map_err(|e| e.to_string())
}

pub fn reconcile_placeholder_dirs(root_path: &Path, rel_dirs: &[PathBuf]) {
    for rel in std::iter::once(PathBuf::new()).chain(rel_dirs.iter().cloned()) {
        let dir = if rel.as_os_str().is_empty() {
            root_path.to_path_buf()
        } else {
            root_path.join(&rel)
        };
        if !dir.is_dir() {
            continue;
        }
        if let Ok(mut ph) = Placeholder::open(&dir) {
            let _ = ph.update(UpdateOptions::default().has_children().mark_in_sync(), None);
        }
    }
}
