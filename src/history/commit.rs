//! Create a commit from the working tree, optionally restricted to paths.

use std::collections::BTreeSet;
use std::path::Path;

use crate::history::head_tree;
use crate::history::tree_map::{TreeEntryInfo, TreeMap, build_trees};
use crate::odb::{Commit, ObjectId, ObjectKind};
use crate::repo::{MAIN_REF, Repository, refs};
use crate::util::path;
use crate::worktree::scan::{self, EntryKind, WorktreeEntry};
use crate::worktree::stat_cache::StatCache;
use crate::worktree::status::compute_status;
use crate::{FodeltaError, Result};

#[derive(Debug, Clone)]
pub struct CommitOptions {
    pub message: String,
    pub allow_empty: bool,
    pub allow_large_files: bool,
    /// Repository-relative path prefixes. Empty means the whole tree.
    pub paths: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct CommitOutcome {
    pub oid: ObjectId,
    pub changed: usize,
    pub remaining: usize,
}

pub fn create_commit(repo: &Repository, options: &CommitOptions) -> Result<CommitOutcome> {
    if options.message.trim().is_empty() {
        return Err(FodeltaError::EmptyCommitMessage);
    }
    let _lock = repo.lock()?;

    let head_oid = repo.head_oid()?;
    let head_map = head_tree(repo)?;
    let tracked: BTreeSet<Vec<u8>> = head_map.keys().cloned().collect();
    let worktree = scan::scan(&repo.root, &tracked)?;
    let store = repo.store();
    let max_file_size = repo
        .config
        .core
        .max_file_size_mb
        .saturating_mul(1024 * 1024);

    let mut new_map = if options.paths.is_empty() {
        TreeMap::new()
    } else {
        head_map.clone()
    };
    let mut selected_worktree: Vec<&WorktreeEntry> = Vec::new();

    if options.paths.is_empty() {
        for entry in &worktree {
            ensure_size(entry, max_file_size, options.allow_large_files)?;
            let info = write_entry(&store, &repo.root, entry)?;
            new_map.insert(entry.path.clone(), info);
            selected_worktree.push(entry);
        }
    } else {
        for spec in &options.paths {
            let head_matches: Vec<Vec<u8>> = head_map
                .keys()
                .filter(|candidate| path::is_within(spec, candidate))
                .cloned()
                .collect();
            let worktree_matches: Vec<&WorktreeEntry> = worktree
                .iter()
                .filter(|entry| path::is_within(spec, &entry.path))
                .collect();

            if head_matches.is_empty() && worktree_matches.is_empty() {
                let absolute = if spec.is_empty() {
                    repo.root.clone()
                } else {
                    path::join(&repo.root, spec)
                };
                if let Ok(metadata) = std::fs::symlink_metadata(&absolute) {
                    let is_file = metadata.is_file() || metadata.file_type().is_symlink();
                    if is_file && !head_map.contains_key(spec) {
                        let verdict = crate::ignore::check(&repo.root, spec)?;
                        if verdict.ignored {
                            return Err(FodeltaError::PathIgnored {
                                path: path::bytes_to_path(spec),
                            });
                        }
                    }
                }
                return Err(FodeltaError::PathNotMatched {
                    path: path::bytes_to_path(spec),
                });
            }

            new_map.retain(|candidate, _| !path::is_within(spec, candidate));
            for entry in worktree_matches {
                ensure_size(entry, max_file_size, options.allow_large_files)?;
                let info = write_entry(&store, &repo.root, entry)?;
                new_map.insert(entry.path.clone(), info);
                selected_worktree.push(entry);
            }
        }
    }

    if new_map == head_map && !options.allow_empty {
        return Err(FodeltaError::NothingToCommit);
    }

    let changed = count_changes(&head_map, &new_map);
    let tree = build_trees(&store, &new_map)?;
    let author = repo.author()?;
    let commit = Commit::new(tree, head_oid, author, options.message.clone());
    let payload = commit.encode()?;
    let oid = store.write(ObjectKind::Commit, &payload)?;

    refs::update_ref(
        &repo.fodelta,
        MAIN_REF,
        head_oid,
        oid,
        repo.config.core.fsync.fsync_refs(),
    )?;

    update_stat_cache(repo, &selected_worktree, &options.paths, &store)?;

    let remaining = compute_status(repo, false)
        .map(|report| report.total_changes())
        .unwrap_or(0);

    Ok(CommitOutcome {
        oid,
        changed,
        remaining,
    })
}

fn write_entry(
    store: &crate::odb::ObjectStore,
    root: &Path,
    entry: &WorktreeEntry,
) -> Result<TreeEntryInfo> {
    let oid = match entry.kind {
        EntryKind::Symlink => {
            let target = scan::symlink_target_bytes(&entry.absolute(root))?;
            store.write_blob(&target)?
        }
        EntryKind::File => store.write_file(&entry.absolute(root))?,
    };
    Ok(TreeEntryInfo {
        mode: entry.stat.mode,
        oid,
    })
}

fn ensure_size(entry: &WorktreeEntry, limit: u64, allow_large: bool) -> Result<()> {
    if entry.kind == EntryKind::File && entry.stat.size > limit && !allow_large {
        return Err(FodeltaError::FileTooLarge {
            path: path::bytes_to_path(&entry.path),
            size: entry.stat.size,
            limit,
        });
    }
    Ok(())
}

fn count_changes(before: &TreeMap, after: &TreeMap) -> usize {
    let mut changed = 0;
    for (path, info) in after {
        match before.get(path) {
            Some(old) if old == info => {}
            _ => changed += 1,
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            changed += 1;
        }
    }
    changed
}

fn update_stat_cache(
    repo: &Repository,
    selected: &[&WorktreeEntry],
    specs: &[Vec<u8>],
    store: &crate::odb::ObjectStore,
) -> Result<()> {
    let cache_path = repo.fodelta.join("stat-cache");
    let mut cache = StatCache::load(&cache_path);
    if specs.is_empty() {
        cache.clear();
    } else {
        cache.retain(|path| !specs.iter().any(|spec| path::is_within(spec, path)));
    }
    for entry in selected {
        let oid = match entry.kind {
            EntryKind::Symlink => {
                let target = scan::symlink_target_bytes(&entry.absolute(&repo.root))?;
                crate::odb::blob_hash(&target)
            }
            EntryKind::File => store.hash_file(&entry.absolute(&repo.root))?,
        };
        cache.insert(entry.path.clone(), entry.stat, oid);
    }
    // The cache is only an optimization; a failure here must not fail a commit
    // that already moved HEAD.
    let _ = cache.save(&cache_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repository;

    fn repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        (dir, repo)
    }

    fn write(repo: &Repository, name: &str, data: &[u8]) {
        let path = repo.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, data).unwrap();
    }

