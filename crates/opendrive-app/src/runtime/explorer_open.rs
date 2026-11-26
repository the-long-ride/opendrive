//! Explorer item open resolution, stream `.m3u` playlist writer, and video open routing.

use crate::runtime::bridge::Bridge;
use crate::runtime::bridge_types::str_err;
use crate::runtime::explorer_helpers::{is_regular_cached_file, node_to_entry};
use crate::runtime::explorer_ops::ResolvedOpenTarget;
use opendrive_core::{resolve_open_app, supports_http_stream_arg, MediaKind};
use opendrive_drive::DriveClient;
use opendrive_store::NodeState;
use opendrive_sync::NodeId;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn write_stream_playlist(
    cache_dir: &Path,
    node_id: NodeId,
    name: &str,
    url: &str,
) -> Result<PathBuf, String> {
    let dir = cache_dir.join("streams");
    std::fs::create_dir_all(&dir).map_err(str_err)?;
    let path = dir.join(format!("stream-{node_id}.m3u"));
    let body = format!("#EXTM3U\n#EXTINF:-1,{name}\n{url}\n");
    std::fs::write(&path, body).map_err(str_err)?;
    Ok(path)
}

impl Bridge {
    #[allow(clippy::too_many_arguments)]
    fn resolve_video_target(
        &self,
        account_id: Uuid,
        node_id: NodeId,
        name: &str,
        app_command: Option<String>,
        cached: Option<PathBuf>,
        local_p: Option<PathBuf>,
    ) -> Result<ResolvedOpenTarget, String> {
        if let Some(p) = cached.filter(|p| p.metadata().is_ok_and(|m| m.len() > 0)) {
            return Ok(ResolvedOpenTarget {
                app_command,
                target: p.to_string_lossy().into_owned(),
                is_stream_url: false,
                stream_url: None,
            });
        }
        let mounted = self.mounts.is_account_mounted(account_id)
            && local_p.as_ref().is_some_and(|p| p.exists());
        if app_command.is_some() && !supports_http_stream_arg(app_command.as_deref()) {
            let path = if mounted {
                let _ = self.explorer_stream_url(account_id, node_id);
                local_p.expect("exists")
            } else {
                self.explorer_sync_down(account_id, node_id)?
                    .1
                    .ok_or("sync-down produced no local file")?
            };
            return Ok(ResolvedOpenTarget {
                app_command,
                target: path.to_string_lossy().into_owned(),
                is_stream_url: false,
                stream_url: None,
            });
        }
        let url = self.explorer_stream_url(account_id, node_id)?;
        let (target, is_stream_url) = if supports_http_stream_arg(app_command.as_deref()) {
            (url.clone(), true)
        } else if mounted {
            (
                local_p.expect("exists").to_string_lossy().into_owned(),
                false,
            )
        } else {
            let m3u = write_stream_playlist(&self.cache_root_dir(), node_id, name, &url)?;
            (m3u.to_string_lossy().into_owned(), true)
        };
        Ok(ResolvedOpenTarget {
            app_command,
            target,
            is_stream_url,
            stream_url: Some(url),
        })
    }

