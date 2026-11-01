//! Safety snapshots: commits that preserve the working tree before a
//! destructive operation. They are recorded in `logs/safety`, not in refs.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Result;

use crate::history::tree_map::{TreeEntryInfo, TreeMap, build_trees, head_tree};
use crate::odb::ObjectStore;
use crate::odb::{Commit, ObjectId, ObjectKind};
use crate::repo::Repository;
use crate::util::atomic::write_atomic;
use crate::util::time;
use crate::worktree::scan::{self, EntryKind, WorktreeEntry};

pub const LOG_FILE: &str = "logs/safety";

#[derive(Debug, Clone)]
pub struct SafetyEntry {
    pub oid: ObjectId,
    pub timestamp: i64,
    pub offset_minutes: i32,
    pub reason: String,
}

/// Create a snapshot of the current working tree. Returns `None` when the
/// working tree already matches HEAD.
///
/// The caller must hold the repository lock.
pub fn create(repo: &Repository, reason: &str, extras: &[Vec<u8>]) -> Result<Option<ObjectId>> {
    let head = head_tree(repo)?;
    let tracked: std::collections::BTreeSet<Vec<u8>> = head.keys().cloned().collect();
    let entries = scan::scan(&repo.root, &tracked)?;
    let store = repo.store();
    let mut current = TreeMap::new();
    for entry in &entries {
        current.insert(
            entry.path.clone(),
            TreeEntryInfo {
                mode: entry.stat.mode,
                oid: write_object(&store, &repo.root, entry)?,
            },
        );
    }
    for extra in extras {
        if current.contains_key(extra) {
            continue;
        }
        let absolute = crate::util::path::join(&repo.root, extra);
        let entry = scan::entry_from_path(&absolute, extra.clone())?;
        current.insert(
            extra.clone(),
            TreeEntryInfo {
                mode: entry.stat.mode,
                oid: write_object(&store, &repo.root, &entry)?,
            },
        );
    }
    if current == head {
        return Ok(None);
    }

    let tree = build_trees(&store, &current)?;
    let author = repo.author()?;
    let parent = repo.head_oid()?;
    let message = format!("safety: {reason}");
    let commit = Commit::new(tree, parent, author, message);
    let payload = commit.encode()?;
    let oid = store.write(ObjectKind::Commit, &payload)?;
    append(repo, oid, reason)?;
    Ok(Some(oid))
}

fn write_object(store: &ObjectStore, root: &Path, entry: &WorktreeEntry) -> Result<ObjectId> {
    match entry.kind {
        EntryKind::Symlink => {
            let target = scan::symlink_target_bytes(&entry.absolute(root))?;
            store.write_blob(&target)
        }
        EntryKind::File => store.write_file(&entry.absolute(root)),
    }
}

pub fn append(repo: &Repository, oid: ObjectId, reason: &str) -> Result<()> {
    let path = repo.fodelta.join(LOG_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (timestamp, offset) = time::now();
    let reason = reason.replace(['\n', '\r', '\t'], " ");
    let line = format!(
        "{oid} {timestamp} {}	{reason}\n",
        time::format_offset(offset)
    );
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(line.as_bytes())?;
    if repo.config.core.fsync.fsync_refs() {
        file.sync_all()?;
    }
    Ok(())
}

pub fn list(repo: &Repository) -> Result<Vec<SafetyEntry>> {
    let path = repo.fodelta.join(LOG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut entries = Vec::new();
    let store = repo.store();
    for line in text.lines() {
        let Some((meta, reason)) = line.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (Some(oid), Some(timestamp), Some(offset)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(oid) = ObjectId::from_hex(oid) else {
            continue;
        };
        let Ok(timestamp) = timestamp.parse::<i64>() else {
            continue;
        };
        let Some(offset_minutes) = time::parse_offset(offset) else {
            continue;
        };
        if !store.exists(oid) {
            continue;
        }
        entries.push(SafetyEntry {
            oid,
            timestamp,
            offset_minutes,
            reason: reason.to_string(),
        });
    }
    entries.reverse();
    Ok(entries)
}

/// Drop safety records older than the retention window.
pub fn prune_log(repo: &Repository, retention_days: u64) -> Result<()> {
    let path = repo.fodelta.join(LOG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let cutoff = time::now()
        .0
        .saturating_sub((retention_days.saturating_mul(86_400)) as i64);
    let mut kept = String::new();
    for line in text.lines() {
        let Some((meta, _reason)) = line.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (Some(_oid), Some(timestamp)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(timestamp) = timestamp.parse::<i64>() else {
            continue;
        };
        if timestamp < cutoff {
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    if kept != text {
        write_atomic(&path, kept.as_bytes(), repo.config.core.fsync.fsync_refs())?;
    }
    Ok(())
}

pub fn log_path(repo: &Repository) -> PathBuf {
    repo.fodelta.join(LOG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repository;

    #[test]
    fn creates_none_for_clean_tree_and_snapshot_for_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(repo.root.join("a.txt"), b"a").unwrap();
        crate::history::create_commit(
            &repo,
            &crate::history::CommitOptions {
                message: "seed".to_string(),
                allow_empty: false,
                allow_large_files: false,
                paths: Vec::new(),
            },
        )
        .unwrap();

        assert!(create(&repo, "clean", &[]).unwrap().is_none());
        std::fs::write(repo.root.join("a.txt"), b"dirty").unwrap();
        let oid = create(&repo, "dirty", &[]).unwrap().expect("snapshot");
        assert!(repo.store().read(oid).is_ok());
        let entries = list(&repo).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].oid, oid);
        assert_eq!(entries[0].reason, "dirty");
    }
}
