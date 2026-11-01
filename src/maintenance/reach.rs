//! Reachability analysis shared by `gc` and `fsck`.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::odb::{Commit, ObjectId, ObjectKind, ObjectStore, Tree};
use crate::repo::Repository;
use crate::{FodeltaError, Result};

const MAX_DEPTH: usize = 256;

pub fn collect_roots(repo: &Repository, safety_days: Option<u64>) -> Result<Vec<ObjectId>> {
    let mut roots = Vec::new();
    if let Some(oid) = repo.head_oid()? {
        roots.push(oid);
    }
    let now = unix_now();
    for entry in crate::safety::list(repo)? {
        let keep = match safety_days {
            Some(days) => entry.timestamp >= now.saturating_sub((days * 86_400) as i64),
            None => true,
        };
        if keep {
            roots.push(entry.oid);
        }
    }
    Ok(roots)
}

pub fn mark_reachable(store: &ObjectStore, roots: &[ObjectId]) -> Result<BTreeSet<ObjectId>> {
    let mut seen = BTreeSet::new();
    for root in roots {
        mark_object(store, *root, &mut seen, 0)?;
    }
    Ok(seen)
}

fn mark_object(
    store: &ObjectStore,
    oid: ObjectId,
    seen: &mut BTreeSet<ObjectId>,
    depth: usize,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(FodeltaError::RepositoryCorrupt(
            "object graph is too deep".to_string(),
        ));
    }
    if !seen.insert(oid) {
        return Ok(());
    }
    let (kind, payload) = store.read(oid)?;
    match kind {
        ObjectKind::Commit => {
            let commit = Commit::decode(&payload)?;
            mark_object(store, commit.tree, seen, depth + 1)?;
            if let Some(parent) = commit.parent {
                mark_object(store, parent, seen, depth + 1)?;
            }
        }
        ObjectKind::Tree => {
            let tree = Tree::decode(&payload)?;
            for entry in tree.entries() {
                if entry.is_dir() {
                    mark_object(store, entry.oid, seen, depth + 1)?;
                } else {
                    seen.insert(entry.oid);
                }
            }
        }
        ObjectKind::Blob => {}
    }
    Ok(())
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