    pub fn explorer_open_item(
        &self,
        account_id: Uuid,
        node_id: NodeId,
        dry_run: bool,
    ) -> Result<ResolvedOpenTarget, String> {
        let node = self.get_store_node(node_id)?;
        if node.is_folder() {
            return Err("cannot open a folder as a file".into());
        }
        let cache_p = self
            .engine
            .store()
            .lock()
            .get_cache_path(node_id as i64)
            .map_err(str_err)?
            .map(PathBuf::from);
        let entry = node_to_entry(node.clone());
        let rules = self
            .ctrl
            .lock()
            .expect("controller lock")
            .settings
            .open_with_rules
            .clone();
        let app_command =
            resolve_open_app(&rules, &entry.name, entry.mime.as_deref()).map(str::to_string);
        let root_node = self.explorer_root_node(account_id)?;
        let local_p = self.local_node_path(account_id, root_node, node_id);
        let cached = (node.state != NodeState::Placeholder)
            .then(|| {
                cache_p
                    .filter(|p| is_regular_cached_file(p, node.size))
                    .or_else(|| {
                        local_p
                            .clone()
                            .filter(|p| is_regular_cached_file(p, node.size))
                    })
            })
            .flatten();

        let resolved = if entry.is_native_doc {
            if entry.drive_id.is_empty() {
                return Err("native doc has no Drive ID".into());
            }
            ResolvedOpenTarget {
                app_command,
                target: DriveClient::native_doc_link(&entry.drive_id),
                is_stream_url: false,
                stream_url: None,
            }
        } else if entry.media_kind == MediaKind::Video {
            self.resolve_video_target(
                account_id,
                node_id,
                &entry.name,
                app_command,
                cached,
                local_p,
            )?
        } else {
            let path = match cached {
                Some(p) => p,
                None => self
                    .explorer_sync_down(account_id, node_id)?
                    .1
                    .ok_or("sync-down produced no local file")?,
            };
            ResolvedOpenTarget {
                app_command,
                target: path.to_string_lossy().into_owned(),
                is_stream_url: false,
                stream_url: None,
            }
        };
        if !dry_run {
            opendrive_platform::launch_with_app(resolved.app_command.as_deref(), &resolved.target)
                .map_err(str_err)?;
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ui::build_bridge_in_dirs;
    use opendrive_core::{AppController, Settings};
    use opendrive_fake_drive::{FakeDrive, FakeDriveServer, FakeFile};
    use opendrive_store::Node;
    use std::sync::Arc;

    #[test]
    fn video_resolution_playlist_and_open_errors() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = std::env::temp_dir().join(format!("od-open-unit-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let blocker = tmp.join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(write_stream_playlist(&blocker, 1, "v.mp4", "http://127.0.0.1/v").is_err());

        let drive = Arc::new(FakeDrive::new());
        drive.add_file(FakeFile {
            id: "vid1".into(),
            name: "clip.mp4".into(),
            mime_type: "video/mp4".into(),
            size: 4,
            parents: vec!["root".into()],
            trashed: false,
            content: b"data".into(),
            head_revision_id: Some("r1".into()),
        });
        let (addr, _h) = rt.block_on(FakeDriveServer::start(drive));
        let cfg = Settings {
            cache_dir: tmp.join("cache").to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let br = build_bridge_in_dirs(AppController::new(cfg), None, Some(&tmp)).unwrap();
        let acct = br
            .add_account("U".into(), "u@x.com".into(), "u".into(), false)
            .unwrap()
            .accounts[0]
            .id;
        br.engine().set_drive_client(
            acct,
            DriveClient::with_base_urls(
                "t".into(),
                format!("http://{addr}/drive/v3"),
                format!("http://{addr}/upload/drive/v3"),
            ),
        );
        let root = br.explorer_root_node(acct).unwrap();
        assert!(br.explorer_open_item(acct, root, true).is_err());

        let list = br.explorer_list(acct, Some(root), true).unwrap();
        let vid = list.1.iter().find(|e| e.name == "clip.mp4").unwrap().id;
        // Uncached video without app_command -> writes .m3u playlist and marks is_stream_url
        let pl = br.explorer_open_item(acct, vid, true).unwrap();
        assert!(pl.is_stream_url && pl.target.ends_with(".m3u"));
        assert!(Path::new(&pl.target).exists());

        // Cached video path non-empty -> returns local cached file directly
        let cached_vid = tmp.join("cached_clip.mp4");
        std::fs::write(&cached_vid, b"1234").unwrap();
        let res = br
            .resolve_video_target(acct, vid, "clip.mp4", None, Some(cached_vid.clone()), None)
            .unwrap();
        assert!(!res.is_stream_url && res.target.contains("cached_clip.mp4"));

        // Native doc without drive_id -> error
        let nd_id = br.engine().store().lock().next_id().unwrap();
        br.engine()
            .store()
            .lock()
            .upsert_node(&Node {
                id: nd_id,
                account_id: acct,
                drive_id: String::new(),
                parent_id: Some(root as i64),
                name: "Doc".into(),
                display_name: "Doc".into(),
                kind: "native_doc".into(),
                mime: Some("application/vnd.google-apps.document".into()),
                size: 0,
                md5: None,
                modified_ms: 0,
                head_revision: None,
                trashed: false,
                shared_drive_id: None,
                state: NodeState::Placeholder,
                pinned: false,
                last_access_ms: 0,
            })
            .unwrap();
        assert!(br.explorer_open_item(acct, nd_id as u64, true).is_err());
        drop(br);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
