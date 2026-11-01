//! Walk the working tree with `.gitignore` semantics.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use ignore::WalkBuilder;

use crate::odb::{ObjectId, ObjectStore};
use crate::repo::discover::DOT_DIR;
use crate::util::path;
use crate::worktree::stat_cache::FileStat;
use crate::{FodeltaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: Vec<u8>,
    pub kind: EntryKind,
    pub stat: FileStat,
}

impl WorktreeEntry {
    pub fn absolute(&self, root: &Path) -> PathBuf {
        path::join(root, &self.path)
    }

    pub fn is_symlink(&self) -> bool {
        self.kind == EntryKind::Symlink
    }
}

/// Scan the working tree, applying `.gitignore` rules, and add tracked paths
/// that would otherwise be hidden by an ignore rule.
pub fn scan(root: &Path, tracked: &BTreeSet<Vec<u8>>) -> Result<Vec<WorktreeEntry>> {
    let mut entries: BTreeMap<Vec<u8>, WorktreeEntry> = BTreeMap::new();
    for entry in walk(root, true)? {
        entries.insert(entry.path.clone(), entry);
    }
    for relative in tracked {
        if entries.contains_key(relative) {
            continue;
        }
        let absolute = path::join(root, relative);
        match fs::symlink_metadata(&absolute) {
            Ok(_) => {
                let entry = entry_from_path(&absolute, relative.clone())?;
                entries.insert(entry.path.clone(), entry);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(entries.into_values().collect())
}

/// Scan every supported file, including ignored ones. Used by `status
/// --ignored`.
pub fn scan_all(root: &Path) -> Result<Vec<WorktreeEntry>> {
    walk(root, false)
}

fn walk(root: &Path, respect_gitignore: bool) -> Result<Vec<WorktreeEntry>> {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(respect_gitignore)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name();
            if name == OsStr::new(DOT_DIR) || name == OsStr::new(".git") {
                return false;
            }
            if entry.file_type().is_some_and(|kind| kind.is_dir())
                && entry.path().join(DOT_DIR).is_dir()
            {
                return false;
            }
            true
        });

    let mut entries = Vec::new();
    for result in builder.build() {
        let entry =
            result.map_err(|error| FodeltaError::Io(std::io::Error::other(error.to_string())))?;
        if entry.depth() == 0 {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| FodeltaError::PathEscapes {
                path: entry.path().to_path_buf(),
            })?;
        let relative = path::path_to_bytes(relative);
        entries.push(entry_from_path(entry.path(), relative)?);
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

pub fn entry_from_path(absolute: &Path, relative: Vec<u8>) -> Result<WorktreeEntry> {
    let metadata = fs::symlink_metadata(absolute)?;
    let kind = if metadata.file_type().is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::File
    };
    let mode = match kind {
        EntryKind::Symlink => crate::odb::MODE_SYMLINK,
        EntryKind::File if is_executable(&metadata) => crate::odb::MODE_EXEC,
        EntryKind::File => crate::odb::MODE_FILE,
    };
    let size = match kind {
        EntryKind::Symlink => symlink_target_bytes(absolute)?.len() as u64,
        EntryKind::File => metadata.len(),
    };
    let (mtime_sec, mtime_nsec) = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| (duration.as_secs() as i64, duration.subsec_nanos()))
        .unwrap_or((0, 0));
    let inode = inode(&metadata);
    Ok(WorktreeEntry {
        path: relative,
        kind,
        stat: FileStat {
            size,
            mtime_sec,
            mtime_nsec,
            inode,
            mode,
        },
    })
}

/// Hash the content of a scanned entry without writing an object.
pub fn entry_oid(store: &ObjectStore, root: &Path, entry: &WorktreeEntry) -> Result<ObjectId> {
    match entry.kind {
        EntryKind::Symlink => {
            let target = symlink_target_bytes(&entry.absolute(root))?;
            Ok(crate::odb::blob_hash(&target))
        }
        EntryKind::File => store.hash_file(&entry.absolute(root)),
    }
}

pub fn symlink_target_bytes(absolute: &Path) -> Result<Vec<u8>> {
    let target = fs::read_link(absolute)?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(target.as_os_str().as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        Ok(target.to_string_lossy().as_bytes().to_vec())
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn inode(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.ino()
}

#[cfg(not(unix))]
fn inode(_metadata: &fs::Metadata) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, contents: &[u8]) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn respects_nested_gitignore_and_negation() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".gitignore", b"*.log\n");
        write(dir.path(), "src/.gitignore", b"!keep.log\n");
        write(dir.path(), "other.log", b"x");
        write(dir.path(), "src/keep.log", b"x");
        write(dir.path(), "src/drop.log", b"x");

        let entries = scan(dir.path(), &BTreeSet::new()).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| path::display(&e.path)).collect();
        assert!(paths.contains(&"src/keep.log".to_string()));
        assert!(!paths.contains(&"other.log".to_string()));
        assert!(!paths.contains(&"src/drop.log".to_string()));
    }

    #[test]
    fn includes_tracked_files_even_when_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".gitignore", b"*.log\n");
        write(dir.path(), "tracked.log", b"x");
        let tracked = BTreeSet::from([b"tracked.log".to_vec()]);
        let entries = scan(dir.path(), &tracked).unwrap();
        assert!(entries.iter().any(|e| e.path == b"tracked.log"));
    }

    #[test]
    fn skips_metadata_and_nested_repositories() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".fodelta/HEAD", b"ref: refs/heads/main\n");
        write(dir.path(), ".git/config", b"x");
        write(dir.path(), "vendor/other/.fodelta/HEAD", b"x");
        write(dir.path(), "vendor/other/file.txt", b"x");
        write(dir.path(), "keep.txt", b"x");
        let entries = scan(dir.path(), &BTreeSet::new()).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| path::display(&e.path)).collect();
        assert_eq!(paths, ["keep.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn records_symlink_mode_and_target_size() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("target", dir.path().join("link")).unwrap();
        let entries = scan(dir.path(), &BTreeSet::new()).unwrap();
        let link = entries.iter().find(|e| e.path == b"link").unwrap();
        assert!(link.is_symlink());
        assert_eq!(link.stat.mode, crate::odb::MODE_SYMLINK);
        assert_eq!(link.stat.size, 6);
    }
}
