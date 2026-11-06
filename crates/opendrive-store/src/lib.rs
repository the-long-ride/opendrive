//! SQLite metadata + content cache index + upload queue.

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("db: {0}")]
    Db(String),
    #[error("not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store;

impl Store {
    pub fn open(_path: &str) -> Result<Self> {
        Ok(Self)
    }

    pub fn migrate(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: i64,
    pub account_id: Uuid,
    pub drive_id: String,
    pub parent_id: Option<i64>,
    pub name: String,
    pub kind: String,
    pub size: i64,
    pub state: String,
    pub pinned: bool,
}
