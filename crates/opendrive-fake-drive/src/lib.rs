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

    pub fn add_file(&self, file: FakeFile) {
        self.files.lock().unwrap().insert(file.id.clone(), file);
        *self.change_token.lock().unwrap() += 1;
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
        assert!(drive.files.lock().unwrap().contains_key("root"));
    }

    #[test]
    fn add_and_list() {
        let drive = FakeDrive::new();
        drive.add_file(FakeFile {
            id: "f1".into(),
            name: "hello.txt".into(),
            mime_type: "text/plain".into(),
            size: 5,
            parents: vec!["root".into()],
            trashed: false,
            content: b"hello".to_vec(),
        });
        let kids = drive.list_children("root");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].name, "hello.txt");
    }
}
