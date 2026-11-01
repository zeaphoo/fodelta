//! Atomic file replacement: write to a temporary file in the same directory,
//! optionally fsync, then rename over the target.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary path that removes itself on drop unless it was persisted.
pub struct TempPath {
    path: PathBuf,
    persisted: bool,
}

impl TempPath {
    pub fn new_in(dir: &Path) -> io::Result<Self> {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0);
        let path = dir.join(format!(
            ".fodelta-tmp-{}-{nanos}-{counter}",
            std::process::id()
        ));
        Ok(Self {
            path,
            persisted: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Rename the temporary file over `target`.
    pub fn persist(mut self, target: &Path) -> io::Result<()> {
        fs::rename(&self.path, target)?;
        self.persisted = true;
        Ok(())
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Write `target` atomically using `f`. The closure owns the temporary file
/// handle and must flush any buffered writers before returning.
pub fn write_atomic_with<F>(target: &Path, fsync: bool, f: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let parent = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "target has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;

    let temp = TempPath::new_in(parent)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp.path())?;
    f(&mut file)?;
    if fsync {
        file.sync_all()?;
    }
    drop(file);
    temp.persist(target)?;
    if fsync {
        sync_dir(parent)?;
    }
    Ok(())
}

/// Convenience wrapper for writing a byte slice atomically.
pub fn write_atomic(target: &Path, contents: &[u8], fsync: bool) -> io::Result<()> {
    write_atomic_with(target, fsync, |file| file.write_all(contents))
}

/// Best-effort directory fsync. Windows has no portable equivalent.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("value");
        fs::write(&target, b"old").unwrap();
        write_atomic(&target, b"new", false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn cleans_up_temporary_file_on_error() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("value");
        let result = write_atomic_with(&target, false, |_file| Err(io::Error::other("boom")));
        assert!(result.is_err());
        assert!(!target.exists());
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(leftovers.is_empty(), "leftovers: {leftovers:?}");
    }
}
