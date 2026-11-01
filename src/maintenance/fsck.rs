//! Integrity verification for commits, trees, blobs and references.

use std::collections::BTreeSet;

use crate::Result;
use crate::maintenance::reach::{collect_roots, mark_reachable};
use crate::odb::{Commit, ObjectId, ObjectKind, ObjectStore, Tree};
use crate::repo::Repository;

const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Default)]
pub struct FsckReport {
    pub checked: usize,
    pub dangling: usize,
    pub roots: usize,
    pub problems: Vec<String>,
}

impl FsckReport {
    pub fn is_healthy(&self) -> bool {
        self.problems.is_empty()
    }
}

pub fn fsck(repo: &Repository, full: bool) -> Result<FsckReport> {
    let roots = collect_roots(repo, None)?;
    let store = repo.store();
    let reachable = mark_reachable(&store, &roots).unwrap_or_default();

    let mut report = FsckReport {
        roots: roots.len(),
        ..FsckReport::default()
    };
    let mut seen = BTreeSet::new();

    for root in &roots {
        check_object(&store, *root, &mut seen, &mut report.problems, 0);
    }

    let all = store.iter_oids()?;
    if full {
        for oid in &all {
            check_object(&store, *oid, &mut seen, &mut report.problems, 0);
        }
    }

    report.checked = seen.len();
    report.dangling = all.iter().filter(|oid| !reachable.contains(oid)).count();
    Ok(report)
}

fn check_object(
    store: &ObjectStore,
    oid: ObjectId,
    seen: &mut BTreeSet<ObjectId>,
    problems: &mut Vec<String>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        problems.push(format!("{}: object graph is too deep", oid.short()));
        return;
    }
    if !seen.insert(oid) {
        return;
    }
    let (kind, payload) = match store.read(oid) {
        Ok(object) => object,
        Err(error) => {
            problems.push(format!("{}: {error}", oid.short()));
            return;
        }
    };
    match kind {
        ObjectKind::Commit => match Commit::decode(&payload) {
            Ok(commit) => {
                check_object(store, commit.tree, seen, problems, depth + 1);
                if let Some(parent) = commit.parent {
                    check_object(store, parent, seen, problems, depth + 1);
                }
            }
            Err(error) => problems.push(format!("{}: {error}", oid.short())),
        },
        ObjectKind::Tree => match Tree::decode(&payload) {
            Ok(tree) => {
                for entry in tree.entries() {
                    check_object(store, entry.oid, seen, problems, depth + 1);
                }
            }
            Err(error) => problems.push(format!("{}: {error}", oid.short())),
        },
        ObjectKind::Blob => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{CommitOptions, create_commit};

    #[test]
    fn healthy_repository_has_no_problems() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(repo.root.join("a.txt"), b"a").unwrap();
        create_commit(
            &repo,
            &CommitOptions {
                message: "seed".to_string(),
                allow_empty: false,
                allow_large_files: false,
                paths: Vec::new(),
            },
        )
        .unwrap();

        let report = fsck(&repo, false).unwrap();
        assert!(report.is_healthy(), "{:?}", report.problems);
        assert!(report.checked >= 3);
    }

    #[test]
    fn detects_corrupt_blob() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(repo.root.join("a.txt"), b"contents").unwrap();
        create_commit(
            &repo,
            &CommitOptions {
                message: "seed".to_string(),
                allow_empty: false,
                allow_large_files: false,
                paths: Vec::new(),
            },
        )
        .unwrap();

        let oid = repo.store().write_blob(b"contents").unwrap();
        let path = repo.store().path_for(oid);
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(path, bytes).unwrap();

        let report = fsck(&repo, false).unwrap();
        assert!(!report.is_healthy());
        assert!(
            report
                .problems
                .iter()
                .any(|problem| problem.contains("corrupt") || problem.contains("object"))
        );
    }

    #[test]
    fn reports_dangling_objects_without_failing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.store().write_blob(b"orphan").unwrap();
        let report = fsck(&repo, false).unwrap();
        assert!(report.is_healthy());
        assert_eq!(report.dangling, 1);
    }
}
