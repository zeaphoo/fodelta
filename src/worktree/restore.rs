//! Restore files from a commit into the working tree.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::diff::worktree_map;
use crate::history::revparse::read_commit;
use crate::history::tree_map::{TreeMap, flatten_tree, head_tree};
use crate::odb::{MODE_SYMLINK, ObjectId};
use crate::repo::Repository;
use crate::util::atomic::{TempPath, write_atomic_with};
use crate::util::path;
use crate::worktree::stat_cache::StatCache;
use crate::{FodeltaError, Result};

#[derive(Debug, Clone, Default)]
pub struct RestoreOptions {
    /// `None` means HEAD.
    pub source: Option<ObjectId>,
    /// Repository-relative prefixes; empty means the whole tree.
    pub paths: Vec<Vec<u8>>,
    pub delete: bool,
    pub no_safety: bool,
}

#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    pub safety: Option<ObjectId>,
    pub restored: usize,
    pub deleted: usize,
    pub dirty: bool,
    pub safety_skipped: bool,
}

pub fn restore(repo: &Repository, options: &RestoreOptions) -> Result<RestoreOutcome> {
    let _lock = repo.lock()?;
    restore_with_lock(repo, options)
}

/// Restore implementation for callers that already hold the repository lock.
pub fn restore_with_lock(repo: &Repository, options: &RestoreOptions) -> Result<RestoreOutcome> {
    let (source_map, label) = match options.source {
        Some(oid) => {
            let commit = read_commit(repo, oid)?;
            (
                flatten_tree(&repo.store(), commit.tree)?,
                format!("revision {}", oid.short()),
            )
        }
        None => (head_tree(repo)?, "HEAD".to_string()),
    };
    let head = head_tree(repo)?;
    let current = worktree_map(repo)?;
    let dirty = current != head;

    for spec in &options.paths {
        let in_source = source_map
            .keys()
            .any(|candidate| path::is_within(spec, candidate));
        let in_current = current
            .keys()
            .any(|candidate| path::is_within(spec, candidate));
        if !in_source && !(options.delete && in_current) {
            return Err(FodeltaError::PathMissingInRev {
                rev: label,
                path: path::bytes_to_path(spec),
            });
        }
    }

    let safety = if dirty && !options.no_safety {
        crate::safety::create(repo, "before restore")?
    } else {
        None
    };
    let safety_skipped = dirty && options.no_safety;

    let (restored, deleted) = sync_worktree(repo, &source_map, &options.paths, options.delete)?;
    refresh_cache_scope(repo, &options.paths)?;

    Ok(RestoreOutcome {
        safety,
        restored,
        deleted,
        dirty,
        safety_skipped,
    })
}

/// Write `source` into the working tree, optionally deleting files that are not
/// in the source. The caller must hold the repository lock.
pub fn sync_worktree(
    repo: &Repository,
    source: &TreeMap,
    specs: &[Vec<u8>],
    delete: bool,
) -> Result<(usize, usize)> {
    let store = repo.store();
    let current = worktree_map(repo)?;
    let mut restored = 0;
    for (relative, info) in source {
        if !selected(specs, relative) {
            continue;
        }
        if current.get(relative) == Some(info) {
            continue;
        }
        let bytes = store.read_blob(info.oid)?;
        write_entry(repo, relative, info.mode, &bytes)?;
        restored += 1;
    }

    let mut deleted = 0;
    if delete {
        for relative in current.keys() {
            if !selected(specs, relative) || source.contains_key(relative) {
                continue;
            }
            remove_entry(repo, relative)?;
            deleted += 1;
        }
    }
    Ok((restored, deleted))
}

fn selected(specs: &[Vec<u8>], relative: &[u8]) -> bool {
    specs.is_empty() || specs.iter().any(|spec| path::is_within(spec, relative))
}

pub fn refresh_cache_scope(repo: &Repository, specs: &[Vec<u8>]) -> Result<()> {
    let cache_path = repo.fodelta.join("stat-cache");
    let mut cache = StatCache::load(&cache_path);
    if specs.is_empty() {
        cache.clear();
    } else {
        cache.retain(|candidate| !specs.iter().any(|spec| path::is_within(spec, candidate)));
    }
    let _ = cache.save(&cache_path);
    Ok(())
}

pub fn clear_cache(repo: &Repository) -> Result<()> {
    let cache_path = repo.fodelta.join("stat-cache");
    let mut cache = StatCache::load(&cache_path);
    cache.clear();
    let _ = cache.save(&cache_path);
    Ok(())
}

fn write_entry(repo: &Repository, relative: &[u8], mode: u32, bytes: &[u8]) -> Result<()> {
    let target = path::join(&repo.root, relative);
    ensure_safe_parents(&repo.root, relative)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_existing(&target)?;
    if mode == MODE_SYMLINK {
        write_symlink(&target, bytes)?;
    } else {
        let fsync = repo.config.core.fsync.fsync_refs();
        write_atomic_with(&target, fsync, |file: &mut File| file.write_all(bytes))?;
        set_mode(&target, mode)?;
    }
    Ok(())
}

fn remove_entry(repo: &Repository, relative: &[u8]) -> Result<()> {
    let target = path::join(&repo.root, relative);
    ensure_safe_parents(&repo.root, relative)?;
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&target)?,
        Ok(_) => fs::remove_file(&target)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if let Some(parent) = target.parent() {
        cleanup_empty_dirs(parent, &repo.root);
    }
    Ok(())
}

fn cleanup_empty_dirs(start: &Path, root: &Path) {
    let mut current = Some(start);
    while let Some(directory) = current {
        if directory == root || !directory.starts_with(root) {
            break;
        }
        if fs::remove_dir(directory).is_err() {
            break;
        }
        current = directory.parent();
    }
}

fn ensure_safe_parents(root: &Path, relative: &[u8]) -> Result<()> {
    let relative = path::bytes_to_path(relative);
    let components: Vec<_> = relative.components().collect();
    let mut current = root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(FodeltaError::PathEscapes { path: current });
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(FodeltaError::Io(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!("{} is not a directory", current.display()),
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn remove_existing(target: &Path) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(target)?,
        Ok(_) => fs::remove_file(target)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(unix)]
fn write_symlink(target: &Path, contents: &[u8]) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let parent = target.parent().ok_or_else(|| FodeltaError::PathEscapes {
        path: target.to_path_buf(),
    })?;
    let temp = TempPath::new_in(parent)?;
    std::os::unix::fs::symlink(OsStr::from_bytes(contents), temp.path())?;
    temp.persist(target)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_symlink(_target: &Path, _contents: &[u8]) -> Result<()> {
    Err(FodeltaError::Io(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlink restore is not supported on this platform",
    )))
}

#[cfg(unix)]
fn set_mode(target: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(target, fs::Permissions::from_mode(mode & 0o777))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_target: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

pub fn source_label(repo: &Repository, source: Option<ObjectId>) -> Result<String> {
    match source {
        Some(oid) => Ok(format!("revision {}", oid.short())),
        None => {
            let _ = repo;
            Ok("HEAD".to_string())
        }
    }
}

pub fn current_paths(repo: &Repository) -> Result<Vec<PathBuf>> {
    Ok(worktree_map(repo)?
        .keys()
        .map(|path| path::bytes_to_path(path))
        .collect())
}
