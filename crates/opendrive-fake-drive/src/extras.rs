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

    pub fn file_to_json(&self, f: &FakeFile) -> serde_json::Value {
        file_to_json(f, &self.extras.lock().unwrap())
    }
}
