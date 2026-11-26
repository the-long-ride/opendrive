use crate::types::file_to_json;
use crate::{FakeDrive, FakeFile};

impl FakeDrive {
    pub fn set_photo_url(&self, url: Option<String>) {
        self.extras.lock().unwrap().photo_url = url;
    }

    pub fn photo_url(&self) -> Option<String> {
        self.extras.lock().unwrap().photo_url.clone()
    }

    pub fn mark_shared_with_me(&self, id: &str) {
        self.extras.lock().unwrap().shared_with_me.insert(id.into());
    }

    pub fn list_shared_with_me(&self) -> Vec<FakeFile> {
        let ex = self.extras.lock().unwrap();
        let mut out: Vec<_> = self
            .files
            .lock()
            .unwrap()
            .values()
            .filter(|f| ex.shared_with_me.contains(&f.id) && !f.trashed)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn set_shortcut(&self, id: &str, target_id: &str, target_mime: &str) {
        self.extras
            .lock()
            .unwrap()
            .shortcuts
            .insert(id.into(), (target_id.into(), target_mime.into()));
    }

    pub fn set_static_asset(&self, path: &str, bytes: Vec<u8>) {
        self.extras
            .lock()
            .unwrap()
            .assets
            .insert(path.into(), bytes);
    }

    pub fn get_static_asset(&self, path: &str) -> Option<Vec<u8>> {
        self.extras.lock().unwrap().assets.get(path).cloned()
    }

    pub fn mark_unowned(&self, id: &str) {
        self.extras.lock().unwrap().unowned.insert(id.into());
    }

    pub fn set_root_real_id(&self, real_id: &str) {
        self.extras.lock().unwrap().root_real_id = Some(real_id.into());
    }

    pub fn root_real_id(&self) -> Option<String> {
        self.extras.lock().unwrap().root_real_id.clone()
    }

    pub fn list_not_in_root(&self) -> Vec<FakeFile> {
        let root_real = self.root_real_id();
        let mut out: Vec<_> = self
            .files
            .lock()
            .unwrap()
            .values()
            .filter(|f| {
                !f.trashed
                    && f.id != "root"
                    && Some(f.id.as_str()) != root_real.as_deref()
                    && !f
                        .parents
                        .iter()
                        .any(|p| p == "root" || Some(p.as_str()) == root_real.as_deref())
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn file_to_json(&self, f: &FakeFile) -> serde_json::Value {
        file_to_json(f, &self.extras.lock().unwrap())
    }

    pub fn copy_file(
        &self,
        id: &str,
        name: Option<&str>,
        parents: Option<Vec<String>>,
    ) -> Option<FakeFile> {
        let src = self.get_file(id)?;
        if src.mime_type == "application/vnd.google-apps.folder" {
            return None;
        }
        let mut seq = self.next_seq.lock().unwrap();
        let new_id = format!("file-{}", *seq);
        *seq += 1;
        drop(seq);
        let copy = FakeFile {
            id: new_id.clone(),
            name: name.map(str::to_string).unwrap_or(src.name),
            mime_type: src.mime_type,
            size: src.size,
            parents: parents.filter(|p| !p.is_empty()).unwrap_or(src.parents),
            trashed: false,
            content: src.content,
            head_revision_id: Some("rev-1".into()),
        };
        self.files
            .lock()
            .unwrap()
            .insert(new_id.clone(), copy.clone());
        self.record_change(new_id, false, Some(copy.clone()));
        Some(copy)
    }

    pub fn patch_parents(
        &self,
        id: &str,
        add_parents: Option<&str>,
        remove_parents: Option<&str>,
        name: Option<&str>,
    ) -> Option<FakeFile> {
        let root_real = self.root_real_id();
        let updated = {
            let mut files = self.files.lock().unwrap();
            let f = files.get_mut(id)?;
            if let Some(rm) = remove_parents {
                let rms: Vec<&str> = rm.split(',').filter(|s| !s.is_empty()).collect();
                f.parents.retain(|p| {
                    !rms.iter().any(|r| {
                        *r == p
                            || (*r == "root" && root_real.as_deref() == Some(p.as_str()))
                            || (p == "root" && root_real.as_deref() == Some(*r))
                    })
                });
            }
            if let Some(add) = add_parents {
                for a in add.split(',').filter(|s| !s.is_empty()) {
                    if !f.parents.iter().any(|p| p == a) {
                        f.parents.push(a.to_string());
                    }
                }
            }
            if let Some(n) = name {
                f.name = n.to_string();
            }
            f.head_revision_id = Some(crate::types::bump_rev(f.head_revision_id.as_deref()));
            f.clone()
        };
        self.record_change(id.to_string(), false, Some(updated.clone()));
        Some(updated)
    }
}
