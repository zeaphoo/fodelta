//! Append-only rollback: create a new commit whose tree equals a target
//! revision, then synchronize the working tree to it.

use crate::Result;
use crate::history::revparse::read_commit;
use crate::history::tree_map::{TreeMap, flatten_tree, head_tree};
use crate::odb::{Commit, ObjectId, ObjectKind};
use crate::repo::{MAIN_REF, Repository, refs};
use crate::worktree::restore::{clear_cache, overwrite_collisions, sync_worktree};

#[derive(Debug, Clone)]
pub struct RollbackOptions {
    pub target: ObjectId,
    pub message: Option<String>,
    pub no_safety: bool,
}

#[derive(Debug, Clone)]
pub struct RollbackOutcome {
    pub commit: Option<ObjectId>,
    pub safety: Option<ObjectId>,
    pub changed: usize,
    pub dirty: bool,
    pub safety_skipped: bool,
    pub synced: bool,
}

pub fn rollback(repo: &Repository, options: &RollbackOptions) -> Result<RollbackOutcome> {
    let _lock = repo.lock()?;
    let target_commit = read_commit(repo, options.target)?;
    let target_map = flatten_tree(&repo.store(), target_commit.tree)?;
    let head_oid = repo.head_oid()?;
    let head_map = head_tree(repo)?;
    let current = crate::diff::worktree_map(repo)?;
    let extras = overwrite_collisions(repo, &target_map, &[], &current)?;
    let dirty = current != head_map || !extras.is_empty();

    if target_map == head_map {
        if !dirty {
            return Ok(RollbackOutcome {
                commit: None,
                safety: None,
                changed: 0,
                dirty: false,
                safety_skipped: false,
                synced: false,
            });
        }
        let safety = if options.no_safety {
            None
        } else {
            crate::safety::create(repo, "before rollback", &extras)?
        };
        sync_worktree(repo, &target_map, &[], true)?;
        clear_cache(repo)?;
        return Ok(RollbackOutcome {
            commit: None,
            safety,
            changed: 0,
            dirty,
            safety_skipped: options.no_safety,
            synced: true,
        });
    }

    let safety = if dirty && !options.no_safety {
        crate::safety::create(repo, "before rollback", &extras)?
    } else {
        None
    };
    let message = options
        .message
        .clone()
        .unwrap_or_else(|| format!("rollback to {}", options.target.short()));
    let store = repo.store();
    let tree = target_commit.tree;
    let commit = Commit::new(tree, head_oid, repo.author()?, message);
    let payload = commit.encode()?;
    let oid = store.write(ObjectKind::Commit, &payload)?;
    refs::update_ref(
        &repo.fodelta,
        MAIN_REF,
        head_oid,
        oid,
        repo.config.core.fsync.fsync_refs(),
    )?;

    sync_worktree(repo, &target_map, &[], true)?;
    clear_cache(repo)?;

    Ok(RollbackOutcome {
        commit: Some(oid),
        safety,
        changed: count_changes(&head_map, &target_map),
        dirty,
        safety_skipped: dirty && options.no_safety,
        synced: true,
    })
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
