//! Commit history walking and commit-to-parent path diffs.

use std::collections::BTreeSet;

use crate::history::revparse::read_commit;
use crate::history::tree_map::{TreeMap, flatten_tree};
use crate::odb::{Commit, ObjectId};
use crate::repo::Repository;
use crate::util::path;
use crate::worktree::stat_cache::StatCache;
use crate::{FodeltaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

impl ChangeKind {
    pub const fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub oid: ObjectId,
    pub commit: Commit,
    pub changes: Vec<(Vec<u8>, ChangeKind)>,
}

#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    pub limit: Option<usize>,
    pub reverse: bool,
    pub stat: bool,
    pub paths: Vec<Vec<u8>>,
}

pub fn log_entries(repo: &Repository, options: &LogOptions) -> Result<Vec<LogEntry>> {
    let Some(head) = repo.head_oid()? else {
        return Err(FodeltaError::NoCommits);
    };
    let mut current = Some(head);
    let mut entries = Vec::new();

    while let Some(oid) = current {
        let commit = read_commit(repo, oid)?;
        let changes = if options.stat || !options.paths.is_empty() {
            changes_for_commit(repo, &commit)?
        } else {
            Vec::new()
        };
        let touches = options.paths.is_empty()
            || changes.iter().any(|(candidate, _)| {
                options
                    .paths
                    .iter()
                    .any(|spec| path::is_within(spec, candidate))
            });
        if touches {
            entries.push(LogEntry {
                oid,
                commit: commit.clone(),
                changes,
            });
        }
        current = commit.parent;
        if !options.reverse && options.limit.is_some_and(|limit| entries.len() >= limit) {
            break;
        }
    }

    if options.reverse {
        entries.reverse();
    }
    if let Some(limit) = options.limit {
        entries.truncate(limit);
    }
    Ok(entries)
}

pub fn changes_for_commit(
    repo: &Repository,
    commit: &Commit,
) -> Result<Vec<(Vec<u8>, ChangeKind)>> {
    let store = repo.store();
    let before = match commit.parent {
        Some(parent) => {
            let parent_commit = read_commit(repo, parent)?;
            flatten_tree(&store, parent_commit.tree)?
        }
        None => TreeMap::new(),
    };
    let after = flatten_tree(&store, commit.tree)?;
    Ok(diff_maps(&before, &after))
}

pub fn diff_maps(before: &TreeMap, after: &TreeMap) -> Vec<(Vec<u8>, ChangeKind)> {
    let mut paths: BTreeSet<&Vec<u8>> = BTreeSet::new();
    paths.extend(before.keys());
    paths.extend(after.keys());
    let mut changes = Vec::new();
    for path in paths {
        match (before.get(path), after.get(path)) {
            (None, Some(_)) => changes.push((path.clone(), ChangeKind::Added)),
            (Some(_), None) => changes.push((path.clone(), ChangeKind::Deleted)),
            (Some(old), Some(new)) if old != new => {
                changes.push((path.clone(), ChangeKind::Modified));
            }
            _ => {}
        }
    }
    changes
}

/// Load a commit object and its tree map.
pub fn commit_tree(repo: &Repository, oid: ObjectId) -> Result<(Commit, TreeMap)> {
    let commit = read_commit(repo, oid)?;
    let store = repo.store();
    let tree = flatten_tree(&store, commit.tree)?;
    Ok((commit, tree))
}

/// Read a blob from a revision's tree.
pub fn blob_at(repo: &Repository, commit: &Commit, relative: &[u8]) -> Result<Option<Vec<u8>>> {
    let store = repo.store();
    let tree = flatten_tree(&store, commit.tree)?;
    let Some(info) = tree.get(relative) else {
        return Ok(None);
    };
    Ok(Some(store.read_blob(info.oid)?))
}

/// Refresh the stat cache after an operation that rewrote the working tree.
pub fn refresh_stat_cache(
    repo: &Repository,
    entries: &[crate::worktree::WorktreeEntry],
) -> Result<()> {
    let cache_path = repo.fodelta.join("stat-cache");
    let mut cache = StatCache::load(&cache_path);
    cache.clear();
    let store = repo.store();
    for entry in entries {
        let oid = match entry.kind {
            crate::worktree::EntryKind::Symlink => {
                let target =
                    crate::worktree::scan::symlink_target_bytes(&entry.absolute(&repo.root))?;
                crate::odb::blob_hash(&target)
            }
            crate::worktree::EntryKind::File => store.hash_file(&entry.absolute(&repo.root))?,
        };
        cache.insert(entry.path.clone(), entry.stat, oid);
    }
    let _ = cache.save(&cache_path);
    Ok(())
}

pub fn unborn_error() -> FodeltaError {
    FodeltaError::UnbornHead
}
