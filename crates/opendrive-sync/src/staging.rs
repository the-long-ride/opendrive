//! Local staging files for writes (never partial remote).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
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