    fn commit(repo: &Repository, paths: Vec<Vec<u8>>) -> Result<CommitOutcome> {
        create_commit(
            repo,
            &CommitOptions {
                message: "commit".to_string(),
                allow_empty: false,
                allow_large_files: false,
                paths,
            },
        )
    }

    #[test]
    fn unchanged_subtrees_keep_their_oid() {
        let (_dir, repo) = repo();
        write(&repo, "docs/x.md", b"x");
        write(&repo, "src/y.rs", b"y");
        commit(&repo, Vec::new()).unwrap();
        let first =
            crate::history::revparse::read_commit(&repo, repo.require_head().unwrap()).unwrap();
        let first_tree = tree_of(&repo, first.tree);

        write(&repo, "src/y.rs", b"y2");
        commit(&repo, Vec::new()).unwrap();
        let second =
            crate::history::revparse::read_commit(&repo, repo.require_head().unwrap()).unwrap();
        let second_tree = tree_of(&repo, second.tree);

        let docs_before = first_tree
            .entries()
            .iter()
            .find(|e| e.name == b"docs")
            .unwrap();
        let docs_after = second_tree
            .entries()
            .iter()
            .find(|e| e.name == b"docs")
            .unwrap();
        assert_eq!(docs_before.oid, docs_after.oid);
        let src_before = first_tree
            .entries()
            .iter()
            .find(|e| e.name == b"src")
            .unwrap();
        let src_after = second_tree
            .entries()
            .iter()
            .find(|e| e.name == b"src")
            .unwrap();
        assert_ne!(src_before.oid, src_after.oid);
    }

