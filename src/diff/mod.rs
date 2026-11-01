//! Diffing trees and working trees.

pub mod render;
pub mod text;

use std::collections::BTreeSet;

use crate::Result;
use crate::history::revparse::read_commit;
use crate::history::tree_map::{TreeEntryInfo, TreeMap, flatten_tree, head_tree};
use crate::odb::{Commit, MODE_SYMLINK, ObjectId};
use crate::repo::Repository;
use crate::util::path;
use crate::worktree::{StatCache, scan};

pub use text::{DiffAlgorithm, LineStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Deleted,
    Modified,
    TypeChanged,
    Renamed,
}

impl FileChangeKind {
    pub const fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Modified => 'M',
            Self::TypeChanged => 'T',
            Self::Renamed => 'R',
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileChange {
    /// New path, or the only path for non-rename changes.
    pub path: Vec<u8>,
    /// Old path for renames.
    pub old_path: Option<Vec<u8>>,
    pub kind: FileChangeKind,
    /// 100 for exact renames; `None` for non-renames.
    pub similarity: Option<u8>,
    pub old: Option<TreeEntryInfo>,
    pub new: Option<TreeEntryInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideKind {
    Store,
    Worktree,
}

pub struct DiffSide<'a> {
    pub repo: &'a Repository,
    pub map: TreeMap,
    pub kind: SideKind,
}

impl DiffSide<'_> {
    pub fn read(&self, path: &[u8], info: &TreeEntryInfo) -> Result<Vec<u8>> {
        match self.kind {
            SideKind::Store => self.repo.store().read_blob(info.oid),
            SideKind::Worktree => {
                let absolute = path::join(&self.repo.root, path);
                if info.mode == MODE_SYMLINK {
                    scan::symlink_target_bytes(&absolute)
                } else {
                    Ok(std::fs::read(absolute)?)
                }
            }
        }
    }
}

pub fn head_side(repo: &Repository) -> Result<DiffSide<'_>> {
    Ok(DiffSide {
        repo,
        map: head_tree(repo)?,
        kind: SideKind::Store,
    })
}

pub fn rev_side(repo: &Repository, oid: ObjectId) -> Result<DiffSide<'_>> {
    let commit = read_commit(repo, oid)?;
    let store = repo.store();
    Ok(DiffSide {
        repo,
        map: flatten_tree(&store, commit.tree)?,
        kind: SideKind::Store,
    })
}

pub fn worktree_side(repo: &Repository) -> Result<DiffSide<'_>> {
    Ok(DiffSide {
        repo,
        map: worktree_map(repo)?,
        kind: SideKind::Worktree,
    })
}

/// Build a `path -> (mode, oid)` map for the working tree without writing any
/// objects.
pub fn worktree_map(repo: &Repository) -> Result<TreeMap> {
    let head = head_tree(repo)?;
    let tracked: BTreeSet<Vec<u8>> = head.keys().cloned().collect();
    let entries = scan::scan(&repo.root, &tracked)?;
    let cache = StatCache::load(&repo.fodelta.join("stat-cache"));
    let store = repo.store();
    let mut map = TreeMap::new();
    for entry in entries {
        let oid = match cache.lookup(&entry.path, entry.stat) {
            Some(oid) => oid,
            None => scan::entry_oid(&store, &repo.root, &entry)?,
        };
        map.insert(
            entry.path,
            TreeEntryInfo {
                mode: entry.stat.mode,
                oid,
            },
        );
    }
    Ok(map)
}

/// Sides for a commit against its parent, both backed by the object store.
pub fn commit_sides<'a>(
    repo: &'a Repository,
    commit: &Commit,
) -> Result<(DiffSide<'a>, DiffSide<'a>)> {
    let store = repo.store();
    let right_map = flatten_tree(&store, commit.tree)?;
    let left_map = match commit.parent {
        Some(parent) => {
            let parent = read_commit(repo, parent)?;
            flatten_tree(&store, parent.tree)?
        }
        None => TreeMap::new(),
    };
    Ok((
        DiffSide {
            repo,
            map: left_map,
            kind: SideKind::Store,
        },
        DiffSide {
            repo,
            map: right_map,
            kind: SideKind::Store,
        },
    ))
}

