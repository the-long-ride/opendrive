//! In-process fake Drive v3 server for tests.

pub struct FakeDrive;

impl FakeDrive {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FakeDrive {
    fn default() -> Self {
        Self::new()
    }
}
