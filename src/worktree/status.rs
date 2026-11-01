//! Compute working tree status relative to HEAD.

use std::collections::BTreeSet;

use crate::Result;
use crate::history::head_tree;
use crate::repo::Repository;
use crate::worktree::scan;
use crate::worktree::stat_cache::StatCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Default)]
pub struct StatusReport {
    pub added: Vec<Vec<u8>>,
    pub modified: Vec<Vec<u8>>,
    pub deleted: Vec<Vec<u8>>,
    pub ignored: Vec<Vec<u8>>,
    pub warnings: Vec<String>,
    pub unborn: bool,
}

impl StatusReport {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    pub fn total_changes(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }
}

pub fn compute_status(repo: &Repository, include_ignored: bool) -> Result<StatusReport> {
    let head = head_tree(repo)?;
    let unborn = repo.head_oid()?.is_none();
    let tracked: BTreeSet<Vec<u8>> = head.keys().cloned().collect();
    let entries = scan::scan(&repo.root, &tracked)?;
    let cache = StatCache::load(&repo.fodelta.join("stat-cache"));
    let store = repo.store();

    let mut report = StatusReport {
        unborn,
        ..StatusReport::default()
    };
    let mut seen = BTreeSet::new();

    for entry in &entries {
        seen.insert(entry.path.clone());
        let oid = match cache.lookup(&entry.path, entry.stat) {
            Some(oid) => oid,
            None => scan::entry_oid(&store, &repo.root, entry)?,
        };
        match head.get(&entry.path) {
            None => report.added.push(entry.path.clone()),
            Some(info) if info.mode != entry.stat.mode || info.oid != oid => {
                report.modified.push(entry.path.clone());
            }
            Some(_) => {}
        }
    }

    report.deleted = head
        .keys()
        .filter(|path| !seen.contains(*path))
        .cloned()
        .collect();

    if include_ignored {
        let all = scan::scan_all(&repo.root)?;
        for entry in all {
            if !seen.contains(&entry.path) && !head.contains_key(&entry.path) {
                report.ignored.push(entry.path);
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{TreeEntryInfo, build_trees};
    use crate::odb::{Commit, MODE_FILE, ObjectKind};
    use crate::repo::{MAIN_REF, Repository, refs};
    use crate::util::path;

    fn seed(repo: &Repository, files: &[(&str, &[u8])]) {
        let store = repo.store();
        let mut map = crate::history::TreeMap::new();
        for (name, contents) in files {
            let path = repo.root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, contents).unwrap();
            let oid = store.write_blob(contents).unwrap();
            map.insert(
                name.as_bytes().to_vec(),
                TreeEntryInfo {
                    mode: MODE_FILE,
                    oid,
                },
            );
        }
        let tree = build_trees(&store, &map).unwrap();
        let commit = Commit::new(tree, None, repo.author().unwrap(), "seed".to_string());
        let oid = store
            .write(ObjectKind::Commit, &commit.encode().unwrap())
            .unwrap();
        refs::update_ref(&repo.fodelta, MAIN_REF, None, oid, false).unwrap();
    }

    #[test]
    fn classifies_added_modified_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        seed(&repo, &[("a.txt", b"a"), ("b.txt", b"b")]);

        std::fs::write(repo.root.join("a.txt"), b"a changed").unwrap();
        std::fs::remove_file(repo.root.join("b.txt")).unwrap();
        std::fs::write(repo.root.join("c.txt"), b"c").unwrap();

        let report = compute_status(&repo, false).unwrap();
        assert_eq!(report.added, vec![b"c.txt".to_vec()]);
        assert_eq!(report.modified, vec![b"a.txt".to_vec()]);
        assert_eq!(report.deleted, vec![b"b.txt".to_vec()]);
        assert_eq!(report.total_changes(), 3);
    }

    #[test]
    fn ignored_files_are_hidden_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(repo.root.join(".gitignore"), b"*.log\n").unwrap();
        std::fs::write(repo.root.join("build.log"), b"log").unwrap();

        let report = compute_status(&repo, false).unwrap();
        assert!(report.ignored.is_empty());
        assert_eq!(report.added, vec![b".gitignore".to_vec()]);

        let report = compute_status(&repo, true).unwrap();
        assert_eq!(report.ignored, vec![b"build.log".to_vec()]);
    }

    #[test]
    fn tracked_files_ignore_later_gitignore_rules() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        seed(&repo, &[("build.log", b"old")]);
        std::fs::write(repo.root.join(".gitignore"), b"*.log\n").unwrap();

        let report = compute_status(&repo, false).unwrap();
        assert!(report.added.contains(&b".gitignore".to_vec()));

        std::fs::write(repo.root.join("build.log"), b"new").unwrap();
        let report = compute_status(&repo, false).unwrap();
        assert_eq!(report.modified, vec![b"build.log".to_vec()]);
        assert!(path::display(&report.modified[0]).contains("build.log"));
    }
}
