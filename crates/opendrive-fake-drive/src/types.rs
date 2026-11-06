//! Data structures for `opendrive-fake-drive`.

use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct FakeFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub parents: Vec<String>,
    pub trashed: bool,
    pub content: Vec<u8>,
    pub head_revision_id: Option<String>,
}

impl FakeFile {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "mimeType": self.mime_type,
            "size": self.size.to_string(),
            "parents": self.parents,
            "trashed": self.trashed,
            "headRevisionId": self.head_revision_id.as_deref().unwrap_or("rev-1"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FakeSharedDrive {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FakeAbout {
    pub email: String,
    pub display_name: String,
    pub storage_quota: u64,
    pub storage_used: u64,
}

impl Default for FakeAbout {
    fn default() -> Self {
        Self {
            email: "user@example.com".into(),
            display_name: "Test User".into(),
            storage_quota: 15 * 1024 * 1024 * 1024,
            storage_used: 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FakeChange {
    pub token: u64,
    pub file_id: String,
    pub removed: bool,
    pub file: Option<FakeFile>,
}

#[derive(Debug, Clone)]
pub struct UploadSession {
    pub id: String,
    pub file_id: Option<String>,
    pub name: String,
    pub mime_type: String,
    pub parents: Vec<String>,
    pub buffer: Vec<u8>,
}

use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct FakeExtras {
    pub photo_url: Option<String>,
    pub shared_with_me: HashSet<String>,
    pub shortcuts: HashMap<String, (String, String)>,
    pub assets: HashMap<String, Vec<u8>>,
}

pub fn file_to_json(f: &FakeFile, extras: &FakeExtras) -> Value {
    let mut obj = f.to_json();
    if let (Some(map), Some((tid, tmime))) = (obj.as_object_mut(), extras.shortcuts.get(&f.id)) {
        map.insert(
            "shortcutDetails".into(),
            json!({ "targetId": tid, "targetMimeType": tmime }),
        );
    }
    obj
}
pub(crate) fn bump_rev(prev: Option<&str>) -> String {
    let n = prev
        .and_then(|s| s.strip_prefix("rev-"))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    format!("rev-{}", n + 1)
}
