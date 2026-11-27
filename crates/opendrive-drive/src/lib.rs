//! Thin Drive v3 HTTP client: paging, changes, range/resumable, backoff.

mod client;
mod retry;
mod token;
mod transfer;
mod types;

pub use client::DriveClient;
pub use retry::with_backoff;
pub use token::{TokenFuture, TokenRefresher};
pub use types::{
    is_native_google_doc, About, Change, ChangeList, DriveError, DriveFile, FileJson, FileList,
    Result, SharedDrive, SharedDriveList,
};
