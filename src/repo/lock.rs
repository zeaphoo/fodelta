//! Repository-wide exclusive lock for mutating commands.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

use crate::{FodeltaError, Result};

pub const LOCK_FILE: &str = "lock";

pub struct RepoLock {
    file: File,
}

impl RepoLock {
    pub fn acquire(fodelta: &Path) -> Result<Self> {
        let path = fodelta.join(LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { file }),
            Err(TryLockError::WouldBlock) => Err(FodeltaError::Locked { path }),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }
}

impl Drop for RepoLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        fs_create_repo(dir.path());
        {
            let _lock = RepoLock::acquire(&dir.path().join(".fodelta")).unwrap();
            assert!(dir.path().join(".fodelta/lock").exists());
        }
        // The lock must be available again after the guard is dropped.
        let _lock = RepoLock::acquire(&dir.path().join(".fodelta")).unwrap();
    }

    fn fs_create_repo(path: &Path) {
        std::fs::create_dir_all(path.join(".fodelta")).unwrap();
    }
}
