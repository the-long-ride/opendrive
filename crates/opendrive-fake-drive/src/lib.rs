//! In-process fake Drive v3 server for tests.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct FakeFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub parents: Vec<String>,
    pub trashed: bool,
    pub content: Vec<u8>,
}

pub struct FakeDrive {
    files: Mutex<HashMap<String, FakeFile>>,
    change_token: Mutex<u64>,
}

impl FakeDrive {
    pub fn new() -> Self {
        let mut files = HashMap::new();
        files.insert(
            "root".into(),
            FakeFile {
                id: "root".into(),
                name: "My Drive".into(),
                mime_type: "application/vnd.google-apps.folder".into(),
                size: 0,
                parents: vec![],
                trashed: false,
                content: vec![],
            },
        );
        Self {
            files: Mutex::new(files),
            change_token: Mutex::new(1),
        }
    }

    pub fn list_children(&self, parent: &str) -> Vec<FakeFile> {
        self.files
            .lock()
            .unwrap()
            .values()
            .filter(|f| f.parents.contains(&parent.to_string()) && !f.trashed)
            .cloned()
            .collect()
    }

    pub fn get_file(&self, id: &str) -> Option<FakeFile> {
        self.files.lock().unwrap().get(id).cloned()
    }

    pub fn add_file(&self, file: FakeFile) {
        self.files.lock().unwrap().insert(file.id.clone(), file);
        *self.change_token.lock().unwrap() += 1;
    }

    pub fn trash(&self, id: &str) -> bool {
        let mut files = self.files.lock().unwrap();
        if let Some(f) = files.get_mut(id) {
            f.trashed = true;
            *self.change_token.lock().unwrap() += 1;
            true
        } else {
            false
        }
    }

    pub fn current_change_token(&self) -> u64 {
        *self.change_token.lock().unwrap()
    }
}

impl Default for FakeDrive {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_exists() {
        let drive = FakeDrive::new();
        assert!(drive.get_file("root").is_some());
    }

    #[test]
    fn add_list_trash() {
        let drive = FakeDrive::new();
        let t0 = drive.current_change_token();
        drive.add_file(FakeFile {
            id: "f1".into(),
            name: "hello.txt".into(),
            mime_type: "text/plain".into(),
            size: 5,
            parents: vec!["root".into()],
            trashed: false,
            content: b"hello".to_vec(),
        });
        assert!(drive.current_change_token() > t0);
        assert_eq!(drive.list_children("root").len(), 1);
        assert!(drive.trash("f1"));
        assert!(drive.list_children("root").is_empty());
    }
}
