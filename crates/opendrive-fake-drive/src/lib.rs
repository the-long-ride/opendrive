//! In-process fake Drive v3 state and HTTP server for tests.

mod extras;
mod http;
pub mod server;
mod types;

pub use server::FakeDriveServer;
pub use types::{FakeAbout, FakeChange, FakeExtras, FakeFile, FakeSharedDrive, UploadSession};

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use types::bump_rev;

pub struct FakeDrive {
    pub(crate) files: Mutex<HashMap<String, FakeFile>>,
    shared_drives: Mutex<Vec<FakeSharedDrive>>,
    about_info: Mutex<FakeAbout>,
    change_token: Mutex<u64>,
    changes: Mutex<Vec<FakeChange>>,
    sessions: Mutex<HashMap<String, UploadSession>>,
    next_seq: Mutex<u64>,
    error_queue: Mutex<VecDeque<u16>>,
    forced_error: Mutex<Option<u16>>,
    pub(crate) extras: Mutex<FakeExtras>,
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
                head_revision_id: Some("rev-1".into()),
            },
        );
        Self {
            files: Mutex::new(files),
            shared_drives: Mutex::new(Vec::new()),
            about_info: Mutex::new(FakeAbout::default()),
            change_token: Mutex::new(1),
            changes: Mutex::new(Vec::new()),
            sessions: Mutex::new(HashMap::new()),
            next_seq: Mutex::new(1),
            error_queue: Mutex::new(VecDeque::new()),
            forced_error: Mutex::new(None),
            extras: Mutex::new(FakeExtras::default()),
        }
    }

    pub fn about(&self) -> FakeAbout {
        self.about_info.lock().unwrap().clone()
    }

    pub fn set_about(&self, about: FakeAbout) {
        *self.about_info.lock().unwrap() = about;
    }

    pub fn add_shared_drive(&self, id: &str, name: &str) {
        self.shared_drives.lock().unwrap().push(FakeSharedDrive {
            id: id.into(),
            name: name.into(),
        });
    }

    pub fn list_shared_drives(&self) -> Vec<FakeSharedDrive> {
        self.shared_drives.lock().unwrap().clone()
    }

    pub fn list_children(&self, parent: &str) -> Vec<FakeFile> {
        let mut out: Vec<_> = self
            .files
            .lock()
            .unwrap()
            .values()
            .filter(|f| f.parents.iter().any(|p| p == parent) && !f.trashed)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn get_file(&self, id: &str) -> Option<FakeFile> {
        self.files.lock().unwrap().get(id).cloned()
    }

    fn record_change(&self, file_id: String, removed: bool, file: Option<FakeFile>) {
        let mut tok = self.change_token.lock().unwrap();
        let at = *tok;
        *tok += 1;
        self.changes.lock().unwrap().push(FakeChange {
            token: at,
            file_id,
            removed,
            file,
        });
    }

    pub fn add_file(&self, mut file: FakeFile) {
        if file.head_revision_id.is_none() {
            file.head_revision_id = Some("rev-1".into());
        }
        let id = file.id.clone();
        self.files.lock().unwrap().insert(id.clone(), file.clone());
        self.record_change(id, false, Some(file));
    }

    pub fn update_metadata(
        &self,
        id: &str,
        name: Option<&str>,
        trashed: Option<bool>,
        parents: Option<Vec<String>>,
    ) -> Option<FakeFile> {
        let updated = {
            let mut files = self.files.lock().unwrap();
            let f = files.get_mut(id)?;
            if let Some(n) = name {
                f.name = n.to_string();
            }
            if let Some(t) = trashed {
                f.trashed = t;
            }
            if let Some(p) = parents {
                f.parents = p;
            }
            f.head_revision_id = Some(bump_rev(f.head_revision_id.as_deref()));
            f.clone()
        };
        self.record_change(id.to_string(), false, Some(updated.clone()));
        Some(updated)
    }

    pub fn trash(&self, id: &str) -> bool {
        let exists = {
            let mut files = self.files.lock().unwrap();
            if let Some(f) = files.get_mut(id) {
                f.trashed = true;
                true
            } else {
                false
            }
        };
        if exists {
            self.record_change(id.to_string(), true, None);
        }
        exists
    }

    pub fn changes_since(&self, token: u64) -> (Vec<FakeChange>, u64) {
        let current = *self.change_token.lock().unwrap();
        let list = self
            .changes
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.token >= token)
            .cloned()
            .collect();
        (list, current)
    }

    pub fn current_change_token(&self) -> u64 {
        *self.change_token.lock().unwrap()
    }

    pub fn create_upload_session(
        &self,
        name: String,
        mime_type: String,
        parents: Vec<String>,
        file_id: Option<String>,
    ) -> String {
        let mut seq = self.next_seq.lock().unwrap();
        let sid = format!("sess-{}", *seq);
        *seq += 1;
        self.sessions.lock().unwrap().insert(
            sid.clone(),
            UploadSession {
                id: sid.clone(),
                file_id,
                name,
                mime_type,
                parents,
                buffer: Vec::new(),
            },
        );
        sid
    }

    pub fn put_upload_chunk(
        &self,
        session_id: &str,
        offset: u64,
        total: Option<u64>,
        data: &[u8],
    ) -> Result<Option<FakeFile>, String> {
        let mut sessions = self.sessions.lock().unwrap();
        let sess = sessions
            .get_mut(session_id)
            .ok_or_else(|| "unknown upload session".to_string())?;
        let start = offset as usize;
        let end = start + data.len();
        if sess.buffer.len() < end {
            sess.buffer.resize(end, 0);
        }
        sess.buffer[start..end].copy_from_slice(data);
        let done = match total {
            Some(0) => true,
            Some(t) => (sess.buffer.len() as u64) >= t,
            None => true,
        };
        if !done {
            return Ok(None);
        }
        let sess = sessions.remove(session_id).unwrap();
        drop(sessions);

        let file_id = match sess.file_id {
            Some(id) => id,
            None => {
                let mut seq = self.next_seq.lock().unwrap();
                let id = format!("file-{}", *seq);
                *seq += 1;
                id
            }
        };
        let rev = {
            let files = self.files.lock().unwrap();
            let prev = files
                .get(&file_id)
                .and_then(|f| f.head_revision_id.as_deref());
            bump_rev(prev)
        };
        let file = FakeFile {
            id: file_id.clone(),
            name: sess.name,
            mime_type: sess.mime_type,
            size: sess.buffer.len() as u64,
            parents: if sess.parents.is_empty() {
                vec!["root".into()]
            } else {
                sess.parents
            },
            trashed: false,
            content: sess.buffer,
            head_revision_id: Some(rev),
        };
        self.files
            .lock()
            .unwrap()
            .insert(file_id.clone(), file.clone());
        self.record_change(file_id, false, Some(file.clone()));
        Ok(Some(file))
    }

    pub fn push_error(&self, status: u16) {
        self.error_queue.lock().unwrap().push_back(status);
    }

    pub fn set_forced_error(&self, status: Option<u16>) {
        *self.forced_error.lock().unwrap() = status;
    }

    pub fn take_error(&self) -> Option<u16> {
        self.error_queue
            .lock()
            .unwrap()
            .pop_front()
            .or_else(|| *self.forced_error.lock().unwrap())
    }
}

impl Default for FakeDrive {
    fn default() -> Self {
        Self::new()
    }
}
