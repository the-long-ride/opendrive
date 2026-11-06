//! Local staging files for writes (never partial remote).

use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StagingError {
    #[error("io: {0}")]
    Io(String),
}

pub struct StagingDir {
    root: PathBuf,
}

impl StagingDir {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StagingError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|e| StagingError::Io(e.to_string()))?;
        Ok(Self { root })
    }

    pub fn path_for(&self, node_id: i64) -> PathBuf {
        self.root.join(format!("{node_id}.part"))
    }

    pub fn open_write(&self, node_id: i64) -> Result<File, StagingError> {
        let p = self.path_for(node_id);
        File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(p)
            .map_err(|e| StagingError::Io(e.to_string()))
    }

    pub fn read_all(&self, node_id: i64) -> Result<Vec<u8>, StagingError> {
        let p = self.path_for(node_id);
        if !p.exists() {
            return Ok(vec![]);
        }
        fs::read(p).map_err(|e| StagingError::Io(e.to_string()))
    }

    pub fn write_at(&self, node_id: i64, offset: u64, data: &[u8]) -> Result<(), StagingError> {
        let mut f = self.open_write(node_id)?;
        f.seek(SeekFrom::Start(offset))
            .map_err(|e| StagingError::Io(e.to_string()))?;
        f.write_all(data)
            .map_err(|e| StagingError::Io(e.to_string()))?;
        Ok(())
    }

    pub fn remove(&self, node_id: i64) -> Result<(), StagingError> {
        let p = self.path_for(node_id);
        if p.exists() {
            fs::remove_file(p).map_err(|e| StagingError::Io(e.to_string()))?;
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn staging_write_read_remove_lifecycle() {
        let root = std::env::temp_dir().join(format!("opendrive-staging-{}", Uuid::new_v4()));
        let dir = StagingDir::new(&root).unwrap();
        assert_eq!(dir.root(), root.as_path());
        assert!(dir.read_all(7).unwrap().is_empty());

        let _f = dir.open_write(7).unwrap();
        dir.write_at(7, 0, b"hello world").unwrap();
        dir.write_at(7, 6, b"rust!").unwrap();
        assert_eq!(dir.read_all(7).unwrap(), b"hello rust!");

        dir.remove(7).unwrap();
        assert!(dir.read_all(7).unwrap().is_empty());
        dir.remove(7).unwrap();

        let dir_entry = dir.path_for(99);
        fs::create_dir_all(&dir_entry).unwrap();
        assert!(dir.open_write(99).is_err());
        assert!(dir.read_all(99).is_err());
        assert!(dir.write_at(99, 0, b"x").is_err());
        assert!(dir.remove(99).is_err());

        let _ = fs::remove_dir_all(&root);
    }
}