pub fn diff_trees(before: &TreeMap, after: &TreeMap) -> Vec<FileChange> {
    let mut paths: BTreeSet<&Vec<u8>> = BTreeSet::new();
    paths.extend(before.keys());
    paths.extend(after.keys());
    let mut changes = Vec::new();
    for path in paths {
        match (before.get(path), after.get(path)) {
            (None, Some(new)) => changes.push(FileChange {
                path: path.clone(),
                old_path: None,
                kind: FileChangeKind::Added,
                similarity: None,
                old: None,
                new: Some(*new),
            }),
            (Some(old), None) => changes.push(FileChange {
                path: path.clone(),
                old_path: None,
                kind: FileChangeKind::Deleted,
                similarity: None,
                old: Some(*old),
                new: None,
            }),
            (Some(old), Some(new)) if old != new => {
                let kind = if mode_kind(old.mode) != mode_kind(new.mode) {
                    FileChangeKind::TypeChanged
                } else {
                    FileChangeKind::Modified
                };
                changes.push(FileChange {
                    path: path.clone(),
                    old_path: None,
                    kind,
                    similarity: None,
                    old: Some(*old),
                    new: Some(*new),
                });
            }
            _ => {}
        }
    }
    detect_exact_renames(&mut changes);
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

fn detect_exact_renames(changes: &mut Vec<FileChange>) {
    let added: Vec<usize> = changes
        .iter()
        .enumerate()
        .filter(|(_, change)| change.kind == FileChangeKind::Added)
        .map(|(index, _)| index)
        .collect();
    let deleted: Vec<usize> = changes
        .iter()
        .enumerate()
        .filter(|(_, change)| change.kind == FileChangeKind::Deleted)
        .map(|(index, _)| index)
        .collect();
    if added.is_empty() || deleted.is_empty() {
        return;
    }

    let mut used_added = std::collections::BTreeSet::new();
    let mut used_deleted = std::collections::BTreeSet::new();
    let mut renames = Vec::new();
    for add_index in added {
        let new = changes[add_index].new.expect("added entry has a new side");
        let Some(&del_index) = deleted.iter().find(|&&index| {
            !used_deleted.contains(&index)
                && changes[index].old.expect("deleted entry has old side").oid == new.oid
        }) else {
            continue;
        };
        used_added.insert(add_index);
        used_deleted.insert(del_index);
        renames.push(FileChange {
            path: changes[add_index].path.clone(),
            old_path: Some(changes[del_index].path.clone()),
            kind: FileChangeKind::Renamed,
            similarity: Some(100),
            old: changes[del_index].old,
            new: Some(new),
        });
    }
    if renames.is_empty() {
        return;
    }

    let mut remaining = Vec::new();
    for (index, change) in changes.drain(..).enumerate() {
        if !used_added.contains(&index) && !used_deleted.contains(&index) {
            remaining.push(change);
        }
    }
    remaining.extend(renames);
    *changes = remaining;
}

const fn mode_kind(mode: u32) -> u8 {
    if mode == MODE_SYMLINK { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::odb::ObjectId;

    fn info(mode: u32, byte: u8) -> TreeEntryInfo {
        TreeEntryInfo {
            mode,
            oid: ObjectId::from_bytes([byte; 32]),
        }
    }

    #[test]
    fn classifies_add_modify_delete_and_type_change() {
        let mut before = TreeMap::new();
        before.insert(b"keep".to_vec(), info(0o100644, 1));
        before.insert(b"gone".to_vec(), info(0o100644, 2));
        before.insert(b"changed".to_vec(), info(0o100644, 3));
        before.insert(b"type".to_vec(), info(0o100644, 4));

        let mut after = TreeMap::new();
        after.insert(b"keep".to_vec(), info(0o100644, 1));
        after.insert(b"new".to_vec(), info(0o100644, 5));
        after.insert(b"changed".to_vec(), info(0o100644, 6));
        after.insert(b"type".to_vec(), info(0o120000, 7));

        let changes = diff_trees(&before, &after);
        let kinds: Vec<_> = changes
            .iter()
            .map(|change| (path::display(&change.path), change.kind))
            .collect();
        assert_eq!(kinds.len(), 4);
        assert!(kinds.contains(&("new".to_string(), FileChangeKind::Added)));
        assert!(kinds.contains(&("gone".to_string(), FileChangeKind::Deleted)));
        assert!(kinds.contains(&("changed".to_string(), FileChangeKind::Modified)));
        assert!(kinds.contains(&("type".to_string(), FileChangeKind::TypeChanged)));
    }
}