    fn tree_of(repo: &Repository, oid: ObjectId) -> crate::odb::Tree {
        let (kind, payload) = repo.store().read(oid).unwrap();
        assert_eq!(kind, ObjectKind::Tree);
        crate::odb::Tree::decode(&payload).unwrap()
    }

    #[test]
    fn full_commit_records_everything() {
        let (_dir, repo) = repo();
        write(&repo, "a.txt", b"a");
        write(&repo, "src/b.txt", b"b");
        let outcome = commit(&repo, Vec::new()).unwrap();
        assert_eq!(outcome.changed, 2);
        assert!(repo.head_oid().unwrap().is_some());
        assert_eq!(outcome.remaining, 0);
    }

    #[test]
    fn selective_commit_leaves_other_changes_pending() {
        let (_dir, repo) = repo();
        write(&repo, "a.txt", b"a");
        write(&repo, "b.txt", b"b");
        commit(&repo, Vec::new()).unwrap();

        write(&repo, "a.txt", b"a2");
        write(&repo, "b.txt", b"b2");
        let outcome = commit(&repo, vec![b"a.txt".to_vec()]).unwrap();
        assert_eq!(outcome.changed, 1);
        assert_eq!(outcome.remaining, 1);

        let head = head_tree(&repo).unwrap();
        assert_eq!(head.len(), 2);
    }

    #[test]
    fn ignored_files_are_not_committed() {
        let (_dir, repo) = repo();
        write(&repo, ".gitignore", b"*.log\n");
        write(&repo, "build.log", b"x");
        commit(&repo, Vec::new()).unwrap();
        let head = head_tree(&repo).unwrap();
        assert!(!head.contains_key(b"build.log".as_slice()));
        assert!(head.contains_key(b".gitignore".as_slice()));
    }

    #[test]
    fn explicit_ignored_path_is_rejected() {
        let (_dir, repo) = repo();
        write(&repo, ".gitignore", b"*.log\n");
        write(&repo, "build.log", b"x");
        let error = commit(&repo, vec![b"build.log".to_vec()]).unwrap_err();
        assert!(matches!(error, FodeltaError::PathIgnored { .. }));
    }

    #[test]
    fn unmatched_path_is_rejected() {
        let (_dir, repo) = repo();
        let error = commit(&repo, vec![b"missing.txt".to_vec()]).unwrap_err();
        assert!(matches!(error, FodeltaError::PathNotMatched { .. }));
    }

    #[test]
    fn empty_commit_requires_flag() {
        let (_dir, repo) = repo();
        commit(&repo, Vec::new()).unwrap_err();
        let outcome = create_commit(
            &repo,
            &CommitOptions {
                message: "empty".to_string(),
                allow_empty: true,
                allow_large_files: false,
                paths: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(outcome.changed, 0);
    }

    #[test]
    fn large_files_need_the_flag() {
        let (_dir, repo) = repo();
        let mut config = repo.config.clone();
        config.core.max_file_size_mb = 1;
        config.save(&repo.config_path()).unwrap();
        let repo = Repository::open(&repo.root).unwrap();
        write(&repo, "big.bin", &vec![0u8; 1024 * 1024 + 512 * 1024]);
        let error = commit(&repo, Vec::new()).unwrap_err();
        assert!(matches!(error, FodeltaError::FileTooLarge { .. }));

        let outcome = create_commit(
            &repo,
            &CommitOptions {
                message: "big".to_string(),
                allow_empty: false,
                allow_large_files: true,
                paths: Vec::new(),
            },
        );
        assert!(outcome.is_ok(), "{outcome:?}");
    }
}
